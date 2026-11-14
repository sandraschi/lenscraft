set shell := ["powershell.exe", "-NoProfile", "-Command"]

default:
    @just --list

ci:
    cargo fmt --all -- --check
    cargo clippy --workspace --all-targets --all-features -- -D warnings
    cargo test --workspace

status:
    cargo run -q -p lenscraft-mcp -- status

identify path:
    cargo run -q -p lenscraft-mcp -- identify "{{path}}"

mcp:
    cargo run -q -p lenscraft-mcp -- mcp