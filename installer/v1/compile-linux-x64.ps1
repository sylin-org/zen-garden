<#
.SYNOPSIS
    Compile the Zen Garden v1 'garden' binary for Linux x64 using Docker.

.DESCRIPTION
    Perennial-builder pattern (mirrors ../compile-linux-x64.ps1, simplified):
    - Reuses a long-lived build container/image across runs; rebuilds only when
      this Dockerfile changes or src/v1/Cargo.lock changes (dependency sync).
    - Cargo registry + target live in named volumes for warm incremental builds.
    - v1 is koi-free and dependency-free at the native level: no koi mount,
      no libudev/clang/mold needed.

.PARAMETER ForceRebuild
    Rebuild the builder image regardless of staleness.

.PARAMETER DebugBuild
    Build debug instead of release.

.EXAMPLE
    .\compile-linux-x64.ps1              # release build to dist\v1\linux-x64\
    .\compile-linux-x64.ps1 -ForceRebuild
#>
[CmdletBinding()]
param(
    [switch]$ForceRebuild,
    [switch]$DebugBuild
)

$ErrorActionPreference = "Stop"

$script:ImageName = "zen-builder-v1-linux-x64:latest"
$script:ContainerName = "zen-builder-v1-linux-x64"
$script:CargoVolume = "zen-cargo-v1"
$script:TargetVolume = "zen-target-v1"

$repoRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)   # installer/v1 -> repo root
$v1Workspace = Join-Path $repoRoot "src\v1"
$distDir = Join-Path $repoRoot "dist\v1\linux-x64"
$lockFile = Join-Path $v1Workspace "Cargo.lock"
$markerFile = Join-Path $distDir ".container-lockhash"
$profile = if ($DebugBuild) { "debug" } else { "release" }
$binaryName = "garden"

function Test-Docker {
    try { docker version *> $null; return $LASTEXITCODE -eq 0 } catch { return $false }
}

if (-not (Test-Docker)) {
    Write-Host "Docker is not available. Start Docker Desktop and retry." -ForegroundColor Red
    exit 1
}

New-Item -ItemType Directory -Force -Path $distDir | Out-Null

# --- staleness: image missing, Dockerfile newer than marker, or lockfile changed
$currentLockHash = if (Test-Path $lockFile) {
    (Get-FileHash $lockFile -Algorithm SHA256).Hash.Substring(0, 16)
} else { "" }
$storedLockHash = if (Test-Path $markerFile) { (Get-Content $markerFile -Raw).Trim() } else { "" }
$imageExists = [bool](docker images -q $ImageName 2>$null)

$dockfile = Join-Path $PSScriptRoot "Dockerfile.linux-x64"
$dockfileNewer = (Test-Path $markerFile) -and
    ((Get-Item $dockfile).LastWriteTime -gt (Get-Item $markerFile).LastWriteTime)

$needsBuild = $ForceRebuild -or (-not $imageExists) -or `
    ($currentLockHash -ne $storedLockHash) -or $dockfileNewer

if ($needsBuild) {
    Write-Host "Building image $ImageName ($(if ($ForceRebuild) {'forced'} elseif (-not $imageExists) {'missing'} elseif ($currentLockHash -ne $storedLockHash) {'Cargo.lock changed'} else {'Dockerfile changed'}))..." -ForegroundColor Cyan
    docker build -f $dockfile -t $ImageName $PSScriptRoot --quiet
    if ($LASTEXITCODE -ne 0) { Write-Host "docker build failed." -ForegroundColor Red; exit 1 }
} else {
    Write-Host "Using existing image: $ImageName" -ForegroundColor Green
}

# --- remove any stale container from a previous run
docker rm -f $ContainerName 2>$null | Out-Null

Write-Host "Starting perennial container (cargo cache: $CargoVolume, target: $TargetVolume)..." -ForegroundColor Cyan
docker run -d --name $ContainerName `
    -v "${v1Workspace}:/build" `
    -v "${CargoVolume}:/usr/local/cargo/registry" `
    -v "${TargetVolume}:/build/target" `
    $ImageName sleep infinity | Out-Null
if ($LASTEXITCODE -ne 0) { Write-Host "docker run failed." -ForegroundColor Red; exit 1 }

try {
    Write-Host "Compiling garden ($profile)..." -ForegroundColor Cyan
    docker exec $ContainerName cargo build --${profile} -p garden-daemon
    if ($LASTEXITCODE -ne 0) { Write-Host "cargo build failed." -ForegroundColor Red; exit 1 }

    # docker cp out (volume mounts may not reflect immediately on Windows)
    $outPath = Join-Path $distDir $binaryName
    docker cp "${ContainerName}:/build/target/${profile}/${binaryName}" $outPath
    if ($LASTEXITCODE -ne 0) { Write-Host "docker cp failed." -ForegroundColor Red; exit 1 }

    # record lock hash only after a successful compile with current lockfile
    Set-Content -Path $markerFile -Value $currentLockHash -NoNewline
    Write-Host "OK Binary at $outPath" -ForegroundColor Green
} finally {
    docker rm -f $ContainerName 2>$null | Out-Null
}
