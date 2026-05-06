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

# Canonicalise through any Windows junctions / symlinks before
# deriving sub-paths. If the user invokes the script via a
# junction (e.g. `F:\Files\repo\...` → `F:\Replica\NAS\Files\repo\...`),
# Node's `fs.realpathSync` — which Vite's [vite:build-html] plugin
# transitively calls when resolving index.html — canonicalises to
# the underlying real path. That diverges from `$PSScriptRoot`'s
# junction-traversed view, so rollup ends up with one path used as
# the build root and a different path as the asset's resolved
# location. It then computes `path.relative(root, asset)` which
# emits an asset name like `..\..\..\..\..\..\..\..\Replica\NAS\…\index.html`
# — eight dotdots that rollup rejects with
# `The "fileName" or "name" properties of emitted chunks and
# assets must be strings that are neither absolute nor relative
# paths`.
#
# Resolving here keeps every downstream tool (npm, vite, cargo,
# Push-Location) on the same canonical path. Falls back to the
# raw `$PSScriptRoot` if node is unavailable or the resolve fails
# for any reason.
$nodeCmd = Get-Command node -ErrorAction SilentlyContinue
if ($null -ne $nodeCmd) {
    try {
        $resolved = & $nodeCmd.Source -e "process.stdout.write(require('fs').realpathSync(process.argv[1]))" $RepoRoot 2>$null
        if ($LASTEXITCODE -eq 0 -and $resolved) {
            $RepoRoot = $resolved.Trim()
        }
    } catch { }
}

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

# ─── Force tauri-build re-run (asset embedding) ───────────────
#
# `tauri::generate_context!()` embeds frontend/dist contents into the
# binary at compile time via a cache file written by tauri-build in
# OUT_DIR. Cargo's `rerun-if-changed=frontend/dist` directive in
# build.rs SHOULD pick up dist changes, but it's been observed to miss
# the edge case where dist was empty/partial on a prior run and got
# fully (re-)populated by the npm build above. Touching build.rs
# guarantees cargo re-runs the build script — which rewrites the OUT_DIR
# asset cache, which forces the macro to re-expand on the next compile.
$BuildRs = Join-Path $PavilionDir "build.rs"
if (Test-Path $BuildRs) { (Get-Item $BuildRs).LastWriteTime = Get-Date }

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
