# Changelog - lenscraft

## 0.3.0 (unreleased) - World Museums, Floorplans & 3D Walk-Ins (M1-M5)
- `lenscraft-core`:
  - `museums.rs`: World Museum Directory (`WORLD_MUSEUMS`, 18 flagship art museums across Vienna and global capitals) with search and Haversine distance sorting.
  - `floorplan.rs`: Multi-level floorplan engine (`parse_overpass_floorplan` for OSM `indoor=room`, `curated_floorplan` for KHM/Louvre, `point_in_polygon` room hit-testing).
  - `worlds.rs`: Persistent 3D Walk-In Simulation DB (`worlds/worlds_db.json`, `WorldKind::Painting` / `GalleryRoom`) + pure-Rust `generate_relief_ply` generator.
- `lenscraft-mcp`:
  - `GET /api/museums`, `GET /api/museums/{slug}/floorplan`, `GET /api/pack/{slug}` for all 18 museums, `GET /api/worlds`, `GET /api/worlds/{qid}/splat`, `POST /api/reground`.
  - Mobile PWA (`mobile_ui.html`): **Museums** directory + interactive SVG floorplan + **3D Walk-In** `<canvas>` viewer + **Fix Work** correction flow.
  - Prep scripts: `scripts/prep-world-museums.ps1`, `scripts/prep-iconic-worlds.ps1`.
- `lenscraft-app`:
  - `Museums` menu + interactive **World Museums & Floorplans** window with `egui` room polygon painter and 3D walk-in status.

## 0.2.0 (unreleased) - lens steps L2 to L8
- `lenscraft-core`:
  - L2: `IdentifyMode::Label` label-first prompt path, `venue` Overpass GPS narrowing
    (`nearby_venues`, Haversine distance), `grounding` Wikidata + Wikipedia lookup
    (`ground_work`, `ground_result`, `SourcedFact`), and `explain` long-form museum guide
    in `30s` / `3m` / `10m` lengths with verified `[sourced]` / `[visual]` / `[model]`
    attribution.
  - L3/L5: `speech` sentence-streamed TTS (`split_sentences`, `synthesize_sentences`) and
    STT (`transcribe_file`) via `speech-mcp` (`http://127.0.0.1:10909`), plus `qa`
    grounded follow-up Q&A (`answer_followup`).
  - L8: `collection` per-museum spatial color signature + text pack (`build_wikidata_pack`
    for KHM `Q95569`, `match_collection`, `save_pack`/`load_pack`).
- `lenscraft-mcp`:
  - CLI subcommands: `venues`, `ground`, `explain`, `speak`, `pack-build`, `serve`, plus
    `identify --mode auto|artwork|label`.
  - MCP tools: `lens_venues`, `lens_ground`, `lens_explain`, `lens_speak`, `lens_pack`.
  - L4/L5 HTTP `lens-server` on port `11265` with mobile web UI (camera, GPS, compass
    heading, `30s`/`3m`/`10m` guide, sentence-streamed TTS, push-to-talk mic, barge-in)
    and `scripts/tailscale-serve.ps1`.

## 0.1.0 (unreleased) - lens step L1
- `lenscraft-core`: photo preparation (EXIF rotation, downscale to 1536 px, JPEG; HEIC/AVIF
  refused with a clear message; decompression-bomb check before decode), Ollama vision
  transport with capability check, identify with a JSON-schema answer and a 1500-token
  output cap, one retry with a repetition penalty when the model loops, deterministic check
  of candidates against text read in the photo, verdict `identified` / `likely` /
  `uncertain` with its basis. No numeric confidence. `identified` requires text in the
  photo that names the title; recognition alone is at most `likely` (measured: about one
  confidently wrong title in five on a famous painting).
- `lenscraft-mcp`: CLI `identify` and `status`, MCP stdio server with `lens_identify` and
  `lens_status`; failures set `isError`, CLI exits 1.