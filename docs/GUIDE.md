# LensCraft Architecture & Operations Guide

## Crate Layout
- `crates/lenscraft-core` (L1/L2/L3/L5/L8): Pure-Rust library with `#![forbid(unsafe_code)]` and `deny(unwrap_used, expect_used, panic, todo, unimplemented)`.
- `crates/lenscraft-mcp` (L2/L4): CLI subcommands, `stdio` MCP server (`lenscraft-mcp mcp`), and `axum` mobile HTTP server (`lenscraft-mcp serve` on port `11265`).
- `crates/lenscraft-app` (L3): Native `eframe` / `egui 0.37` desktop workstation with Bookend Menu Bar, Command Palette, 7-tab Help Center, Light/Dark high-contrast themes, and `Ctrl+Wheel` zoom.

## Environment Variables
- `LENSCRAFT_OLLAMA_URL`: Default `http://127.0.0.1:11434`
- `LENSCRAFT_SPEECH_URL`: Default `http://127.0.0.1:10909` (`speech-mcp`)
- `LENSCRAFT_OVERPASS_URL`: Default `https://overpass-api.de/api/interpreter`

## Mobile Setup via Tailscale Serve (L4/L5)
iOS Safari requires HTTPS for camera (`getUserMedia`), GPS (`navigator.geolocation`), and microphone (`MediaRecorder`) access.
1. Start the server on Goliath:
   ```powershell
   cargo run -p lenscraft-mcp -- serve --host 127.0.0.1 --port 11265
   ```
2. Expose over Tailscale HTTPS:
   ```powershell
   .\scripts\tailscale-serve.ps1
   ```
3. Open the Tailscale HTTPS URL on iPhone or GPD and add it to the Home Screen.
4. Tap **Hold / Tap Ask** at any time while audio is playing to trigger **barge-in** (stops playback and aborts pending TTS requests immediately).

## Building a Museum Collection Pack (L8)
```powershell
cargo run -p lenscraft-mcp -- pack-build --museum "Kunsthistorisches Museum Wien" --qid Q95569 --limit 100 --out khm-pack.json
```
Load the resulting JSON pack in `lenscraft-app` via **Packs > Load Collection Pack...** and match photos offline.
