# Overnight prep for iconic 3D walk-in painting simulations stored in worlds/worlds_db.json.
# Mode 1 (default): downloads Commons thumbnail and builds local 3D relief PLY (zero API cost).
# Mode 2 (-UseSplatcraftMarble): calls `splatcraft-cli worlds generate` when WORLDLABS_API_KEY is set.
param(
    [switch]$UseSplatcraftMarble,
    [switch]$ConfirmSpend
)
$ErrorActionPreference = 'Stop'

$RepoRoot = Split-Path -Parent $PSScriptRoot
$WorldsDir = Join-Path $RepoRoot "worlds"
if (-not (Test-Path $WorldsDir)) { New-Item -ItemType Directory -Path $WorldsDir | Out-Null }

$Iconic = @(
    @{ Qid = "Q153642"; Title = "The Peasant Wedding";             Creator = "Pieter Bruegel the Elder"; Museum = "Kunsthistorisches Museum Wien"; Prompt = "Step inside Pieter Bruegel's 1567 Peasant Wedding barn banquet, warm ochre and wooden tables" },
    @{ Qid = "Q167794"; Title = "The Tower of Babel";              Creator = "Pieter Bruegel the Elder"; Museum = "Kunsthistorisches Museum Wien"; Prompt = "Walk along the monumental spiral stone ramps of Bruegel's Tower of Babel overlooking the harbour" },
    @{ Qid = "Q12418";  Title = "Mona Lisa";                       Creator = "Leonardo da Vinci";        Museum = "Louvre";                        Prompt = "3D studio around Leonardo da Vinci's Mona Lisa so you can orbit behind the Tuscan landscape" },
    @{ Qid = "Q321303"; Title = "The Garden of Earthly Delights";  Creator = "Hieronymus Bosch";         Museum = "Museo del Prado";               Prompt = "Walkable surreal triptych landscape of Bosch's Garden of Earthly Delights" }
)

$Exe = Join-Path $RepoRoot "target\debug\lenscraft-mcp.exe"

foreach ($w in $Iconic) {
    Write-Host "Prepping 3D world for $($w.Title) ($($w.Qid))..."
    if ($UseSplatcraftMarble) {
        $splatCli = "D:\Dev\repos\splatcraft\target\debug\splatcraft-cli.exe"
        if (-not (Test-Path $splatCli)) {
            Write-Warning "splatcraft-cli.exe not found at $splatCli"
            continue
        }
        $argsList = @("worlds", "generate", "--prompt", $w.Prompt, "--json")
        if ($ConfirmSpend) { $argsList += "--confirm-spend" }
        & $splatCli @argsList
    } else {
        # Fetch Commons image via Wikidata P18 and build local 3D relief PLY
        $wdUrl = "https://www.wikidata.org/w/api.php?action=wbgetentities&ids=$($w.Qid)&props=claims&format=json"
        $resp = Invoke-RestMethod -Uri $wdUrl -Headers @{"User-Agent"="lenscraft/0.2"}
        $imgName = $resp.entities.($w.Qid).claims.P18[0].mainsnak.datavalue.value
        if (-not $imgName) { Write-Warning "No P18 image for $($w.Qid)"; continue }
        $imgUrl = "https://commons.wikimedia.org/wiki/Special:FilePath/$([uri]::EscapeDataString($imgName))?width=800"
        $imgFile = Join-Path $WorldsDir "$($w.Qid).jpg"
        $plyFile = Join-Path $WorldsDir "$($w.Qid).ply"
        Invoke-WebRequest -Uri $imgUrl -OutFile $imgFile -Headers @{"User-Agent"="lenscraft/0.2"}
        & $Exe world-relief --qid $w.Qid --title $w.Title --creator $w.Creator --museum $w.Museum --image $imgFile --out $plyFile --prompt $w.Prompt
        Start-Sleep -Seconds 1
    }
}
