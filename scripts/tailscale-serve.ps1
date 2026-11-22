# Expose LensCraft mobile UI (port 11265) over Tailscale HTTPS for iPhone and GPD
param(
    [int]$Port = 11265
)
$ErrorActionPreference = 'Stop'
& "C:\Program Files\Tailscale\tailscale.exe" serve --bg $Port
Write-Host "Tailscale serve active for http://127.0.0.1:$Port"
