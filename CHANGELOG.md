# Changelog - lenscraft

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