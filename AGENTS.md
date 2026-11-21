# AGENTS.md - lenscraft

Shared lens engine for the craft fleet: a photo goes in, ranked identifications with the
evidence for each come out, from a local vision model. Used by travelcraft (SightLens tab,
journal), giscraft (venue lookup, bearing lens) and, later, a small server for phone and GPD.
Plan: `D:\Dev\repos\giscraft\docs\TIERED_PLAN.md`, Tier 5.

## Rules
- **LF only.** `.gitattributes` enforces it; write files with LF, never CRLF.
- **Single trunk.** Work on `main`. No agent branches, no leftover WIP branches.
- **No em dash** in any file, code, doc or commit message.
- **No fakes.** No canned answers, no hardcoded confidence, no demo hits that look real.
  An unknown subject is `verdict: uncertain`; every failure is a typed `LensError` with a
  `kind`. MCP failures set `isError: true`; CLI failures exit 1.
- **Deny lints:** unwrap, expect, panic, todo, unimplemented; `unsafe_code` forbidden.
  Tests return `Result` and use `?` instead.
- **stdio invariant:** in `mcp` mode stdout is pure JSON-RPC; logs go to stderr.
- **Tool names** use `[a-z0-9_]` only (`lens_identify`), no dots.
- **Test photos** go in `samples/` (gitignored). Do not commit museum photos.

## Layout
- `crates/lenscraft-core` (L1): `image_prep` (read, EXIF rotate, downscale, JPEG),
  `vision` (Ollama transport, model capabilities, status), `identify` (prompt, schema,
  label check, verdict).
- `crates/lenscraft-mcp` (L2): CLI `identify` / `status` and `mcp` stdio server.

## Gates (all must pass before a commit)
1. `cargo fmt --all --check`
2. `cargo clippy --workspace --all-targets --all-features -- -D warnings`
3. `cargo test --workspace`
4. CHANGELOG line for anything user-visible.

## Environment
- `LENSCRAFT_OLLAMA_URL` (default `http://127.0.0.1:11434`)
- Default model `gemma4:12b` (vision + audio on Goliath). `lenscraft-mcp status` checks it.

## Documentation preservation and hooks

On 2026-11-20 a README "standardization" sweep across the fleet deleted project descriptions,
status sections and user warnings in nine repos. Full rules:
`mcp-central-docs/standards/CONTENT_PRESERVATION_STANDARD.md`.

- Applying any standard or template restructures; it never deletes. Move content (within the file
  or to `docs/` with a link), do not drop it. Never remove what the project is, what it is based on,
  its status (what works, what does not, what is not built) or warnings to users.
- Remove content only when Sandra explicitly asks for that removal, in this repo.
- Never add claims you have not verified (release downloads, versions, test counts, features).
- Only standards with `status: active` may be applied across repos. A change across several repos is
  done on one repo first and approved by Sandra; its commits name the tool and carry a `Sweep-Id:` line.
- `.githooks/commit-msg` blocks a commit that deletes a Markdown file, removes 20% of its lines
  (15 or more) or removes 3 or more headings. A removal Sandra asked for passes with
  `Docs-Removal-Approved: <who and why>` in the message. Enable once per clone:
  `git config core.hooksPath .githooks`. Never use `--no-verify`; if a hook blocks you, stop and ask.
