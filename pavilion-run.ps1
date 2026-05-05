#!/usr/bin/env pwsh
<#
.SYNOPSIS
    Build and launch Pavilion (Windows tray client for Zen Garden).

.DESCRIPTION
    One-shot build + launch script for Pavilion. Idempotent: re-runs
    rebuild only what changed.

    Default mode produces an optimised release binary at
    target/release/garden-pavilion.exe and launches it.

    -Dev mode uses `cargo tauri dev`, which hot-reloads the frontend on
    file changes. Useful while iterating on the React app.

.EXAMPLE
    .\pavilion-run.ps1
    Build (release) and launch.

.EXAMPLE
    .\pavilion-run.ps1 -Dev
    Run in development mode with hot-reload.

.EXAMPLE
    .\pavilion-run.ps1 -BuildOnly
    Build but do not launch.
#>

[CmdletBinding()]
param(
    [switch]$Dev,
    [switch]$BuildOnly,
    [switch]$Clean
)

$ErrorActionPreference = "Stop"
$RepoRoot = $PSScriptRoot
$PavilionDir = Join-Path $RepoRoot "src\pavilion"
$FrontendDir = Join-Path $PavilionDir "frontend"

function Write-Step {
    param([string]$Msg)
    Write-Host "==> $Msg" -ForegroundColor Cyan
}

function Test-Cmd {
    param([string]$Name, [string]$Hint)
    if (-not (Get-Command $Name -ErrorAction SilentlyContinue)) {
        Write-Error "$Name not found. $Hint"
    }
}

# ─── Preflight ────────────────────────────────────────────────

Write-Step "Preflight checks"
Test-Cmd "cargo" "Install Rust: https://rustup.rs/"
Test-Cmd "node"  "Install Node.js 18+: https://nodejs.org/"
Test-Cmd "npm"   "npm should ship with Node.js."

if ($Dev) {
    Test-Cmd "cargo-tauri" "Install: cargo install tauri-cli --version '^2.0' --locked"
}

# ─── Clean (optional) ─────────────────────────────────────────

if ($Clean) {
    Write-Step "Cleaning artifacts"
    if (Test-Path (Join-Path $FrontendDir "dist"))         { Remove-Item -Recurse -Force (Join-Path $FrontendDir "dist") }
    if (Test-Path (Join-Path $FrontendDir "node_modules")) { Remove-Item -Recurse -Force (Join-Path $FrontendDir "node_modules") }
    & cargo clean -p garden-pavilion
}

# ─── Frontend deps ────────────────────────────────────────────

Push-Location $FrontendDir
try {
    if (-not (Test-Path "node_modules")) {
        Write-Step "Installing frontend dependencies (~10s, one-time)"
        npm install --no-progress
        if ($LASTEXITCODE -ne 0) { throw "npm install failed" }
    }
} finally {
    Pop-Location
}

# ─── Dev mode short-circuit ───────────────────────────────────

if ($Dev) {
    Push-Location $PavilionDir
    try {
        Write-Step "Launching cargo tauri dev (Vite + Rust, hot-reload)"
        Write-Host "Press Ctrl+C to stop." -ForegroundColor DarkGray
        & cargo tauri dev
    } finally {
        Pop-Location
    }
    exit $LASTEXITCODE
}

# ─── Frontend build ───────────────────────────────────────────

Push-Location $FrontendDir
try {
    Write-Step "Building frontend (Vite)"
    npm run build
    if ($LASTEXITCODE -ne 0) { throw "frontend build failed" }
} finally {
    Pop-Location
}

# ─── Rust release build ───────────────────────────────────────

Push-Location $RepoRoot
try {
    Write-Step "Building garden-pavilion (release; first build ~3min, incremental ~10s)"
    # --features custom-protocol enables Tauri's embedded-asset protocol
    # so the binary loads frontend/dist/ rather than http://localhost:5173.
    & cargo build --release -p garden-pavilion --features custom-protocol
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
} finally {
    Pop-Location
}

$Binary = Join-Path $RepoRoot "target\release\garden-pavilion.exe"
if (-not (Test-Path $Binary)) {
    Write-Error "Build succeeded but $Binary not found. Investigate."
}

$SizeMb = [Math]::Round((Get-Item $Binary).Length / 1MB, 1)
Write-Host ""
Write-Host "Binary:   $Binary ($SizeMb MB)" -ForegroundColor Green
Write-Host "Frontend: $FrontendDir\dist\" -ForegroundColor Green

# ─── Launch ───────────────────────────────────────────────────

if ($BuildOnly) {
    Write-Host ""
    Write-Host "Build complete. Run with:" -ForegroundColor DarkGray
    Write-Host "    & '$Binary'" -ForegroundColor DarkGray
    exit 0
}

Write-Step "Launching Pavilion"
Write-Host "Look for the tray icon (P, dark green) in your notification area."
Write-Host "Window opens centred. Close-to-tray; quit via tray menu."
Write-Host ""

# Single-instance plugin handles duplicate-launch focus, so re-running
# this script while Pavilion is already running just brings the window
# to the foreground rather than opening a second instance.
Start-Process -FilePath $Binary
