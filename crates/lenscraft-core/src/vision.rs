//! The vision model behind the lens. Ollama today; the `VisionTransport` trait keeps
//! tests offline and leaves room for another backend.
use crate::error::LensError;
use serde::Serialize;
use serde_json::{json, Value};
use std::time::Duration;

/// Default vision model. On Goliath it reports vision and audio capability.
pub const DEFAULT_MODEL: &str = "gemma4:12b";
/// Default Ollama endpoint. Override with `LENSCRAFT_OLLAMA_URL`.
pub const DEFAULT_OLLAMA_URL: &str = "http://127.0.0.1:11434";
/// One identify on a 12B model takes seconds; a cold model load can take a minute.
/// Five minutes is the outer bound before the model counts as offline.
const CHAT_TIMEOUT: Duration = Duration::from_secs(300);
const QUICK_TIMEOUT: Duration = Duration::from_secs(10);

pub trait VisionTransport {
    fn base_url(&self) -> String;
    /// `POST /api/show` for one model.
    fn show(&self, model: &str) -> Result<Value, LensError>;
    /// `POST /api/chat`, non-streaming.
    fn chat(&self, body: &Value) -> Result<Value, LensError>;
    /// `GET /api/version`.
    fn version(&self) -> Result<String, LensError>;
}

pub struct OllamaHttp {
    base: String,
    quick: reqwest::blocking::Client,
    slow: reqwest::blocking::Client,
}

impl OllamaHttp {
    /// `LENSCRAFT_OLLAMA_URL` or the local default.
    pub fn from_env() -> Result<Self, LensError> {
        let base = std::env::var("LENSCRAFT_OLLAMA_URL")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| DEFAULT_OLLAMA_URL.to_string());
        Self::new(&base)
    }

    pub fn new(base: &str) -> Result<Self, LensError> {
        let base = base.trim().trim_end_matches('/').to_string();
        if !(base.starts_with("http://") || base.starts_with("https://")) {
            return Err(LensError::InvalidInput(format!(
                "ollama url must start with http:// or https://, got '{base}'"
            )));
        }
        let client = |timeout: Duration| {
            reqwest::blocking::Client::builder()
                .connect_timeout(QUICK_TIMEOUT)
                .timeout(timeout)
                .build()
                .map_err(|e| LensError::Offline(format!("http client: {e}")))
        };
        Ok(Self {
            quick: client(QUICK_TIMEOUT)?,
            slow: client(CHAT_TIMEOUT)?,
            base,
        })
    }

    fn finish(
        url: &str,
        sent: Result<reqwest::blocking::Response, reqwest::Error>,
    ) -> Result<Value, LensError> {
        let resp = sent.map_err(|e| {
            if e.is_timeout() {
                LensError::Offline(format!("ollama timed out at {url}: {e}"))
            } else {
                LensError::Offline(format!("ollama unreachable at {url}: {e}"))
            }
        })?;
        let status = resp.status();
        let text = resp
            .text()
            .map_err(|e| LensError::Protocol(format!("ollama read failed: {e}")))?;
        if !status.is_success() {
            let reason = serde_json::from_str::<Value>(&text)
                .ok()
                .and_then(|v| v.get("error").and_then(Value::as_str).map(str::to_string))
                .unwrap_or_else(|| text.chars().take(300).collect());
            return Err(if status.as_u16() == 404 {
                LensError::Model(format!("ollama {status}: {reason}"))
            } else {
                LensError::Protocol(format!("ollama {status}: {reason}"))
            });
        }
        serde_json::from_str(&text)
            .map_err(|e| LensError::Protocol(format!("ollama sent invalid json: {e}")))
    }
}

impl VisionTransport for OllamaHttp {
    fn base_url(&self) -> String {
        self.base.clone()
    }

    fn show(&self, model: &str) -> Result<Value, LensError> {
        let url = format!("{}/api/show", self.base);
        let sent = self
            .quick
            .post(&url)
            .json(&json!({ "model": model }))
            .send();
        Self::finish(&url, sent)
    }

    fn chat(&self, body: &Value) -> Result<Value, LensError> {
        let url = format!("{}/api/chat", self.base);
        let sent = self.slow.post(&url).json(body).send();
        Self::finish(&url, sent)
    }

    fn version(&self) -> Result<String, LensError> {
        let url = format!("{}/api/version", self.base);
        let sent = self.quick.get(&url).send();
        let v = Self::finish(&url, sent)?;
        v.get("version")
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| LensError::Protocol("ollama /api/version has no version".to_string()))
    }
}

/// Model names: Ollama tags only, nothing that could smuggle a path or a URL.
pub fn validate_model(name: &str) -> Result<String, LensError> {
    let n = name.trim();
    if n.is_empty() {
        return Err(LensError::InvalidInput(
            "model must not be empty".to_string(),
        ));
    }
    if n.chars().count() > 128 {
        return Err(LensError::InvalidInput(
            "model name exceeds 128 characters".to_string(),
        ));
    }
    if !n
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | ':' | '/' | '-'))
    {
        return Err(LensError::InvalidInput(format!(
            "model name has characters outside A-Z a-z 0-9 . _ : / -: '{n}'"
        )));
    }
    if n.contains("//") || n.starts_with('/') {
        return Err(LensError::InvalidInput(format!(
            "model name looks like a path or URL, not an Ollama tag: '{n}'"
        )));
    }
    Ok(n.to_string())
}

/// The capability list Ollama reports for a model ("vision", "thinking", ...).
pub fn capabilities(t: &dyn VisionTransport, model: &str) -> Result<Vec<String>, LensError> {
    let v = t.show(model)?;
    Ok(v.get("capabilities")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default())
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct ModelStatus {
    /// True when an identify call would reach a model that can see.
    pub ready: bool,
    pub ollama_url: String,
    pub ollama_version: Option<String>,
    pub model: String,
    pub capabilities: Vec<String>,
    /// Why `ready` is false.
    pub problem: Option<String>,
}

/// Diagnose the lens backend. The diagnosis itself succeeds; `ready` and `problem`
/// say whether an identify would work and, if not, why. Only a bad model name is
/// an error.
pub fn status(t: &dyn VisionTransport, model: &str) -> Result<ModelStatus, LensError> {
    let model = validate_model(model)?;
    let mut s = ModelStatus {
        ollama_url: t.base_url(),
        model: model.clone(),
        ..ModelStatus::default()
    };
    match t.version() {
        Ok(v) => s.ollama_version = Some(v),
        Err(e) => {
            s.problem = Some(e.to_string());
            return Ok(s);
        }
    }
    match capabilities(t, &model) {
        Ok(caps) => {
            if caps.iter().any(|c| c == "vision") {
                s.ready = true;
            } else {
                s.problem = Some(format!("model '{model}' has no vision capability"));
            }
            s.capabilities = caps;
        }
        Err(e) => s.problem = Some(e.to_string()),
    }
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_names_are_fenced() {
        assert_eq!(validate_model(" gemma4:12b "), Ok("gemma4:12b".to_string()));
        assert!(validate_model("hf.co/org/Model-GGUF:Q4_K_M").is_ok());
        for bad in ["", "   ", "a b", "x;rm", "http://evil", &"m".repeat(129)] {
            assert_eq!(
                validate_model(bad).map_err(|e| e.kind()),
                Err("invalid"),
                "{bad}"
            );
        }
    }

    #[test]
    fn ollama_url_needs_a_scheme() {
        assert_eq!(
            OllamaHttp::new("127.0.0.1:11434")
                .map(|_| ())
                .map_err(|e| e.kind()),
            Err("invalid")
        );
        assert!(OllamaHttp::new("http://127.0.0.1:11434/").is_ok());
    }

    #[test]
    fn unreachable_backend_is_reported_not_hidden() -> Result<(), LensError> {
        // Port 9 (discard) is closed on a normal machine: connection refused, fast.
        let t = OllamaHttp::new("http://127.0.0.1:9")?;
        let s = status(&t, "gemma4:12b")?;
        assert!(!s.ready);
        assert!(s.problem.unwrap_or_default().contains("unreachable"));
        Ok(())
    }
}
