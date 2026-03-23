<#
.SYNOPSIS
    Modern cross-compilation via cross-rs

.DESCRIPTION
    Uses cross-rs for reliable Linux binary builds from Windows.
    Handles version stamping correctly via Cargo environment variables.

.PARAMETER Fast
    Use fast-release profile (thin LTO, ~40% faster)

.PARAMETER Release
    Use full release profile (full LTO, smallest binaries)

.PARAMETER DebugBuild
    Compile debug binaries

.PARAMETER Jobs
    Number of parallel jobs (default: CPU count)

.EXAMPLE
    .\compile-cross.ps1
    # Default: fast-release build

.EXAMPLE
    .\compile-cross.ps1 -Release
    # Full LTO release build
#>

[CmdletBinding()]
param(
    [switch]$Fast,
    [switch]$Release,
    [switch]$DebugBuild,
    [int]$Jobs = 0
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$WORKSPACE_ROOT = (Get-Item $PSScriptRoot).Parent.FullName
$DIST_DIR = Join-Path $WORKSPACE_ROOT "dist"
$LINUX_DIR = Join-Path $DIST_DIR "linux-x64"

Write-Host "`n+====================================================+" -ForegroundColor Cyan
Write-Host "|   Zen Garden Cross-Compilation Build              |" -ForegroundColor Cyan
Write-Host "+====================================================+`n" -ForegroundColor Cyan

# Check if cross is installed
$crossInstalled = Get-Command cross -ErrorAction SilentlyContinue
if (-not $crossInstalled) {
    Write-Host "cross-rs not found. Installing..." -ForegroundColor Yellow
    cargo install cross --git https://github.com/cross-rs/cross
    if ($LASTEXITCODE -ne 0) {
        Write-Error "Failed to install cross-rs"
        exit 1
    }
    Write-Host "OK cross-rs installed`n" -ForegroundColor Green
}

# Check Docker
try {
    docker version | Out-Null
    Write-Host "OK Docker available`n" -ForegroundColor Green
} catch {
    Write-Error "Docker not available. Install Docker Desktop: https://www.docker.com/products/docker-desktop/"
    exit 1
}

# Determine build profile
$buildProfile = if ($DebugBuild) {
    "debug"
} elseif ($Release) {
    "release"
} elseif ($Fast) {
    "fast-release"
} else {
    "fast-release"  # Default
}

$profileFlag = switch ($buildProfile) {
    "debug" { @() }
    "fast-release" { @("--profile", "fast-release") }
    "release" { @("--release") }
}

$parallelJobs = if ($Jobs -gt 0) { $Jobs } else { [Environment]::ProcessorCount }

# Set version environment variables
if (-not $env:GARDEN_VERSION) {
    $versionFile = Join-Path $WORKSPACE_ROOT "version.json"
    $versionData = Get-Content $versionFile | ConvertFrom-Json
    $revision = (Get-Date).ToString("yyyyMMddHHmm")
    $env:GARDEN_VERSION = "$($versionData.major).$($versionData.minor).$revision"
    $env:BUILD_NUMBER = $revision
    $env:CARGO_BUILD_NUMBER = $revision
}

Write-Host "Build Configuration:" -ForegroundColor Yellow
Write-Host "  Version: $env:GARDEN_VERSION"
Write-Host "  Profile: $buildProfile"
Write-Host "  Target: x86_64-unknown-linux-gnu"
Write-Host "  Jobs: $parallelJobs`n"

# Create output directory
New-Item -ItemType Directory -Force -Path $LINUX_DIR | Out-Null

# Clean previous artifacts to force version rebuild
Write-Host "Cleaning cached binaries..." -ForegroundColor DarkGray
$crossTargetPath = Join-Path $WORKSPACE_ROOT "target\cross\x86_64-unknown-linux-gnu\$buildProfile"
if (Test-Path $crossTargetPath) {
    Get-ChildItem $crossTargetPath -Filter "garden-*" -File -ErrorAction SilentlyContinue | Remove-Item -Force
}

Push-Location $WORKSPACE_ROOT
try {
    Write-Host "Building Linux binaries with cross-rs..." -ForegroundColor Cyan
    Write-Host "  -> garden-moss (daemon)"
    Write-Host "  -> garden-lantern (registry)"
    Write-Host "  -> garden-rake (CLI)`n"

    # Use separate target directory for cross to avoid permission issues on Windows
    $env:CROSS_TARGET_DIR = Join-Path $WORKSPACE_ROOT "target\cross"

    # Build all binaries
    $buildArgs = @(
        "build",
        "--target", "x86_64-unknown-linux-gnu",
        "-j", "$parallelJobs"
    ) + $profileFlag + @(
        "--bin", "garden-moss",
        "--bin", "garden-lantern",
        "--bin", "garden-rake"
    )

    cross @buildArgs
    
    if ($LASTEXITCODE -ne 0) {
        throw "Build failed"
    }

    # Copy binaries to dist/linux-x64/
    $sourcePath = Join-Path $WORKSPACE_ROOT "target\cross\x86_64-unknown-linux-gnu\$buildProfile"
    
    Copy-Item "$sourcePath\garden-lantern" "$LINUX_DIR\garden-lantern" -Force
    Copy-Item "$sourcePath\garden-moss" "$LINUX_DIR\garden-moss" -Force
    Copy-Item "$sourcePath\garden-rake" "$LINUX_DIR\garden-rake" -Force
    
    Write-Host "`nOK Build complete`n" -ForegroundColor Green

} finally {
    Pop-Location
}

# Display results
Write-Host "+====================================================+" -ForegroundColor Green
Write-Host "|   Build Complete!                                  |" -ForegroundColor Green
Write-Host "+====================================================+`n" -ForegroundColor Green

Write-Host "Artifacts in $LINUX_DIR`:" -ForegroundColor Cyan
Get-ChildItem $LINUX_DIR -ErrorAction SilentlyContinue | ForEach-Object {
    $sizeMB = [math]::Round($_.Length / 1MB, 2)
    Write-Host ("  OK {0,-20} {1,10} MB" -f $_.Name, $sizeMB) -ForegroundColor Green
}

Write-Host "`nNext steps:" -ForegroundColor Yellow
Write-Host "  Deploy: .\deploy.ps1 -UsePackage"
Write-Host "  USB:    .\NewStone-linux-x64.ps1 -UsbDrive G:"
