//! lenscraft-core: "what am I looking at?"
//!
//! L1: photo in, ranked identifications with evidence out (local vision model).
//! L2: label-first mode, venue narrowing, Wikidata/Wikipedia grounding, 30s/3m/10m explanations.
//! L3/L5: sentence-streamed TTS and STT via speech-mcp + grounded follow-up Q&A.
//! L8: per-museum collection packs (KHM first) with nearest-neighbour matching.
pub mod collection;
pub mod error;
pub mod explain;
pub mod floorplan;
pub mod grounding;
pub mod identify;
pub mod image_prep;
pub mod museums;
pub mod qa;
pub mod speech;
pub mod venue;
pub mod vision;
pub mod worlds;

pub use collection::{
    build_wikidata_pack, compute_signature, hydrate_pack_signatures, load_pack, match_collection,
    save_pack, CollectionItem, CollectionMatch, CollectionPack, KHM_QID,
};
pub use error::LensError;
pub use explain::{explain, Explanation, ExplanationSection, FactBasis, LengthPreset};
pub use floorplan::{
    build_indoor_overpass_ql, curated_floorplan, fetch_floorplan_image_bytes,
    parse_overpass_floorplan, point_in_polygon, resolve_floorplan, room_at_point,
    search_commons_floorplan, FloorLevel, FloorRoom, MuseumFloorplan,
};
pub use grounding::{
    ground_result, ground_work, GroundingInfo, HttpKnowledgeClient, KnowledgeTransport, SourcedFact,
};
pub use identify::{
    answer_schema, identify, Basis, Candidate, Certainty, IdentifyMode, IdentifyOptions,
    IdentifyResult, ImageInfo, LabelMatch, Verdict,
};
pub use image_prep::{prepare_bytes, prepare_file, PreparedImage};
pub use museums::{find_museum, search_museums, MuseumEntry, MuseumSearchHit, WORLD_MUSEUMS};
pub use qa::{answer_followup, FollowupAnswer, QaTurn};
pub use speech::{
    split_sentences, synthesize_sentences, SpeechMcpHttp, SpeechTransport, SpokenChunk,
    DEFAULT_SPEECH_URL,
};
pub use venue::{
    build_overpass_ql, haversine_m, nearby_venues, OverpassHttp, VenueCandidate, VenueTransport,
    DEFAULT_OVERPASS_URL,
};
pub use vision::{status, ModelStatus, OllamaHttp, VisionTransport, DEFAULT_MODEL};
pub use worlds::{
    generate_relief_ply, WorldDb, WorldEntry, WorldKind, WorldStatus, DEFAULT_WORLD_DB_PATH,
};
