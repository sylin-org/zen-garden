<#
.SYNOPSIS
    Build and run the AI Orchestrator container.

.DESCRIPTION
    Three-stage Docker build:
    1. Node 22 — builds the React dashboard (web/dist/)
    2. Rust — compiles the binary with embedded dashboard assets
    3. Debian slim — production runtime image

    After build, starts the container with host networking so it can
    reach Koi (mDNS) and Moss (gateway) on the local network.

.PARAMETER Build
    Build the image without starting. Default: build AND start.

.PARAMETER NoBuild
    Skip the build step (use existing image).

.PARAMETER Detach
    Run container in background (-d).

.EXAMPLE
    .\build-and-run.ps1
    .\build-and-run.ps1 -Build
    .\build-and-run.ps1 -NoBuild -Detach
#>

param(
    [switch]$Build,
    [switch]$NoBuild,
    [switch]$Detach
)

$ErrorActionPreference = 'Stop'

$ImageName = 'zen-garden-ai-orchestrator'
$ContainerName = 'zen-garden-ai-orchestrator'
$ProxyPort = 21434
$DashboardPort = 7190

# Resolve the repo root (Dockerfile uses COPY from repo root context)
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = Resolve-Path (Join-Path $ScriptDir '..\..\..') | Select-Object -ExpandProperty Path

Write-Host "=== Zen Garden AI Orchestrator ===" -ForegroundColor Cyan
Write-Host "Repo root: $RepoRoot"
Write-Host "Image:     $ImageName"
Write-Host ""

# ── Build ────────────────────────────────────────────────────────

if (-not $NoBuild) {
    Write-Host "Building Docker image..." -ForegroundColor Yellow
    Write-Host "  Stage 1: React dashboard (Node 22)"
    Write-Host "  Stage 2: Rust binary (cargo fast-release)"
    Write-Host "  Stage 3: Runtime (debian:bookworm-slim)"
    Write-Host ""

    docker build `
        -t $ImageName `
        -f "$ScriptDir\Dockerfile" `
        $RepoRoot

    if ($LASTEXITCODE -ne 0) {
        Write-Host "Build FAILED" -ForegroundColor Red
        exit 1
    }

    Write-Host ""
    Write-Host "Build succeeded." -ForegroundColor Green
    Write-Host "Image size: $(docker image inspect $ImageName --format '{{.Size}}' | ForEach-Object { [math]::Round($_ / 1MB) }) MB"
    Write-Host ""
}

if ($Build) {
    Write-Host "Build-only mode. Use -NoBuild to skip build next time." -ForegroundColor Cyan
    exit 0
}

# ── Run ──────────────────────────────────────────────────────────

# Stop existing container if running
$existing = docker ps -aq --filter "name=$ContainerName" 2>$null
if ($existing) {
    Write-Host "Stopping existing container..." -ForegroundColor Yellow
    docker rm -f $ContainerName 2>$null | Out-Null
}

Write-Host "Starting container..." -ForegroundColor Yellow
Write-Host "  Proxy:     http://localhost:$ProxyPort"
Write-Host "  Dashboard: http://localhost:$DashboardPort"
Write-Host ""

$detachFlag = if ($Detach) { '-d' } else { '' }

# Pass through cloud provider API keys from environment if set
$envFlags = @()
foreach ($envVar in @(
    'OPENAI_API_KEY', 'ANTHROPIC_API_KEY', 'GOOGLE_API_KEY',
    'COHERE_API_KEY', 'DEEPGRAM_API_KEY',
    'ZG_STONE', 'KOI_ENDPOINT'
)) {
    $val = [System.Environment]::GetEnvironmentVariable($envVar)
    if ($val) {
        $envFlags += '-e'
        $envFlags += "$envVar=$val"
    }
}

$dockerArgs = @(
    'run'
    '--name', $ContainerName
    '-p', "${ProxyPort}:${ProxyPort}"
    '-p', "${DashboardPort}:${DashboardPort}"
    '-v', 'zen-garden-ai-data:/data'
) + $envFlags

if ($Detach) {
    $dockerArgs += '-d'
} else {
    $dockerArgs += '--rm'
}

$dockerArgs += $ImageName

docker @dockerArgs

if ($Detach -and $LASTEXITCODE -eq 0) {
    Write-Host ""
    Write-Host "Container started in background." -ForegroundColor Green
    Write-Host "  Proxy:     http://localhost:$ProxyPort"
    Write-Host "  Dashboard: http://localhost:$DashboardPort"
    Write-Host "  Logs:      docker logs -f $ContainerName"
    Write-Host "  Stop:      docker rm -f $ContainerName"
}
