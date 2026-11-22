# lenscraft

What am I looking at? Point a camera at a painting, sculpture or building and get ranked
identifications with the evidence for each, from a vision model running on your own machine.

Status: **0.3.0, L1-L8 + Museum Platform M1-M5** (`lenscraft-core`, `lenscraft-mcp`, `lenscraft-app` desktop UI, iPhone PWA; L6 in `travelcraft-core/src/lens.rs`; L7 in `giscraft-core/src/bearing_lens.rs`). See [docs/PRD.md](docs/PRD.md), [docs/GUIDE.md](docs/GUIDE.md), [HELP.md](HELP.md), and `D:\Dev\repos\giscraft\docs\TIERED_PLAN.md`.

## Feature Matrix

| Capability | Status | Notes |
|---|---|---|
| L1 Photo identify (`lens_identify`) | Implemented | Ollama vision (`gemma4:12b`), EXIF rotate, label verification |
| L2 Label-first, venues, Wikidata/Wikipedia grounding, 30s/3m/10m guide | Implemented | `lens_venues`, `lens_ground`, `lens_explain`, `--mode label` |
| L3 Sentence-streamed TTS | Implemented | `speech-mcp` (`:10909`) `/api/v1/tts/wav` + browser fallback |
| L4 `lens-server` + iPhone PWA | Implemented | Port `11265`, `/manifest.webmanifest`, `/sw.js`, `scripts/tailscale-serve.ps1` |
| L5 Speech-in, follow-ups, barge-in | Implemented | `speech-mcp` STT + `qa::answer_followup` (accepts corrections) |
| L6 `travelcraft` SightLens + notebook | Partial | `travelcraft-core/src/lens.rs` supports L2/L5; egui tab wiring open |
| L7 `giscraft` bearing lens | Implemented | `giscraft-core/src/bearing_lens.rs` ray-cast vs OSM polygons |
| L8 Collection packs (18 world museums) | Implemented | `DESC(?sitelinks)` SPARQL + `--with-signatures` 24-dim color match |
| M1 World Museum Directory | Implemented | `museums.rs` (18 Vienna + global flagships), `GET /api/museums` |
| M2 Multi-Level Floorplans | Implemented | `floorplan.rs` (OSM indoor + KHM/Louvre schematics), `GET /api/museums/{slug}/floorplan` |
| M3 3D Walk-In DB & Viewer | Implemented | `worlds.rs` (`worlds/worlds_db.json`), relief `.ply` + `splatcraft` `.spz`, in-browser 3D `<canvas>` |
| L3 Desktop App (`lenscraft-app`) | Implemented | `egui 0.37` Bookend Menu Bar, 10-item Help suite, Command Palette, Floorplan & Light/Dark UI |

## How it decides
1. The photo is EXIF-rotated, scaled to at most 1536 px and sent as JPEG to a local Ollama
   vision model (default `gemma4:12b`) with a JSON schema it must answer in.
2. The model transcribes any readable text (wall label, plaque, signature), describes what
   is visible, and lists up to three candidates (title, creator, date) with high/medium/low
   certainty and evidence. It is told to return no candidates rather than invent one.
3. lenscraft checks each candidate against the transcribed text and ranks candidates the
   text names first. `basis` says what the answer rests on: `label` (text in the photo names
   the title or the artist), `recognition` (the model alone), or `none`.
4. `verdict`:
   - `identified`: text in the photo names the **title** (and the model agrees),
   - `likely`: the model recognised it, or the text names only the artist,
   - `uncertain`: nothing specific.
   There are no numeric confidence scores.

## What live testing showed (2026-11-14, gemma4:12b on an RTX 4090)
- About 3 s per photo with the model loaded; 15-20 s including a cold model load.
- Recognition alone is not reliable enough to call "identified": on a very well-known
  painting the model gave a confidently wrong title in about one run in five. Hence the
  rule above: photograph the wall label with the work and the label becomes the evidence.
- Photos full of text (maps, long labels) push the model into a repetition loop on most
  first attempts. lenscraft caps the output and retries once with a repetition penalty;
  the result says so in `notes`. A penalty on the first attempt was tried and rejected: it
  stopped the loops but also stopped label transcription and worsened titles.
- Prompt wording matters: adding field definitions to the prompt turned 5/6 correct answers
  into 0/3. The museum location is therefore not asked of the model; it will come from
  grounding (Wikidata) in step L2.
- Another workload holding the GPU can starve the lens; Ollama then fails or queues, and
  lenscraft reports that as an error.

Known limits: label matching is word-based, so a German label and an English title only match
on shared words (names usually match). HEIC photos are refused with a clear message; send JPEG.

## Use
```powershell
cargo run -p lenscraft-mcp -- status
cargo run -p lenscraft-mcp -- identify photo.jpg --place "Kunsthistorisches Museum Wien"
cargo run -p lenscraft-mcp -- identify photo.jpg --language de --json
cargo run -p lenscraft-mcp -- mcp      # stdio server: lens_identify, lens_status
```

## License
MIT OR Apache-2.0