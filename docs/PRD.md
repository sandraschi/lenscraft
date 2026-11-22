# Product Requirements Document (PRD) - LensCraft

**Version:** 0.3.0
**Status:** Active (L1-L8 + Museum Platform M1-M5)
**Repository:** `D:\Dev\repos\lenscraft` (`https://github.com/sandraschi/lenscraft`)

## 1. Product Vision
LensCraft answers *"What am I looking at?"* for museum visitors, travelers, and researchers using local-first AI (Ollama vision), verified cultural knowledge (Wikidata, Wikipedia, OpenStreetMap), and immersive 3D walk-in previews (`splatcraft` Marble `.spz` and local relief `.ply`), without inventing confidence numbers or fabricating provenance.

## 2. Core Capabilities
| Track | Scope | Implementation |
|---|---|---|
| **L1 Identify** | EXIF-rotate, <=1536 px JPEG, Ollama vision (`gemma4:12b`), label verification | `lenscraft-core/src/identify.rs`, `image_prep.rs`, `vision.rs` |
| **L2 Ground & Explain** | Label-first mode, Overpass GPS venues, Wikidata/Wikipedia grounding, `30s`/`3m`/`10m` guides with `[sourced]`/`[visual]`/`[model]` tags | `venue.rs`, `grounding.rs`, `explain.rs` |
| **L3/L5 Speech & Q&A** | Sentence-streamed TTS & FunASR STT via `speech-mcp` (`:10909`), grounded follow-up Q&A with visitor correction acceptance, browser `speechSynthesis` fallback, barge-in | `speech.rs`, `qa.rs`, `server.rs` |
| **L4 Mobile PWA** | `axum` server on port `11265` behind `tailscale serve`, iOS/GPD standalone PWA, on-device canvas downscale, visit log export | `crates/lenscraft-mcp/src/server.rs`, `mobile_ui.html` |
| **L6/L7 Sibling Hooks** | `travelcraft` SightLens + Universal Notebook (`travelcraft-core/src/lens.rs`); `giscraft` compass bearing ray-cast (`giscraft-core/src/bearing_lens.rs`) | Sibling repos |
| **L8 Collection Packs** | `DESC(?sitelinks)` Wikidata SPARQL packs (250-500 works), overnight `--with-signatures` 24-dim color matching | `collection.rs`, `scripts/prep-world-museums.ps1` |
| **M1 World Museums** | 18 flagship art museums (6 Vienna + 12 global) with search & Haversine distance sorting | `museums.rs` |
| **M2 Floorplans** | Overpass `indoor=room`/`level=*` parser, `[0..1]` normalizer, 2D polygon hit-testing, curated KHM/Louvre schematics | `floorplan.rs` |
| **M3 3D Walk-Ins** | Persistent `worlds/worlds_db.json` DB (`Painting` + `GalleryRoom`), `.ply` relief generator, `splatcraft` `.spz` integration, in-browser 3D `<canvas>` walk-in viewer | `worlds.rs`, `scripts/prep-iconic-worlds.ps1` |
| **L3 Desktop App** | Pure-Rust `egui 0.37` / `eframe` workstation with Bookend Menu Bar, 10-item Help suite, Command Palette, Light/Dark high-contrast themes, `Ctrl+Wheel` zoom, `glyph_allowlist` guard | `crates/lenscraft-app/` |

## 3. Non-Goals & Honesty Rules
- No fabricated numeric confidence scores; recognition alone never exceeds `likely`.
- Unverified `[sourced]` claims from the LLM are downgraded to `[model]`.
- No `#![forbid(unsafe_code)]` exceptions; `unwrap_used`, `expect_used`, `panic`, `todo`, `unimplemented` are denied workspace-wide.
