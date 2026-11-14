# lenscraft

What am I looking at? Point a camera at a painting, sculpture or building and get ranked
identifications with the evidence for each, from a vision model running on your own machine.

Status: **0.1.0, lens step L1** (identify engine, CLI, MCP). No app, no speech, no phone
access yet; those are steps L2 to L8 in giscraft's `docs/TIERED_PLAN.md`, Tier 5.

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