<#
.SYNOPSIS
  Stage a dev companions tree from the flat dist output and run moss
  locally on Windows against it.

.DESCRIPTION
  Moss expects a companions directory laid out as:

      <companions_dir>/
        firefly/
          garden-firefly.exe
        cricket/
          garden-cricket.exe

  The repo's `dist\windows-x64\` produces a flat layout with all
  binaries at the top level. This script creates a `dev\companions\`
  mirror with the expected subdirectory structure, using hard links
  when possible (instant, zero copies), falling back to copies when
  the filesystem refuses hard links (e.g. across drives).

  Then it sets the relevant `GARDEN_*` environment variables in the
  current PowerShell session and runs `garden-moss.exe`. Moss writes
  its runtime data under `dev\data\` so no system directory is touched.

.PARAMETER RepoRoot
  Optional. Defaults to the detected repo root.

.PARAMETER DataDir
  Optional. Defaults to `<RepoRoot>\dev\data`.

.PARAMETER Port
  Optional. HTTP port for moss. Default 7185.

.PARAMETER Run
  Switch. After staging, launch garden-moss.exe and tail its output.

.EXAMPLE
  # Stage only:
  .\scripts\dev-firefly.ps1

  # Stage and run:
  .\scripts\dev-firefly.ps1 -Run
#>

param(
    [string]$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path,
    [string]$DataDir,
    [int]$Port = 7185,
    [switch]$Run
)

$ErrorActionPreference = "Stop"

$Dist = Join-Path $RepoRoot "dist\windows-x64"
$DevDir = Join-Path $RepoRoot "dev"
$CompanionsDir = Join-Path $DevDir "companions"
if (-not $DataDir) { $DataDir = Join-Path $DevDir "data" }

if (-not (Test-Path $Dist)) {
    throw "Flat dist not found at $Dist. Build first: .\installer\compile-windows-x64.ps1"
}

$MossExe = Join-Path $Dist "garden-moss.exe"
if (-not (Test-Path $MossExe)) {
    throw "garden-moss.exe missing from $Dist. Rebuild."
}

# Companions to stage. Keep this list short; mirrors `dist.json`.
$Companions = @("firefly", "cricket")

Write-Host "Staging dev companions..." -ForegroundColor Cyan
foreach ($c in $Companions) {
    $src = Join-Path $Dist "garden-$c.exe"
    if (-not (Test-Path $src)) {
        Write-Host "  skip   $c (no $src)" -ForegroundColor DarkYellow
        continue
    }
    $destDir = Join-Path $CompanionsDir $c
    $destExe = Join-Path $destDir "garden-$c.exe"

    New-Item -ItemType Directory -Force -Path $destDir | Out-Null
    if (Test-Path $destExe) { Remove-Item $destExe -Force }

    # Prefer hard link so rebuilds are picked up without re-running this
    # script; fall back to copy if the filesystem refuses.
    try {
        New-Item -ItemType HardLink -Path $destExe -Target $src -ErrorAction Stop | Out-Null
        Write-Host "  link   $c -> $destExe" -ForegroundColor Green
    }
    catch {
        Copy-Item $src $destExe -Force
        Write-Host "  copy   $c -> $destExe" -ForegroundColor Green
    }
}

New-Item -ItemType Directory -Force -Path $DataDir | Out-Null
$SharedDataDir = Join-Path $DevDir "shared-data"
New-Item -ItemType Directory -Force -Path $SharedDataDir | Out-Null

$env:GARDEN_COMPANIONS_DIR = $CompanionsDir
$env:GARDEN_DATA_DIR = $DataDir
$env:GARDEN_SHARED_DATA_DIR = $SharedDataDir
$env:PORT = "$Port"
# Verbose-ish dev default; override RUST_LOG on the command line if needed.
if (-not $env:RUST_LOG) {
    $env:RUST_LOG = "info,garden_moss=debug,garden_firefly=debug,garden_companion_sdk::usb_devices=debug"
}

Write-Host "`nEnvironment:" -ForegroundColor Cyan
Write-Host "  GARDEN_COMPANIONS_DIR = $env:GARDEN_COMPANIONS_DIR"
Write-Host "  GARDEN_DATA_DIR       = $env:GARDEN_DATA_DIR"
Write-Host "  GARDEN_SHARED_DATA_DIR= $env:GARDEN_SHARED_DATA_DIR"
Write-Host "  PORT                  = $env:PORT"
Write-Host "  RUST_LOG              = $env:RUST_LOG"

if ($Run) {
    Write-Host "`nLaunching $MossExe ..." -ForegroundColor Cyan
    & $MossExe
}
else {
    Write-Host "`nStaged. To run:" -ForegroundColor Yellow
    Write-Host "  & '$MossExe'"
    Write-Host "Or re-invoke with -Run."
}
