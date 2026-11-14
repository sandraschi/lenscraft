//! lenscraft-core: "what am I looking at?"
//!
//! L1 of the lens. A photo goes in; ranked identifications with the evidence for
//! each come out, from a local vision model (Ollama). There are no canned answers:
//! a subject the model cannot name is reported as uncertain, and every failure is
//! a typed `LensError`.
pub mod error;
pub mod identify;
pub mod image_prep;
pub mod vision;

pub use error::LensError;
pub use identify::{
    answer_schema, identify, Basis, Candidate, Certainty, IdentifyOptions, IdentifyResult,
    ImageInfo, LabelMatch, Verdict,
};
pub use image_prep::{prepare_bytes, prepare_file, PreparedImage};
pub use vision::{status, ModelStatus, OllamaHttp, VisionTransport, DEFAULT_MODEL};
