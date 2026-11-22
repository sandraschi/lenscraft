param(
    [switch]$Web,
    [switch]$Mcp,
    [int]$Port = 11265
)
$ErrorActionPreference = 'Stop'
$RepoRoot = $PSScriptRoot
Set-Location $RepoRoot

if ($Mcp) {
    & cargo run --manifest-path (Join-Path $RepoRoot "Cargo.toml") -p lenscraft-mcp -- mcp
    exit $LASTEXITCODE
}

if ($Web) {
    Write-Host "Starting LensCraft PWA & Museum Server on http://127.0.0.1:$Port ..."
    Start-Process "http://127.0.0.1:$Port"
    & cargo run --manifest-path (Join-Path $RepoRoot "Cargo.toml") -p lenscraft-mcp -- serve --port $Port
    exit $LASTEXITCODE
}

Write-Host "Launching LensCraft Desktop Workstation (Museums -> World Museum Directory & Floorplans)..."
& cargo run --manifest-path (Join-Path $RepoRoot "Cargo.toml") -p lenscraft-app
exit $LASTEXITCODE
