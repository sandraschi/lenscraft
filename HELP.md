# LensCraft Help & User Guide

## 1. Get Started
1. Start Ollama with a vision model (`gemma4:12b` default) and optionally start `speech-mcp` (`:10909`) for TTS/STT.
2. Launch the desktop workstation (`cargo run -p lenscraft-app`) or mobile server (`cargo run -p lenscraft-mcp -- serve`).
3. Open a photo (`Ctrl+O` in GUI, or Take/Pick Photo on mobile).
4. Optionally supply a venue name or GPS coordinates (`Lat`/`Lon`) and click **Venues** to narrow location context.
5. Run **Identify (F5)** or **Identify Wall Label (F6)**, then **Ground (F7)** against Wikidata/Wikipedia and **3m Guide (F8)**.

## 2. Functions
- **Photo Preparation (`image_prep`):** Reads JPEG/PNG/WebP, applies EXIF orientation, downscales to at most 1536 px, and rejects HEIC/AVIF or corrupted inputs with typed `LensError` messages.
- **Identification (`identify`):** Structured JSON vision call with automatic repetition-loop retry and deterministic label verification (`identified` requires label confirmation of the title; recognition alone caps at `likely`).
- **Venue Narrowing (`venue`):** Queries Overpass (OpenStreetMap) around `(lat, lon)` for museums, galleries, monuments, and historic buildings sorted by Haversine distance.
- **Grounding (`grounding`):** Searches Wikidata (`wbsearchentities` + `wbgetentities`) and Wikipedia REST summaries, returning `SourcedFact` entries.
- **Museum Guide (`explain`):** Generates `30s`, `3m`, or `10m` spoken prose divided into sections tagged `[sourced]`, `[visual]`, or `[model]`.
- **Speech & Follow-up Q&A (`speech`, `qa`):** Sentence-streamed TTS (`/api/v1/tts/wav`) and FunASR STT (`/api/v1/transcribe/file`) via `speech-mcp` (`:10909`), plus grounded multi-turn follow-up answers.
- **Collection Packs (`collection`):** Builds per-museum Wikidata SPARQL packs (KHM `Q95569` default) with 24-dim spatial color signature + text nearest-neighbour matching.

## 3. Usage (GUI, CLI & MCP)
### Desktop GUI (`lenscraft-app`)
- Full Bookend Menu Bar (`File`, `Edit`, `View`, `Lens`, `Packs`, `Window`, `Help`).
- `Ctrl+K` / `Ctrl+Shift+P` opens the Command Palette; `F1` / `Ctrl+/` opens Keyboard Shortcuts.
- `Ctrl+MouseWheel` or `Ctrl+=`/`-`/`0` zooms the UI; click the Sun/Moon button for Dark/Light high-contrast themes.

### CLI (`lenscraft-mcp`)
```powershell
lenscraft-mcp status --json
lenscraft-mcp identify photo.jpg --mode label --place "Kunsthistorisches Museum Wien" --json
lenscraft-mcp venues --lat 48.2039 --lon 16.3617 --radius-m 300
lenscraft-mcp ground "The Tower of Babel" --creator Bruegel
lenscraft-mcp explain photo.jpg --length 3m --json
lenscraft-mcp speak "Pieter Bruegel painted The Tower of Babel in 1563."
lenscraft-mcp pack-build --out khm-pack.json --limit 100
lenscraft-mcp serve --port 11265
```

### MCP (`lenscraft-mcp mcp`)
Pure JSON-RPC over `stdio` exposing: `lens_identify`, `lens_status`, `lens_venues`, `lens_ground`, `lens_explain`, `lens_speak`, `lens_pack`.

## 4. Tips & Tricks
- Photograph the wall label alongside the artwork whenever possible: label agreement upgrades the verdict from `likely` to `identified`.
- Use `--mode label` (`F6`) for dense plaques, inscriptions, or maps.
- For mobile use in museums, run `lenscraft-mcp serve` and `scripts/tailscale-serve.ps1` so iPhone Safari gets HTTPS access to camera, GPS, compass, and microphone.

## 5. Common Pitfalls & Safety
- Recognition alone misidentified famous works in ~1 in 5 live tests, so LensCraft never marks recognition-only answers as `identified`.
- Unverified `[sourced]` claims from the LLM are automatically downgraded to `[model]` by `explain::verify_basis`.
- HEIC/AVIF photos are refused with an actionable error; use JPEG/PNG/WebP.

## 6. Planned Extensions
- Live end-to-end iPhone Safari field verification over Tailscale serve.
- Full egui SightLens tab polish in `travelcraft-app`.
- Optional dense CLIP/SigLIP image embeddings for L8 museum collection packs.

## 7. Other Crafts
- **travelcraft:** Consumes `lenscraft-core` in `travelcraft-core/src/lens.rs` for cartridge cross-matching, L2/L5 guide/Q&A, and Universal Notebook logging.
- **giscraft:** Implements L7 `bearing_lens.rs` (position + compass heading ray-cast against OSM building footprints).
- **speech-mcp (`:10909`):** Provides sentence-streamed TTS and FunASR STT.
