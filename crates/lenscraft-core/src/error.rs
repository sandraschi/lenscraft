//! Typed errors. `kind()` is the stable machine-readable tag the MCP layer reports.
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum LensError {
    /// Caller gave bad input: path, hint, coordinates, model name.
    #[error("invalid input: {0}")]
    InvalidInput(String),
    /// Reading a file failed.
    #[error("io error: {0}")]
    Io(String),
    /// The bytes are not an image the lens can use.
    #[error("image error: {0}")]
    Image(String),
    /// The model server is not reachable or timed out.
    #[error("vision model offline: {0}")]
    Offline(String),
    /// The model server answered, but not in the expected shape.
    #[error("protocol error: {0}")]
    Protocol(String),
    /// The model is missing or cannot do the job (for example, no vision).
    #[error("model error: {0}")]
    Model(String),
}

impl LensError {
    pub fn kind(&self) -> &'static str {
        match self {
            LensError::InvalidInput(_) => "invalid",
            LensError::Io(_) => "io",
            LensError::Image(_) => "image",
            LensError::Offline(_) => "offline",
            LensError::Protocol(_) => "protocol",
            LensError::Model(_) => "model",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::LensError;

    #[test]
    fn kinds_are_stable_and_messages_keep_context() {
        let e = LensError::Offline("ollama at :11434".to_string());
        assert_eq!(e.kind(), "offline");
        assert!(e.to_string().contains(":11434"));
        assert_eq!(LensError::Model(String::new()).kind(), "model");
        assert_eq!(LensError::InvalidInput(String::new()).kind(), "invalid");
    }
}
