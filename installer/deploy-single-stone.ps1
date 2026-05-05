<#
.SYNOPSIS
    Deploy the freshest Linux x64 zen-garden package to ONE specific
    stone (skips deploy.ps1's discover-and-fan-out behavior).

.DESCRIPTION
    Targeted single-stone deploy. Same wire contract as deploy.ps1
    (POST /api/v1/stone/deploy with X-Package-SHA256 header) but
    points at one endpoint instead of every stone the network
    answers. Useful for staged rollouts: smoke-test on one stone,
    then run deploy.ps1 to fan out.

.PARAMETER Stone
    Hostname or IP of the target stone. Examples:
      stone-golden-summit
      192.168.1.150

.PARAMETER Port
    HTTP port. Default 7185.

.PARAMETER PackagePath
    Path to the .tar.gz package. Default: freshest
    `dist/packages/zen-garden-*-linux-x64.tar.gz`.

.EXAMPLE
    .\deploy-single-stone.ps1 -Stone stone-golden-summit
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$Stone,

    [int]$Port = 7185,

    [string]$PackagePath
)

$ErrorActionPreference = "Stop"

if (-not $PackagePath) {
    $packagesDir = Resolve-Path "$PSScriptRoot/../dist/packages"
    $latest = Get-ChildItem $packagesDir -Filter "zen-garden-*-linux-x64.tar.gz" `
        | Sort-Object LastWriteTime -Descending `
        | Select-Object -First 1
    if (-not $latest) {
        Write-Host "X No Linux x64 package found in $packagesDir" -ForegroundColor Red
        Write-Host "  Run installer/build-all-platforms.ps1 first." -ForegroundColor Yellow
        exit 1
    }
    $PackagePath = $latest.FullName
}

if (-not (Test-Path $PackagePath)) {
    Write-Host "X Package not found: $PackagePath" -ForegroundColor Red
    exit 1
}

$packageName = Split-Path -Leaf $PackagePath
$endpoint = "http://${Stone}:${Port}"
$url = "$endpoint/api/v1/stone/deploy"

Write-Host ""
Write-Host "=== Single-stone deploy ===" -ForegroundColor Cyan
Write-Host "  Stone:    $Stone" -ForegroundColor White
Write-Host "  Endpoint: $endpoint" -ForegroundColor White
Write-Host "  Package:  $packageName" -ForegroundColor White
Write-Host ""

Write-Host "Computing SHA256..." -ForegroundColor White
$hash = (Get-FileHash $PackagePath -Algorithm SHA256).Hash.ToLower()
Write-Host "  $hash" -ForegroundColor Gray

$packageBytes = [System.IO.File]::ReadAllBytes($PackagePath)
$sizeMb = [math]::Round($packageBytes.Length / 1MB, 2)
Write-Host "  $sizeMb MB" -ForegroundColor Gray
Write-Host ""

Write-Host "POST $url" -ForegroundColor White
$headers = @{ "X-Package-SHA256" = $hash }
try {
    $response = Invoke-RestMethod `
        -Uri $url `
        -Method Post `
        -Body $packageBytes `
        -ContentType "application/octet-stream" `
        -Headers $headers `
        -TimeoutSec 180

    if ($response.status -eq "accepted") {
        Write-Host "[OK] Package staged" -ForegroundColor Green
    } else {
        Write-Host "X Unexpected response:" -ForegroundColor Red
        $response | ConvertTo-Json -Compress | Write-Host
        exit 1
    }
} catch {
    Write-Host "X Deploy POST failed: $_" -ForegroundColor Red
    exit 1
}

Write-Host ""
Write-Host "Waiting for moss restart..." -ForegroundColor White
Start-Sleep -Seconds 8

$healthUrl = "$endpoint/health"
$online = $false
for ($i = 1; $i -le 30; $i++) {
    Start-Sleep -Seconds 3
    try {
        $health = Invoke-RestMethod -Uri $healthUrl -Method Get -TimeoutSec 5
        if ($health.status) {
            Write-Host ""
            Write-Host "[OK] $Stone is back online" -ForegroundColor Green
            Write-Host "     version: $($health.version)" -ForegroundColor Gray
            $online = $true
            break
        }
    } catch {
        Write-Host "." -NoNewline
    }
}

if (-not $online) {
    Write-Host ""
    Write-Host "!  Stone did not respond after restart (timed out)" -ForegroundColor Yellow
    exit 1
}

Write-Host ""
Write-Host "[OK] Deploy complete." -ForegroundColor Green
exit 0
