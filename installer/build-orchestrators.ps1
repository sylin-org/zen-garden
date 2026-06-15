<#
.SYNOPSIS
    Build all Zen Garden orchestrator Docker images

.DESCRIPTION
    Builds Docker images for all orchestrators in src/orchestrators/.
    Each orchestrator is a standalone crate with its own Dockerfile.

    Orchestrators are NOT part of the main workspace build (build.ps1).
    They build independently inside Docker containers using multi-stage
    builds (rust:latest builder → debian:bookworm-slim runtime).

.PARAMETER Include
    Orchestrators to build (comma-separated). Default: all.
    Valid names: ollama, mongodb

.PARAMETER Exclude
    Orchestrators to skip (comma-separated).

.PARAMETER Push
    Push images to Docker Hub after building.

.PARAMETER Tag
    Override image tag (default: "latest").

.EXAMPLE
    .\build-orchestrators.ps1
    Build all orchestrator images

.EXAMPLE
    .\build-orchestrators.ps1 -Include ollama,mongodb
    Build only Ollama and MongoDB orchestrators

.EXAMPLE
    .\build-orchestrators.ps1 -Exclude mongodb
    Build all except MongoDB

.EXAMPLE
    .\build-orchestrators.ps1 -Push
    Build all and push to Docker Hub
#>

[CmdletBinding()]
param(
    [string]$Include = "",
    [string]$Exclude = "",
    [switch]$Push,
    [string]$Tag = "latest",
    [string]$Platform = ""
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

# ── Configuration ──────────────────────────────────────────────────────

$orchestrators = @(
    @{ Name = "ollama";     Dir = "ollama";     Image = "sylinorg/zen-garden-ollama-orchestrator";     Port = "7190" }
    @{ Name = "mongodb";    Dir = "mongodb";    Image = "sylinorg/zen-garden-mongodb-orchestrator";    Port = "7191" }
)

# ── Resolve paths ──────────────────────────────────────────────────────

$scriptDir = $PSScriptRoot
$workspaceRoot = Resolve-Path (Join-Path $scriptDir "..")
$orchestratorsDir = Join-Path $workspaceRoot "src" "orchestrators"

# ── Filter orchestrators ───────────────────────────────────────────────

# @() forces array semantics: a single value (e.g. -Include mongodb) would otherwise
# unwrap to a scalar string, and .Count below throws under Set-StrictMode.
$includeList = @(if ($Include) { $Include -split "," | ForEach-Object { $_.Trim() } } else { @() })
$excludeList = @(if ($Exclude) { $Exclude -split "," | ForEach-Object { $_.Trim() } } else { @() })

$selected = $orchestrators | Where-Object {
    $name = $_.Name
    if ($includeList.Count -gt 0) { return $includeList -contains $name }
    if ($excludeList.Count -gt 0) { return $excludeList -notcontains $name }
    return $true
}

# ── Banner ─────────────────────────────────────────────────────────────

Write-Host ""
Write-Host "  Zen Garden Orchestrator Build" -ForegroundColor Cyan
Write-Host "  ─────────────────────────────" -ForegroundColor DarkGray
Write-Host "  Workspace: $workspaceRoot"
Write-Host "  Tag:       $Tag"
Write-Host "  Push:      $($Push.IsPresent)"
Write-Host ""

$toBuild = $selected | ForEach-Object { $_.Name }
Write-Host "  Building: $($toBuild -join ', ')" -ForegroundColor Green
Write-Host ""

# ── Ensure cross-build emulation (when targeting a foreign platform) ─────

if ($Platform -and $Platform -match "arm64|aarch64") {
    Write-Host "  Cross-build platform: $Platform" -ForegroundColor Cyan
    $emuOk = $false
    try {
        docker run --rm --platform linux/arm64 arm64v8/debian:bookworm-slim true 2>&1 | Out-Null
        if ($LASTEXITCODE -eq 0) { $emuOk = $true }
    } catch {}
    if (-not $emuOk) {
        Write-Host "  Registering arm64 binfmt handlers (tonistiigi/binfmt)..." -ForegroundColor DarkGray
        docker run --privileged --rm tonistiigi/binfmt --install arm64 | Out-Null
    }
    Write-Host ""
}

# ── Build each orchestrator ────────────────────────────────────────────

$built = 0
$failed = 0

foreach ($orch in $selected) {
    $name = $orch.Name
    $dir = Join-Path $orchestratorsDir $orch.Dir
    $dockerfile = Join-Path $dir "Dockerfile"
    $image = "$($orch.Image):$Tag"

    if (-not (Test-Path $dockerfile)) {
        Write-Host "  [$name] SKIP — no Dockerfile" -ForegroundColor Yellow
        continue
    }

    Write-Host "  [$name] Building $image$(if ($Platform) { " ($Platform)" }) ..." -ForegroundColor Cyan

    # Build with workspace root as context.
    # -Platform selects buildx + QEMU emulation (e.g. linux/arm64 for ARM stones);
    # default (empty) keeps the original native `docker build`.
    if ($Platform) {
        docker buildx build --platform $Platform -t $image -f $dockerfile --load $workspaceRoot 2>&1
    } else {
        docker build -t $image -f $dockerfile $workspaceRoot 2>&1
    }
    if ($LASTEXITCODE -ne 0) {
        Write-Host "  [$name] FAILED" -ForegroundColor Red
        $failed++
        continue
    }

    $built++
    Write-Host "  [$name] OK — dashboard :$($orch.Port)" -ForegroundColor Green

    # Push if requested
    if ($Push) {
        Write-Host "  [$name] Pushing $image ..." -ForegroundColor Cyan
        docker push $image
        if ($LASTEXITCODE -ne 0) {
            Write-Host "  [$name] Push FAILED" -ForegroundColor Red
            $failed++
        } else {
            Write-Host "  [$name] Pushed" -ForegroundColor Green
        }
    }
}

# ── Summary ────────────────────────────────────────────────────────────

Write-Host ""
Write-Host "  ─────────────────────────────" -ForegroundColor DarkGray
Write-Host "  Built: $built  Failed: $failed  Total: $($selected.Count)" -ForegroundColor $(if ($failed -gt 0) { "Yellow" } else { "Green" })
Write-Host ""

if ($failed -gt 0) { exit 1 }
