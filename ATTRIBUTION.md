# Attribution

LensCraft is built in Safe Rust (`#![forbid(unsafe_code)]`) using:
- `eframe` / `egui` (MIT / Apache-2.0) - native GPU desktop UI
- `reqwest`, `axum`, `tokio` (MIT / Apache-2.0) - HTTP client and mobile server
- `serde`, `serde_json`, `clap`, `thiserror`, `image`, `base64`, `rfd` (MIT / Apache-2.0)

External data services queried on user request:
- **OpenStreetMap Overpass API** (ODbL) for nearby cultural venues
- **Wikidata** (CC0) and **Wikipedia** (CC BY-SA) for artwork grounding and collection packs
- **Ollama** (local) and **speech-mcp** (local) for vision, TTS, and STT
