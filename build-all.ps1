<#
.SYNOPSIS
    Build, deploy, and start all orchestrators in one shot.

.DESCRIPTION
    1. Builds all platforms (calls installer/build-all-platforms.ps1)
    2. Deploys to stones (calls installer/deploy.ps1)
    3. Starts all orchestrators that have a start.bat

.EXAMPLE
    .\build-all.ps1
#>

$ErrorActionPreference = "Stop"
$root = $PSScriptRoot

Write-Host ""
Write-Host "  Zen Garden - Build, Deploy, Orchestrators" -ForegroundColor Cyan
Write-Host "  ===========================================" -ForegroundColor Cyan
Write-Host ""

# Step 1: Build all platforms
Write-Host "[1/3] Building all platforms..." -ForegroundColor Yellow
Write-Host ""
& "$root\installer\build-all-platforms.ps1"
if ($LASTEXITCODE -ne 0) {
    Write-Host "`nERROR: Build failed. Aborting." -ForegroundColor Red
    exit 1
}

# Step 2: Deploy to stones
Write-Host ""
Write-Host "[2/3] Deploying to stones..." -ForegroundColor Yellow
Write-Host ""
& "$root\installer\deploy.ps1"
$deployExitCode = $LASTEXITCODE
if ($deployExitCode -ne 0) {
    Write-Host "`nWARNING: Some stones failed to deploy (see summary above). Continuing..." -ForegroundColor Yellow
}

if ($deployExitCode -ne 0) {
    Write-Host "  Done (with deploy warnings - some stones failed)." -ForegroundColor Yellow
    exit 1
}
else {
    Write-Host "  All done." -ForegroundColor Green
}
Write-Host ""
