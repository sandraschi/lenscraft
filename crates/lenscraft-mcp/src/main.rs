//! lenscraft-mcp: CLI and MCP stdio server over lenscraft-core.
//!
//! stdout carries only results (CLI) or JSON-RPC (mcp). Diagnostics go to stderr.
//! A failed tool call is an error payload with `isError: true`; a failed CLI call
//! exits 1.
mod server;

use clap::{Parser, Subcommand};
use lenscraft_core::{
    build_wikidata_pack, explain, generate_relief_ply, ground_work, identify, nearby_venues,
    prepare_file, save_pack, status, synthesize_sentences, HttpKnowledgeClient, IdentifyMode,
    IdentifyOptions, IdentifyResult, LengthPreset, LensError, OllamaHttp, OverpassHttp,
    SpeechMcpHttp, WorldDb, WorldEntry, WorldStatus, DEFAULT_MODEL, DEFAULT_WORLD_DB_PATH, KHM_QID,
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
        /// Identification mode: auto, artwork, label
        #[arg(long, default_value = "auto")]
        mode: String,
        /// Print JSON instead of text
        #[arg(long)]
        json: bool,
    },
    /// Find nearby cultural venues from GPS coordinates
    Venues {
        #[arg(long, allow_hyphen_values = true)]
        lat: f64,
        #[arg(long, allow_hyphen_values = true)]
        lon: f64,
        #[arg(long, default_value_t = 300)]
        radius_m: u32,
        #[arg(long)]
        json: bool,
    },
    /// Ground an artwork title against Wikidata and Wikipedia
    Ground {
        title: String,
        #[arg(long)]
        creator: Option<String>,
        #[arg(long, default_value = "en")]
        language: String,
        #[arg(long)]
        json: bool,
    },
    /// Identify, ground, and produce a 30s/3m/10m museum guide explanation
    Explain {
        path: PathBuf,
        #[arg(long, default_value = "3m")]
        length: String,
        #[arg(long)]
        place: Option<String>,
        #[arg(long, default_value = "en")]
        language: String,
        #[arg(long, default_value = DEFAULT_MODEL)]
        model: String,
        #[arg(long)]
        json: bool,
    },
    /// Synthesize speech WAV chunks via speech-mcp (:10909)
    Speak {
        text: String,
        #[arg(long, default_value = "windows")]
        provider: String,
        #[arg(long, default_value = "default")]
        voice: String,
        #[arg(long)]
        out_dir: Option<PathBuf>,
        #[arg(long)]
        json: bool,
    },
    /// Build a museum collection pack from Wikidata (default: KHM Q95569)
    PackBuild {
        #[arg(long, default_value = "Kunsthistorisches Museum Wien")]
        museum: String,
        #[arg(long, default_value = KHM_QID)]
        qid: String,
        #[arg(long, default_value_t = 250)]
        limit: usize,
        #[arg(long)]
        with_signatures: bool,
        #[arg(long)]
        out: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// List ready 3D walk-in worlds in the DB
    WorldList,
    /// Generate a local 3D relief PLY from a painting photo and register it in worlds_db.json
    WorldRelief {
        #[arg(long)]
        qid: String,
        #[arg(long)]
        title: String,
        #[arg(long, default_value = "")]
        creator: String,
        #[arg(long, default_value = "")]
        museum: String,
        #[arg(long)]
        image: PathBuf,
        #[arg(long)]
        out: PathBuf,
        #[arg(long, default_value = "")]
        prompt: String,
    },
    /// Run the HTTP lens-server + mobile web UI (default port 11265)
    Serve {
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        #[arg(long, default_value_t = server::DEFAULT_SERVER_PORT)]
        port: u16,
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

fn run_venues(lat: f64, lon: f64, radius_m: Option<u32>) -> Value {
    let op = "lens_venues";
    let t = match OverpassHttp::from_env() {
        Ok(t) => t,
        Err(e) => return err_value(op, &e),
    };
    match nearby_venues(&t, lat, lon, radius_m) {
        Ok(v) => to_payload(op, &json!({"venues": v})),
        Err(e) => err_value(op, &e),
    }
}

fn run_ground(title: &str, creator: Option<&str>, language: &str) -> Value {
    let op = "lens_ground";
    let t = match HttpKnowledgeClient::new() {
        Ok(t) => t,
        Err(e) => return err_value(op, &e),
    };
    match ground_work(&t, title, creator, language) {
        Ok(g) => to_payload(op, &json!({"grounding": g})),
        Err(e) => err_value(op, &e),
    }
}

fn run_explain(
    path: &Path,
    length: &str,
    place: Option<String>,
    language: &str,
    model: &str,
) -> Value {
    let op = "lens_explain";
    let preset = match LengthPreset::parse(length) {
        Ok(p) => p,
        Err(e) => return err_value(op, &e),
    };
    let transport = match OllamaHttp::from_env() {
        Ok(t) => t,
        Err(e) => return err_value(op, &e),
    };
    let img = match prepare_file(path) {
        Ok(i) => i,
        Err(e) => return err_value(op, &e),
    };
    let opts = IdentifyOptions {
        model: model.to_string(),
        place,
        language: Some(language.to_string()),
        ..IdentifyOptions::default()
    };
    let id_res = match identify(&transport, &img, &opts) {
        Ok(r) => r,
        Err(e) => return err_value(op, &e),
    };
    let grounding = HttpKnowledgeClient::new().ok().and_then(|kc| {
        lenscraft_core::ground_result(&kc, &id_res, language)
            .ok()
            .flatten()
    });
    match explain(
        &transport,
        &id_res,
        grounding.as_ref(),
        preset,
        language,
        model,
    ) {
        Ok(exp) => to_payload(
            op,
            &json!({"identify": id_res, "grounding": grounding, "explanation": exp}),
        ),
        Err(e) => err_value(op, &e),
    }
}

fn run_speak(text: &str, provider: &str, voice: &str, out_dir: Option<&Path>) -> Value {
    let op = "lens_speak";
    let speech = match SpeechMcpHttp::from_env() {
        Ok(s) => s,
        Err(e) => return err_value(op, &e),
    };
    let chunks = match synthesize_sentences(&speech, text, provider, voice) {
        Ok(c) => c,
        Err(e) => return err_value(op, &e),
    };
    let mut files = Vec::new();
    if let Some(dir) = out_dir {
        if let Err(e) = std::fs::create_dir_all(dir) {
            return err_value(op, &LensError::Io(format!("mkdir failed: {e}")));
        }
        for (i, (_, wav)) in chunks.iter().enumerate() {
            let p = dir.join(format!("chunk_{i:03}.wav"));
            if let Err(e) = std::fs::write(&p, wav) {
                return err_value(op, &LensError::Io(format!("write wav failed: {e}")));
            }
            files.push(p.display().to_string());
        }
    }
    to_payload(
        op,
        &json!({
            "chunks": chunks.len(),
            "total_wav_bytes": chunks.iter().map(|(_, b)| b.len()).sum::<usize>(),
            "files": files,
        }),
    )
}

fn run_pack_build(
    museum: &str,
    qid: &str,
    limit: usize,
    with_signatures: bool,
    out: &Path,
) -> Value {
    let op = "lens_pack";
    let kc = match HttpKnowledgeClient::new() {
        Ok(k) => k,
        Err(e) => return err_value(op, &e),
    };
    let mut pack = match build_wikidata_pack(&kc, museum, qid, limit) {
        Ok(p) => p,
        Err(e) => return err_value(op, &e),
    };
    let hydrated = if with_signatures {
        match lenscraft_core::hydrate_pack_signatures(&mut pack) {
            Ok(n) => n,
            Err(e) => return err_value(op, &e),
        }
    } else {
        0
    };
    if let Err(e) = save_pack(&pack, out) {
        return err_value(op, &e);
    }
    to_payload(
        op,
        &json!({
            "museum": pack.museum,
            "qid": pack.qid,
            "items": pack.items.len(),
            "hydrated_signatures": hydrated,
            "path": out.display().to_string(),
        }),
    )
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
        },
        {
            "name": "lens_venues",
            "description": "Resolve GPS coordinates (lat, lon) to nearby museums, galleries, monuments, and historic buildings via Overpass.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "lat": {"type": "number"},
                    "lon": {"type": "number"},
                    "radius_m": {"type": "integer"}
                },
                "required": ["lat", "lon"]
            }
        },
        {
            "name": "lens_ground",
            "description": "Ground an artwork title and optional creator against Wikidata and Wikipedia.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "title": {"type": "string"},
                    "creator": {"type": "string"},
                    "language": {"type": "string"}
                },
                "required": ["title"]
            }
        },
        {
            "name": "lens_explain",
            "description": "Identify a photo, ground it in Wikidata/Wikipedia, and generate a 30s/3m/10m museum guide with sourced facts marked.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": {"type": "string"},
                    "length": {"type": "string", "description": "30s, 3m, or 10m"},
                    "place": {"type": "string"},
                    "language": {"type": "string"},
                    "model": {"type": "string"}
                },
                "required": ["path"]
            }
        },
        {
            "name": "lens_speak",
            "description": "Synthesize spoken text sentence-by-sentence via speech-mcp (:10909).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "text": {"type": "string"},
                    "provider": {"type": "string"},
                    "voice": {"type": "string"},
                    "out_dir": {"type": "string"}
                },
                "required": ["text"]
            }
        },
        {
            "name": "lens_pack",
            "description": "Build a museum collection pack from Wikidata SPARQL (default KHM Q95569) and save to JSON.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "museum": {"type": "string"},
                    "qid": {"type": "string"},
                    "limit": {"type": "integer"},
                    "out": {"type": "string"}
                },
                "required": ["out"]
            }
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

fn parse_mode(raw: Option<&str>) -> Result<IdentifyMode, String> {
    match raw.map(str::trim).map(str::to_lowercase).as_deref() {
        None | Some("") | Some("auto") => Ok(IdentifyMode::Auto),
        Some("artwork") => Ok(IdentifyMode::Artwork),
        Some("label") => Ok(IdentifyMode::Label),
        Some(other) => Err(format!(
            "mode must be auto, artwork, or label; got '{other}'"
        )),
    }
}

fn identify_args(args: &Value) -> Result<(String, IdentifyOptions), String> {
    let path = str_arg(args, "path")?
        .filter(|p| !p.trim().is_empty())
        .ok_or_else(|| "path is required".to_string())?;
    let mode = parse_mode(str_arg(args, "mode")?.as_deref())?;
    Ok((
        path,
        IdentifyOptions {
            model: str_arg(args, "model")?.unwrap_or_default(),
            hint: str_arg(args, "hint")?,
            place: str_arg(args, "place")?,
            lat: num_arg(args, "lat")?,
            lon: num_arg(args, "lon")?,
            language: str_arg(args, "language")?,
            mode,
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
        "lens_venues" => match (
            num_arg(args, "lat"),
            num_arg(args, "lon"),
            num_arg(args, "radius_m"),
        ) {
            (Ok(Some(lat)), Ok(Some(lon)), Ok(rad)) => {
                run_venues(lat, lon, rad.map(|r| r.max(0.0) as u32))
            }
            (Ok(_), Ok(_), Ok(_)) => invalid("lens_venues", "lat and lon are required".to_string()),
            (Err(e), _, _) | (_, Err(e), _) | (_, _, Err(e)) => invalid("lens_venues", e),
        },
        "lens_ground" => match (
            str_arg(args, "title"),
            str_arg(args, "creator"),
            str_arg(args, "language"),
        ) {
            (Ok(Some(t)), Ok(c), Ok(l)) => {
                run_ground(&t, c.as_deref(), l.as_deref().unwrap_or("en"))
            }
            (Ok(None), _, _) => invalid("lens_ground", "title is required".to_string()),
            (Err(e), _, _) | (_, Err(e), _) | (_, _, Err(e)) => invalid("lens_ground", e),
        },
        "lens_explain" => match (
            str_arg(args, "path"),
            str_arg(args, "length"),
            str_arg(args, "place"),
            str_arg(args, "language"),
            str_arg(args, "model"),
        ) {
            (Ok(Some(p)), Ok(len), Ok(pl), Ok(lang), Ok(m)) => run_explain(
                Path::new(&p),
                len.as_deref().unwrap_or("3m"),
                pl,
                lang.as_deref().unwrap_or("en"),
                m.as_deref().unwrap_or(DEFAULT_MODEL),
            ),
            (Ok(None), _, _, _, _) => invalid("lens_explain", "path is required".to_string()),
            (Err(e), _, _, _, _)
            | (_, Err(e), _, _, _)
            | (_, _, Err(e), _, _)
            | (_, _, _, Err(e), _)
            | (_, _, _, _, Err(e)) => invalid("lens_explain", e),
        },
        "lens_speak" => match (
            str_arg(args, "text"),
            str_arg(args, "provider"),
            str_arg(args, "voice"),
            str_arg(args, "out_dir"),
        ) {
            (Ok(Some(t)), Ok(p), Ok(v), Ok(o)) => run_speak(
                &t,
                p.as_deref().unwrap_or("windows"),
                v.as_deref().unwrap_or("default"),
                o.as_deref().map(Path::new),
            ),
            (Ok(None), _, _, _) => invalid("lens_speak", "text is required".to_string()),
            (Err(e), _, _, _) | (_, Err(e), _, _) | (_, _, Err(e), _) | (_, _, _, Err(e)) => {
                invalid("lens_speak", e)
            }
        },
        "lens_pack" => match (
            str_arg(args, "museum"),
            str_arg(args, "qid"),
            num_arg(args, "limit"),
            str_arg(args, "out"),
        ) {
            (Ok(m), Ok(q), Ok(l), Ok(Some(o))) => run_pack_build(
                m.as_deref().unwrap_or("Kunsthistorisches Museum Wien"),
                q.as_deref().unwrap_or(KHM_QID),
                l.map(|x| x.max(1.0) as usize).unwrap_or(250),
                false,
                Path::new(&o),
            ),
            (_, _, _, Ok(None)) => invalid("lens_pack", "out is required".to_string()),
            (Err(e), _, _, _) | (_, Err(e), _, _) | (_, _, Err(e), _) | (_, _, _, Err(e)) => {
                invalid("lens_pack", e)
            }
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
            "protocolVersion": "2026-07-28",
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
            mode,
            json,
        } => {
            let parsed_mode = match parse_mode(Some(&mode)) {
                Ok(m) => m,
                Err(e) => {
                    eprintln!("error: {e}");
                    std::process::exit(1);
                }
            };
            let opts = IdentifyOptions {
                model,
                hint,
                place,
                lat,
                lon,
                language,
                mode: parsed_mode,
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
        Cmd::Venues {
            lat,
            lon,
            radius_m,
            json: _,
        } => {
            let v = run_venues(lat, lon, Some(radius_m));
            if reports_error(&v) {
                emit_error(&v, true)
            } else {
                println!("{v:#}");
                0
            }
        }
        Cmd::Ground {
            title,
            creator,
            language,
            json: _,
        } => {
            let v = run_ground(&title, creator.as_deref(), &language);
            if reports_error(&v) {
                emit_error(&v, true)
            } else {
                println!("{v:#}");
                0
            }
        }
        Cmd::Explain {
            path,
            length,
            place,
            language,
            model,
            json: _,
        } => {
            let v = run_explain(&path, &length, place, &language, &model);
            if reports_error(&v) {
                emit_error(&v, true)
            } else {
                println!("{v:#}");
                0
            }
        }
        Cmd::Speak {
            text,
            provider,
            voice,
            out_dir,
            json: _,
        } => {
            let v = run_speak(&text, &provider, &voice, out_dir.as_deref());
            if reports_error(&v) {
                emit_error(&v, true)
            } else {
                println!("{v:#}");
                0
            }
        }
        Cmd::PackBuild {
            museum,
            qid,
            limit,
            with_signatures,
            out,
            json: _,
        } => {
            let v = run_pack_build(&museum, &qid, limit, with_signatures, &out);
            if reports_error(&v) {
                emit_error(&v, true)
            } else {
                println!("{v:#}");
                0
            }
        }
        Cmd::WorldList => match WorldDb::load(Path::new(DEFAULT_WORLD_DB_PATH)) {
            Ok(db) => {
                println!("{}", serde_json::json!({"worlds": db.list_ready()}));
                0
            }
            Err(e) => emit_error(&err_value("lens_worlds", &e), true),
        },
        Cmd::WorldRelief {
            qid,
            title,
            creator,
            museum,
            image,
            out,
            prompt,
        } => {
            let res: Result<Value, LensError> = (|| {
                let prep = prepare_file(&image)?;
                let pts = generate_relief_ply(&prep, &out, 0.45)?;
                let db_path = Path::new(DEFAULT_WORLD_DB_PATH);
                let mut db = WorldDb::load(db_path)?;
                let entry = WorldEntry {
                    qid,
                    title,
                    creator,
                    museum,
                    kind: lenscraft_core::WorldKind::Painting,
                    museum_slug: None,
                    room_id: None,
                    splat_path: out.display().to_string(),
                    format: "ply".to_string(),
                    source: "lenscraft-relief".to_string(),
                    prompt,
                    status: WorldStatus::Ready,
                };
                db.upsert(entry.clone());
                db.save(db_path)?;
                Ok(json!({"world": entry, "vertices": pts}))
            })();
            match res {
                Ok(v) => {
                    println!("{v:#}");
                    0
                }
                Err(e) => emit_error(&err_value("lens_worlds", &e), true),
            }
        }
        Cmd::Serve { host, port } => match server::run_server(&host, port) {
            Ok(()) => 0,
            Err(e) => {
                eprintln!("error: {e}");
                1
            }
        },
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
            Some("2026-07-28")
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
        assert_eq!(
            names,
            vec![
                "lens_identify",
                "lens_status",
                "lens_venues",
                "lens_ground",
                "lens_explain",
                "lens_speak",
                "lens_pack"
            ]
        );
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
