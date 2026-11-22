# Overnight prep for all 18 flagship world museums in WORLD_MUSEUMS.
# Pass -WithSignatures to hydrate 24-dim color signatures from Commons thumbnails.
param(
    [switch]$WithSignatures
)
$ErrorActionPreference = 'Stop'

$RepoRoot = Split-Path -Parent $PSScriptRoot
$PacksDir = Join-Path $RepoRoot "packs"
if (-not (Test-Path $PacksDir)) { New-Item -ItemType Directory -Path $PacksDir | Out-Null }

$Exe = Join-Path $RepoRoot "target\debug\lenscraft-mcp.exe"
if (-not (Test-Path $Exe)) {
    & cargo build --manifest-path (Join-Path $RepoRoot "Cargo.toml") -p lenscraft-mcp
}

$WorldMuseums = @(
    @{ Slug = "khm"; Name = "Kunsthistorisches Museum Wien"; Qid = "Q95569"; Limit = 250 },
    @{ Slug = "belvedere"; Name = "Belvedere"; Qid = "Q303139"; Limit = 250 },
    @{ Slug = "albertina"; Name = "Albertina"; Qid = "Q371908"; Limit = 250 },
    @{ Slug = "leopold"; Name = "Leopold Museum"; Qid = "Q59435"; Limit = 200 },
    @{ Slug = "mumok"; Name = "mumok"; Qid = "Q689951"; Limit = 150 },
    @{ Slug = "liechtenstein"; Name = "Liechtenstein Museum"; Qid = "Q1824069"; Limit = 150 },
    @{ Slug = "louvre"; Name = "Louvre"; Qid = "Q19675"; Limit = 500 },
    @{ Slug = "orsay"; Name = "Musee d'Orsay"; Qid = "Q23402"; Limit = 300 },
    @{ Slug = "prado"; Name = "Museo del Prado"; Qid = "Q160112"; Limit = 350 },
    @{ Slug = "uffizi"; Name = "Uffizi Gallery"; Qid = "Q51252"; Limit = 350 },
    @{ Slug = "rijksmuseum"; Name = "Rijksmuseum"; Qid = "Q190804"; Limit = 350 },
    @{ Slug = "british-museum"; Name = "British Museum"; Qid = "Q6373"; Limit = 350 },
    @{ Slug = "national-gallery-london"; Name = "National Gallery"; Qid = "Q180788"; Limit = 350 },
    @{ Slug = "met"; Name = "Metropolitan Museum of Art"; Qid = "Q160236"; Limit = 400 },
    @{ Slug = "moma"; Name = "Museum of Modern Art"; Qid = "Q188740"; Limit = 300 },
    @{ Slug = "hermitage"; Name = "Hermitage Museum"; Qid = "Q132783"; Limit = 350 },
    @{ Slug = "gemaeldegalerie-berlin"; Name = "Gemaldegalerie Berlin"; Qid = "Q165631"; Limit = 250 },
    @{ Slug = "kunsthaus-zurich"; Name = "Kunsthaus Zurich"; Qid = "Q685038"; Limit = 200 }
)

foreach ($m in $WorldMuseums) {
    $out = Join-Path $PacksDir "$($m.Slug).json"
    Write-Host "Building $($m.Name) ($($m.Qid), limit $($m.Limit)) -> $out"
    $cliArgs = @("pack-build", "--museum", $m.Name, "--qid", $m.Qid, "--limit", "$($m.Limit)", "--out", $out)
    if ($WithSignatures) { $cliArgs += "--with-signatures" }
    & $Exe @cliArgs
    Start-Sleep -Seconds 1
}
