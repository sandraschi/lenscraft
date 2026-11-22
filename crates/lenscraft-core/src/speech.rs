//! L3 & L5 speech layer: sentence splitting + TTS/STT via speech-mcp (:10909).
use crate::error::LensError;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;

pub const DEFAULT_SPEECH_URL: &str = "http://127.0.0.1:10909";
const TTS_TIMEOUT: Duration = Duration::from_secs(30);

/// Split prose into TTS-ready sentences without breaking common abbreviations.
pub fn split_sentences(text: &str) -> Vec<String> {
    let cleaned = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if cleaned.is_empty() {
        return Vec::new();
    }
    const ABBREVS: &[&str] = &[
        "dr.", "st.", "ca.", "nr.", "mr.", "mrs.", "ms.", "v.", "d.", "ä.", "fig.",
    ];
    let mut sentences = Vec::new();
    let mut cur = String::new();
    let tokens: Vec<&str> = cleaned.split(' ').collect();
    for (i, tok) in tokens.iter().enumerate() {
        if !cur.is_empty() {
            cur.push(' ');
        }
        cur.push_str(tok);
        let ends_punct = tok.ends_with('.') || tok.ends_with('!') || tok.ends_with('?');
        if !ends_punct {
            continue;
        }
        let lc = tok
            .trim_end_matches(|c: char| !c.is_alphabetic() && c != '.')
            .to_lowercase();
        let is_abbrev = ABBREVS.contains(&lc.as_str()) || (tok.len() <= 2 && tok.ends_with('.'));
        let is_last = i + 1 == tokens.len();
        if !is_abbrev || is_last {
            let s = cur.trim().to_string();
            if !s.is_empty() {
                sentences.push(s);
            }
            cur.clear();
        }
    }
    let rem = cur.trim().to_string();
    if !rem.is_empty() {
        sentences.push(rem);
    }
    sentences
}

pub trait SpeechTransport {
    fn base_url(&self) -> String;
    fn tts_wav(&self, text: &str, provider: &str, voice_id: &str) -> Result<Vec<u8>, LensError>;
    fn transcribe_file(
        &self,
        audio_bytes: &[u8],
        filename: &str,
        language: &str,
    ) -> Result<String, LensError>;
}

pub struct SpeechMcpHttp {
    base: String,
    client: reqwest::blocking::Client,
}

impl SpeechMcpHttp {
    pub fn from_env() -> Result<Self, LensError> {
        let base = std::env::var("LENSCRAFT_SPEECH_URL")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| DEFAULT_SPEECH_URL.to_string());
        Self::new(&base)
    }

    pub fn new(base: &str) -> Result<Self, LensError> {
        let base = base.trim().trim_end_matches('/').to_string();
        if !(base.starts_with("http://") || base.starts_with("https://")) {
            return Err(LensError::InvalidInput(format!(
                "speech-mcp url must start with http:// or https://, got '{base}'"
            )));
        }
        let client = reqwest::blocking::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(TTS_TIMEOUT)
            .build()
            .map_err(|e| LensError::Offline(format!("speech client: {e}")))?;
        Ok(Self { base, client })
    }
}

impl SpeechTransport for SpeechMcpHttp {
    fn base_url(&self) -> String {
        self.base.clone()
    }

    fn tts_wav(&self, text: &str, provider: &str, voice_id: &str) -> Result<Vec<u8>, LensError> {
        let text = text.trim();
        if text.is_empty() {
            return Err(LensError::InvalidInput(
                "tts text must not be empty".to_string(),
            ));
        }
        let url = format!("{}/api/v1/tts/wav", self.base);
        let resp = self
            .client
            .get(&url)
            .query(&[
                ("text", text),
                ("provider", provider),
                ("voice_id", voice_id),
            ])
            .send()
            .map_err(|e| LensError::Offline(format!("speech-mcp unreachable at {url}: {e}")))?;
        let status = resp.status();
        let bytes = resp
            .bytes()
            .map_err(|e| LensError::Protocol(format!("speech-mcp read failed: {e}")))?;
        if !status.is_success() {
            let msg = String::from_utf8_lossy(&bytes);
            return Err(LensError::Protocol(format!(
                "speech-mcp {status}: {}",
                msg.chars().take(200).collect::<String>()
            )));
        }
        Ok(bytes.to_vec())
    }

    fn transcribe_file(
        &self,
        audio_bytes: &[u8],
        filename: &str,
        language: &str,
    ) -> Result<String, LensError> {
        if audio_bytes.is_empty() {
            return Err(LensError::InvalidInput(
                "audio bytes must not be empty".to_string(),
            ));
        }
        let url = format!("{}/api/v1/transcribe/file", self.base);
        let part = reqwest::blocking::multipart::Part::bytes(audio_bytes.to_vec())
            .file_name(filename.to_string());
        let form = reqwest::blocking::multipart::Form::new()
            .part("file", part)
            .text("language", language.to_string())
            .text("format", "json".to_string())
            .text("provider", "funasr".to_string());
        let resp = self
            .client
            .post(&url)
            .multipart(form)
            .send()
            .map_err(|e| LensError::Offline(format!("speech-mcp transcribe failed: {e}")))?;
        let status = resp.status();
        let text = resp
            .text()
            .map_err(|e| LensError::Protocol(format!("speech-mcp transcribe read failed: {e}")))?;
        if !status.is_success() {
            return Err(LensError::Protocol(format!(
                "speech-mcp transcribe {status}: {}",
                text.chars().take(200).collect::<String>()
            )));
        }
        let v: Value = serde_json::from_str(&text)
            .map_err(|e| LensError::Protocol(format!("speech-mcp transcribe invalid json: {e}")))?;
        Ok(v.get("text")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .to_string())
    }
}

impl SpeechMcpHttp {
    /// Synthesize and play `text` on the host speaker via `POST /api/v1/tts`.
    pub fn speak_aloud(&self, text: &str, provider: &str, voice_id: &str) -> Result<(), LensError> {
        let text = text.trim();
        if text.is_empty() {
            return Err(LensError::InvalidInput(
                "tts text must not be empty".to_string(),
            ));
        }
        let url = format!("{}/api/v1/tts", self.base);
        let resp = self
            .client
            .post(&url)
            .json(&serde_json::json!({
                "text": text,
                "provider": provider,
                "voice_id": voice_id,
            }))
            .send()
            .map_err(|e| LensError::Offline(format!("speech-mcp unreachable at {url}: {e}")))?;
        let status = resp.status();
        if !status.is_success() {
            let msg = resp.text().unwrap_or_default();
            return Err(LensError::Protocol(format!(
                "speech-mcp {status}: {}",
                msg.chars().take(200).collect::<String>()
            )));
        }
        Ok(())
    }

    /// Stop active host playback via `POST /api/v1/stop`.
    pub fn stop_aloud(&self) -> Result<(), LensError> {
        let url = format!("{}/api/v1/stop", self.base);
        let _ = self
            .client
            .post(&url)
            .send()
            .map_err(|e| LensError::Offline(format!("speech-mcp unreachable at {url}: {e}")))?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpokenChunk {
    pub index: usize,
    pub sentence: String,
    pub wav_bytes: usize,
}

/// Synthesize text sentence-by-sentence, returning ordered WAV buffers.
pub fn synthesize_sentences(
    t: &dyn SpeechTransport,
    text: &str,
    provider: &str,
    voice_id: &str,
) -> Result<Vec<(String, Vec<u8>)>, LensError> {
    let sentences = split_sentences(text);
    if sentences.is_empty() {
        return Err(LensError::InvalidInput("nothing to speak".to_string()));
    }
    let mut out = Vec::with_capacity(sentences.len());
    for s in sentences {
        let wav = t.tts_wav(&s, provider, voice_id)?;
        out.push((s, wav));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeSpeech;
    impl SpeechTransport for FakeSpeech {
        fn base_url(&self) -> String {
            "fake".to_string()
        }
        fn tts_wav(
            &self,
            text: &str,
            _provider: &str,
            _voice_id: &str,
        ) -> Result<Vec<u8>, LensError> {
            Ok(text.as_bytes().to_vec())
        }
        fn transcribe_file(
            &self,
            _bytes: &[u8],
            _name: &str,
            _lang: &str,
        ) -> Result<String, LensError> {
            Ok("Who painted the small figures?".to_string())
        }
    }

    #[test]
    fn split_sentences_preserves_abbreviations() {
        let s = split_sentences(
            "Pieter Bruegel d. Ä. painted this ca. 1563. Look at the harbour! Is it Antwerp?",
        );
        assert_eq!(s.len(), 3);
        assert!(s[0].contains("ca. 1563."));
    }

    #[test]
    fn synthesize_sentences_streams_per_sentence() -> Result<(), LensError> {
        let chunks = synthesize_sentences(
            &FakeSpeech,
            "First sentence. Second sentence.",
            "kokoro",
            "default",
        )?;
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].0, "First sentence.");
        assert_eq!(chunks[1].0, "Second sentence.");
        Ok(())
    }
}
