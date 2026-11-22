# Build collection packs for Vienna's major art museums.
# Pass -WithSignatures for overnight thumbnail signature hydration.
param(
    [switch]$WithSignatures
)
$ErrorActionPreference = 'Stop'

$RepoRoot = Split-Path -Parent $PSScriptRoot
$PacksDir = Join-Path $RepoRoot "packs"
if (-not (Test-Path $PacksDir)) { New-Item -ItemType Directory -Path $PacksDir | Out-Null }

$Museums = @(
    @{ Slug = "khm";           Name = "Kunsthistorisches Museum Wien";             Qid = "Q95569";   Limit = 250 },
    @{ Slug = "belvedere";     Name = "Belvedere";                                 Qid = "Q303139";  Limit = 250 },
    @{ Slug = "albertina";     Name = "Albertina";                                 Qid = "Q371908";  Limit = 250 },
    @{ Slug = "leopold";       Name = "Leopold Museum";                            Qid = "Q59435";   Limit = 200 },
    @{ Slug = "mumok";         Name = "mumok";                                     Qid = "Q689951";  Limit = 150 },
    @{ Slug = "liechtenstein"; Name = "Liechtenstein Museum";                      Qid = "Q1824069"; Limit = 150 }
)

$Exe = Join-Path $RepoRoot "target\debug\lenscraft-mcp.exe"
if (-not (Test-Path $Exe)) {
    & cargo build --manifest-path (Join-Path $RepoRoot "Cargo.toml") -p lenscraft-mcp
}

foreach ($m in $Museums) {
    $out = Join-Path $PacksDir "$($m.Slug).json"
    Write-Host "Building $($m.Name) ($($m.Qid), limit $($m.Limit)) -> $out"
    $cliArgs = @("pack-build", "--museum", $m.Name, "--qid", $m.Qid, "--limit", "$($m.Limit)", "--out", $out)
    if ($WithSignatures) { $cliArgs += "--with-signatures" }
    & $Exe @cliArgs
    Start-Sleep -Seconds 1
}
