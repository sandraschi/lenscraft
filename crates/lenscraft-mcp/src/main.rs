//! lenscraft-mcp: CLI and MCP stdio server over lenscraft-core.
//!
//! stdout carries only results (CLI) or JSON-RPC (mcp). Diagnostics go to stderr.
//! A failed tool call is an error payload with `isError: true`; a failed CLI call
//! exits 1.
use clap::{Parser, Subcommand};
use lenscraft_core::{
    identify, prepare_file, status, IdentifyOptions, IdentifyResult, LensError, OllamaHttp,
    DEFAULT_MODEL,
};
use serde_json::{json, Value};
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(
    name = "lenscraft-mcp",
    version,
    about = "Lens: what am I looking at? Photo in, ranked identifications with evidence out, from a local vision model."
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Identify what a photo shows
    Identify {
        /// Photo file: JPEG, PNG or WebP
        path: PathBuf,
        /// Your own note, e.g. "KHM, Bruegel room"
        #[arg(long)]
        hint: Option<String>,
        /// Venue, e.g. "Kunsthistorisches Museum Wien"
        #[arg(long)]
        place: Option<String>,
        #[arg(long, allow_hyphen_values = true)]
        lat: Option<f64>,
        #[arg(long, allow_hyphen_values = true)]
        lon: Option<f64>,
        /// Ollama vision model
        #[arg(long, default_value = DEFAULT_MODEL)]
        model: String,
        /// Answer language: en, de, ja, ...
        #[arg(long)]
        language: Option<String>,
        /// Print JSON instead of text
        #[arg(long)]
        json: bool,
    },
    /// Check Ollama and the vision model (exit 1 when not ready)
    Status {
        #[arg(long, default_value = DEFAULT_MODEL)]
        model: String,
        #[arg(long)]
        json: bool,
    },
    /// Run the MCP stdio server
    Mcp,
}

fn err_value(op: &str, e: &LensError) -> Value {
    json!({"error": e.to_string(), "kind": e.kind(), "op": op})
}

fn invalid(op: &str, msg: String) -> Value {
    err_value(op, &LensError::InvalidInput(msg))
}

fn reports_error(v: &Value) -> bool {
    v.get("error").is_some_and(|e| !e.is_null())
}

fn to_payload<T: serde::Serialize>(op: &str, v: &T) -> Value {
    serde_json::to_value(v)
        .unwrap_or_else(|e| err_value(op, &LensError::Protocol(format!("serialize failed: {e}"))))
}

fn run_identify(path: &Path, opts: &IdentifyOptions) -> Value {
    let op = "lens_identify";
    let transport = match OllamaHttp::from_env() {
        Ok(t) => t,
        Err(e) => return err_value(op, &e),
    };
    let img = match prepare_file(path) {
        Ok(i) => i,
        Err(e) => return err_value(op, &e),
    };
    match identify(&transport, &img, opts) {
        Ok(r) => to_payload(op, &r),
        Err(e) => err_value(op, &e),
    }
}

fn run_status(model: &str) -> Value {
    let op = "lens_status";
    let transport = match OllamaHttp::from_env() {
        Ok(t) => t,
        Err(e) => return err_value(op, &e),
    };
    match status(&transport, model) {
        Ok(s) => to_payload(op, &s),
        Err(e) => err_value(op, &e),
    }
}

/// The serde tag of an enum value ("identified", "label", "high").
fn tag<T: serde::Serialize>(v: &T) -> String {
    serde_json::to_value(v)
        .ok()
        .and_then(|x| x.as_str().map(str::to_string))
        .unwrap_or_default()
}

fn print_identify(v: &Value) {
    let Ok(r) = serde_json::from_value::<IdentifyResult>(v.clone()) else {
        println!("{v:#}");
        return;
    };
    println!("verdict: {} (basis: {})", tag(&r.verdict), tag(&r.basis));
    if r.candidates.is_empty() {
        println!("no specific identification");
    }
    for (i, c) in r.candidates.iter().enumerate() {
        let parts: Vec<&str> = [c.title.as_str(), c.creator.as_str(), c.date.as_str()]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect();
        println!("{}. {} [{}]", i + 1, parts.join(" | "), tag(&c.certainty));
        for e in &c.evidence {
            println!("   - {e}");
        }
    }
    if !r.label_text.is_empty() {
        println!("label: {}", r.label_text);
    }
    println!("seen: {}", r.description);
    for n in &r.notes {
        println!("note: {n}");
    }
    println!(
        "{} {:.1} s, sent {}x{} from {}x{} {}",
        r.model,
        r.elapsed_ms as f64 / 1000.0,
        r.image.width,
        r.image.height,
        r.image.source_width,
        r.image.source_height,
        r.image.source_format
    );
}

fn print_status(v: &Value) {
    let Ok(s) = serde_json::from_value::<StatusView>(v.clone()) else {
        println!("{v:#}");
        return;
    };
    println!("ready: {}", s.ready);
    println!(
        "ollama: {} ({})",
        s.ollama_url,
        s.ollama_version.unwrap_or_else(|| "no answer".to_string())
    );
    println!("model: {} [{}]", s.model, s.capabilities.join(", "));
    if let Some(p) = s.problem {
        println!("problem: {p}");
    }
}

/// Read-side twin of `ModelStatus` (which only derives Serialize).
#[derive(serde::Deserialize)]
struct StatusView {
    ready: bool,
    ollama_url: String,
    ollama_version: Option<String>,
    model: String,
    capabilities: Vec<String>,
    problem: Option<String>,
}

fn emit_error(v: &Value, as_json: bool) -> i32 {
    if as_json {
        println!("{v}");
    } else {
        eprintln!(
            "error: {}",
            v.get("error")
                .and_then(Value::as_str)
                .unwrap_or("unknown error")
        );
    }
    1
}

fn tools_list() -> Value {
    json!([
        {
            "name": "lens_identify",
            "description": "Identify what a photo shows (painting, sculpture, building, object). Photo file on this machine in; ranked candidates with evidence out, from a local vision model. verdict is identified, likely or uncertain; basis says whether it rests on the transcribed wall label or on recognition alone. No numeric confidence is invented.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "Photo file path: JPEG, PNG or WebP"},
                    "hint": {"type": "string", "description": "Visitor's note, e.g. 'KHM, Bruegel room'"},
                    "place": {"type": "string", "description": "Venue name"},
                    "lat": {"type": "number"},
                    "lon": {"type": "number"},
                    "model": {"type": "string", "description": "Ollama vision model; default gemma4:12b"},
                    "language": {"type": "string", "description": "Answer language code: en, de, ja, ..."}
                },
                "required": ["path"]
            }
        },
        {
            "name": "lens_status",
            "description": "Check the lens backend: Ollama reachable, model present, vision capability. ready=false comes with a problem text.",
            "inputSchema": {"type": "object", "properties": {"model": {"type": "string"}}}
        }
    ])
}

fn str_arg(args: &Value, key: &str) -> Result<Option<String>, String> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.clone())),
        Some(_) => Err(format!("{key} must be a string")),
    }
}

fn num_arg(args: &Value, key: &str) -> Result<Option<f64>, String> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(n)) => n
            .as_f64()
            .map(Some)
            .ok_or_else(|| format!("{key} is not a usable number")),
        Some(_) => Err(format!("{key} must be a number")),
    }
}

fn identify_args(args: &Value) -> Result<(String, IdentifyOptions), String> {
    let path = str_arg(args, "path")?
        .filter(|p| !p.trim().is_empty())
        .ok_or_else(|| "path is required".to_string())?;
    Ok((
        path,
        IdentifyOptions {
            model: str_arg(args, "model")?.unwrap_or_default(),
            hint: str_arg(args, "hint")?,
            place: str_arg(args, "place")?,
            lat: num_arg(args, "lat")?,
            lon: num_arg(args, "lon")?,
            language: str_arg(args, "language")?,
        },
    ))
}

fn call_tool(name: &str, args: &Value) -> Value {
    let payload = match name {
        "lens_identify" => match identify_args(args) {
            Ok((path, opts)) => run_identify(Path::new(&path), &opts),
            Err(e) => invalid("lens_identify", e),
        },
        "lens_status" => match str_arg(args, "model") {
            Ok(m) => run_status(m.as_deref().unwrap_or(DEFAULT_MODEL)),
            Err(e) => invalid("lens_status", e),
        },
        other => invalid(other, format!("unknown tool: {other}")),
    };
    let failed = reports_error(&payload);
    let mut result = json!({"content": [{"type": "text", "text": payload.to_string()}]});
    if failed {
        if let Some(o) = result.as_object_mut() {
            o.insert("isError".to_string(), Value::Bool(true));
        }
    }
    result
}

/// One JSON-RPC message in, at most one reply out. Notifications get no reply.
fn handle(msg: &Value) -> Option<Value> {
    let id = msg.get("id")?.clone();
    let method = msg.get("method").and_then(Value::as_str).unwrap_or("");
    let params = msg.get("params").cloned().unwrap_or(Value::Null);
    let result = match method {
        "initialize" => Ok(json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {"tools": {}},
            "serverInfo": {"name": "lenscraft", "version": env!("CARGO_PKG_VERSION")}
        })),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({"tools": tools_list()})),
        "tools/call" => {
            let name = params.get("name").and_then(Value::as_str).unwrap_or("");
            let args = params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            Ok(call_tool(name, &args))
        }
        other => Err((-32601, format!("method not found: {other}"))),
    };
    Some(match result {
        Ok(r) => json!({"jsonrpc": "2.0", "id": id, "result": r}),
        Err((code, message)) => {
            json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
        }
    })
}

fn serve() -> i32 {
    eprintln!("lenscraft-mcp {} on stdio", env!("CARGO_PKG_VERSION"));
    let stdin = std::io::stdin();
    let mut out = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(e) => {
                eprintln!("stdin read failed: {e}");
                return 1;
            }
        };
        let text = line.trim();
        if text.is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Value>(text) {
            Ok(msg) => handle(&msg),
            Err(e) => Some(
                json!({"jsonrpc": "2.0", "id": Value::Null, "error": {"code": -32700, "message": format!("parse error: {e}")}}),
            ),
        };
        if let Some(r) = reply {
            if writeln!(out, "{r}").and_then(|()| out.flush()).is_err() {
                return 1;
            }
        }
    }
    0
}

fn main() {
    let cli = Cli::parse();
    let code = match cli.cmd {
        Cmd::Identify {
            path,
            hint,
            place,
            lat,
            lon,
            model,
            language,
            json,
        } => {
            let opts = IdentifyOptions {
                model,
                hint,
                place,
                lat,
                lon,
                language,
            };
            let v = run_identify(&path, &opts);
            if reports_error(&v) {
                emit_error(&v, json)
            } else {
                if json {
                    println!("{v}");
                } else {
                    print_identify(&v);
                }
                0
            }
        }
        Cmd::Status { model, json } => {
            let v = run_status(&model);
            if reports_error(&v) {
                emit_error(&v, json)
            } else {
                if json {
                    println!("{v}");
                } else {
                    print_status(&v);
                }
                i32::from(v.get("ready").and_then(Value::as_bool) != Some(true))
            }
        }
        Cmd::Mcp => serve(),
    };
    let _ = std::io::stdout().flush();
    std::process::exit(code);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(result: &Value) -> String {
        result
            .pointer("/content/0/text")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string()
    }

    #[test]
    fn initialize_and_notifications() {
        let init =
            handle(&json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}}))
                .unwrap_or_default();
        assert_eq!(
            init.pointer("/result/protocolVersion")
                .and_then(Value::as_str),
            Some("2024-11-05")
        );
        assert!(
            handle(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"})).is_none()
        );
        let unknown =
            handle(&json!({"jsonrpc": "2.0", "id": 2, "method": "nope"})).unwrap_or_default();
        assert_eq!(
            unknown.pointer("/error/code").and_then(Value::as_i64),
            Some(-32601)
        );
    }

    #[test]
    fn tool_names_are_client_safe() {
        let tools = tools_list();
        let names: Vec<&str> = tools
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|t| t.get("name").and_then(Value::as_str))
                    .collect()
            })
            .unwrap_or_default();
        assert_eq!(names, vec!["lens_identify", "lens_status"]);
        assert!(names.iter().all(|n| n
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')));
    }

    #[test]
    fn failures_carry_is_error_and_a_kind() {
        let unknown = call_tool("lens_nope", &json!({}));
        assert_eq!(unknown.get("isError"), Some(&Value::Bool(true)));
        let no_path = call_tool("lens_identify", &json!({}));
        assert_eq!(no_path.get("isError"), Some(&Value::Bool(true)));
        assert!(text(&no_path).contains("\"kind\":\"invalid\""));
        let missing = call_tool(
            "lens_identify",
            &json!({"path": "definitely-not-here-xyz.jpg"}),
        );
        assert_eq!(missing.get("isError"), Some(&Value::Bool(true)));
        let bad_lat = call_tool("lens_identify", &json!({"path": "x.jpg", "lat": "48.2"}));
        assert!(text(&bad_lat).contains("lat must be a number"));
        let bad_model = call_tool("lens_status", &json!({"model": 5}));
        assert_eq!(bad_model.get("isError"), Some(&Value::Bool(true)));
    }

    #[test]
    fn null_error_is_not_a_failure() {
        assert!(!reports_error(&json!({"error": null, "ready": true})));
        assert!(reports_error(&json!({"error": "x"})));
    }
}
