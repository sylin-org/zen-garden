<#
.SYNOPSIS
    Compile Zen Garden Linux x86 (32-bit) binaries using Docker

.DESCRIPTION
    Cross-compiles Zen Garden binaries for i686-unknown-linux-gnu (32-bit Linux).
    Uses a dedicated Docker container with gcc-multilib and x86 cross-compilation libraries.
    Output goes to dist/linux-x86/ (separate from the x64 dist/linux-x64/).

    This is a parallel pipeline to compile-linux-x64.ps1, not a replacement.
    Use this for 32-bit stones (e.g., Atom Z5xx machines).

.PARAMETER Targets
    List of cargo package names to build (e.g., "garden-moss", "garden-rake")
    If not specified, builds all binaries (moss, lantern, rake, cricket, firefly).

.PARAMETER DebugBuild
    Compile debug binaries instead of optimized release

.PARAMETER Fast
    Use fast-release profile (~40% faster compile, slightly larger binaries)

.PARAMETER ForceRebuild
    Force rebuild of Docker build container

.PARAMETER Jobs
    Number of parallel cargo jobs (default: number of CPUs)

.EXAMPLE
    .\compile-linux-x86.ps1
    # Build all binaries for x86

.EXAMPLE
    .\compile-linux-x86.ps1 -Targets "garden-moss","garden-rake"
    # Build specific binaries for x86

.EXAMPLE
    .\compile-linux-x86.ps1 -ForceRebuild
    # Rebuild Docker image and compile
#>

[CmdletBinding()]
param(
    [string]$Version,
    [string[]]$Targets,
    [switch]$DebugBuild,
    [switch]$Fast,
    [switch]$ForceRebuild,
    [int]$Jobs = 0
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$RUST_TARGET = "i686-unknown-linux-gnu"
$WORKSPACE_ROOT = (Get-Item $PSScriptRoot).Parent.FullName
$DIST_DIR = Join-Path $WORKSPACE_ROOT "dist"
$LINUX_X86_DIR = Join-Path $DIST_DIR "linux-x86"
$IMAGE_NAME = "zen-builder-linux-x86:latest"
$CONTAINER_NAME = "zen-builder-linux-x86"
$CARGO_CACHE_VOLUME = "zen-cargo-cache-linux-x86"

# Detect if running on Windows
$RunningOnWindows = if ($null -ne (Get-Variable -Name IsWindows -ValueOnly -ErrorAction SilentlyContinue)) {
    $IsWindows
} else {
    $env:OS -eq "Windows_NT"
}

# Create dist directory
New-Item -ItemType Directory -Force -Path $LINUX_X86_DIR | Out-Null

# Version handling
if ($Version) {
    $env:GARDEN_VERSION = $Version
    $parts = $Version.Split('.')
    if ($parts.Length -ge 3) {
        $env:BUILD_NUMBER = $parts[2]
        $env:CARGO_BUILD_NUMBER = $parts[2]
    }
} elseif (-not $env:GARDEN_VERSION) {
    $revision = (Get-Date).ToString("yyyyMMddHHmm")
    $env:GARDEN_VERSION = "0.1.$revision"
    $env:BUILD_NUMBER = $revision
    $env:CARGO_BUILD_NUMBER = $revision
    Write-Host "Warning: Version not set, using default: $env:GARDEN_VERSION" -ForegroundColor Yellow
    Write-Host ""
}
$version = $env:GARDEN_VERSION

# Build profile
$buildProfile = if ($DebugBuild) { "debug" } elseif ($Fast) { "fast-release" } else { "release" }
$buildTypeDesc = switch ($buildProfile) {
    "debug" { "Debug (fastest compile, largest binary)" }
    "fast-release" { "Fast-Release (thin LTO, ~40% faster compile)" }
    default { "Release (full LTO, smallest binary)" }
}

$parallelJobs = if ($Jobs -gt 0) { $Jobs } else { [Environment]::ProcessorCount }

Write-Host "`n╔════════════════════════════════════════════════════╗" -ForegroundColor Magenta
Write-Host "║   Zen Garden Linux x86 Build                       ║" -ForegroundColor Magenta
Write-Host "╚════════════════════════════════════════════════════╝`n" -ForegroundColor Magenta

Write-Host "Configuration:" -ForegroundColor Yellow
Write-Host "  Platform: Linux"
Write-Host "  Architecture: x86 ($RUST_TARGET)"
Write-Host "  Version: $version"
Write-Host "  Build Type: $buildTypeDesc"
Write-Host "  Parallel Jobs: $parallelJobs"
Write-Host "  Output Dir: $LINUX_X86_DIR"
Write-Host '  Build Method: Docker Container [cross-compilation]'
Write-Host ""

# Check Docker availability
try {
    docker version | Out-Null
} catch {
    Write-Host "Docker not available. Docker is required for x86 cross-compilation." -ForegroundColor Red
    if ($RunningOnWindows) {
        Write-Host "  Install Docker Desktop: https://www.docker.com/products/docker-desktop/" -ForegroundColor Yellow
    }
    exit 1
}

# Build or reuse Docker image
$existingImage = $null
try { $existingImage = docker images -q $IMAGE_NAME 2>&1 | Where-Object { $_ -is [string] } } catch {}

if ($existingImage -and -not $ForceRebuild) {
    Write-Host "Build Container:" -ForegroundColor Yellow
    Write-Host "  Using existing image: $IMAGE_NAME" -ForegroundColor Green
    Write-Host "    (Use -ForceRebuild to recreate)" -ForegroundColor DarkGray
    Write-Host ""
} else {
    Write-Host "Build Container:" -ForegroundColor Yellow
    Write-Host "  $(if ($ForceRebuild) { 'Rebuilding' } else { 'Creating' }) image: $IMAGE_NAME"

    if ($ForceRebuild) {
        # Remove existing container and cargo cache volume to avoid stale
        # build-script binaries compiled against a different glibc version.
        # The x86 Dockerfile is pinned to rust:bookworm (glibc 2.36) — if the
        # cargo cache was populated under rust:latest (glibc 2.39), host-arch
        # build scripts (libsqlite3-sys, alsa-sys, etc.) will fail at runtime.
        Write-Host "  Removing old container and cargo cache..." -ForegroundColor DarkGray
        docker rm -f $CONTAINER_NAME 2>$null | Out-Null
        docker volume rm $CARGO_CACHE_VOLUME 2>$null | Out-Null
    }

    Push-Location $WORKSPACE_ROOT
    try {
        docker build -f Dockerfile.linux-x86 -t $IMAGE_NAME . --quiet
        if ($LASTEXITCODE -ne 0) { throw "Docker build failed" }
        Write-Host "  Image ready`n" -ForegroundColor Green
    } finally {
        Pop-Location
    }
}

# Determine build arguments
$buildProfile = if ($DebugBuild) { "debug" }
                elseif ($Fast) { "fast-release" }
                else { "release" }

$parallelJobs = if ($Jobs -gt 0) { $Jobs } else { [Environment]::ProcessorCount }

Write-Host "Building x86 binaries in container..." -ForegroundColor Cyan
$buildTypeDesc = switch ($buildProfile) {
    "debug" { "Debug" }
    "fast-release" { "Fast-Release (thin LTO)" }
    default { "Release (full LTO)" }
}
Write-Host "  Build Type: $buildTypeDesc, Jobs: $parallelJobs" -ForegroundColor DarkGray

# Volume mount path
if ($RunningOnWindows) {
    $driveLetter = $WORKSPACE_ROOT.Substring(0, 1).ToLower()
    $unixPath = "/$driveLetter" + $WORKSPACE_ROOT.Substring(2).Replace('\', '/')
} else {
    $unixPath = $WORKSPACE_ROOT
}

Push-Location $WORKSPACE_ROOT
try {
    # Build number
    if (-not $env:CARGO_BUILD_NUMBER) {
        $env:CARGO_BUILD_NUMBER = (Get-Date).ToString("yyyyMMdd.HHmm")
        Write-Host "  Build Number: $env:CARGO_BUILD_NUMBER" -ForegroundColor Cyan
    }

    $defaultTargets = @("garden-moss", "garden-lantern", "garden-rake", "garden-cricket", "garden-firefly")
    $buildTargets = if ($Targets -and $Targets.Count -gt 0) { $Targets } else { $defaultTargets }

    # Build Lantern frontend (on host, before Docker cargo build)
    if ($buildTargets -contains "garden-lantern") {
        $frontendDir = Join-Path $WORKSPACE_ROOT "src/lantern/frontend"
        if (Test-Path (Join-Path $frontendDir "package.json")) {
            Write-Host "Building Lantern frontend SPA..." -ForegroundColor Yellow

            $hasBun = Get-Command bun -ErrorAction SilentlyContinue
            $hasNpm = Get-Command npm -ErrorAction SilentlyContinue

            Push-Location $frontendDir
            try {
                if ($hasBun) {
                    Write-Host "  Using bun..." -ForegroundColor DarkGray
                    bun install --frozen-lockfile 2>$null
                    if ($LASTEXITCODE -ne 0) { bun install }
                    & ./node_modules/.bin/vite build
                } elseif ($hasNpm) {
                    Write-Host "  Using npm..." -ForegroundColor DarkGray
                    npm ci 2>$null
                    if ($LASTEXITCODE -ne 0) { npm install }
                    npx vite build
                } else {
                    Write-Host "  ⚠ Neither bun nor npm found — skipping frontend build" -ForegroundColor Yellow
                    Write-Host "    Lantern will embed whatever is in frontend/dist/" -ForegroundColor DarkGray
                }

                if ($LASTEXITCODE -eq 0 -and (Test-Path (Join-Path $frontendDir "dist/index.html"))) {
                    Write-Host "  ✓ Lantern frontend built`n" -ForegroundColor Green
                } elseif ($LASTEXITCODE -ne 0) {
                    Write-Host "  ⚠ Frontend build failed (exit code $LASTEXITCODE) — continuing with cargo build`n" -ForegroundColor Yellow
                }
            } finally {
                Pop-Location
            }
        }
    }

    foreach ($target in $buildTargets) {
        Write-Host "  -> Building $target (x86)..."
    }

    # Cargo build args with --target for cross-compilation
    $buildArgs = @("cargo", "build", "--target", $RUST_TARGET, "-j", "$parallelJobs")
    if ($buildProfile -eq "debug") {
        # Debug build - no profile flag needed
    } elseif ($buildProfile -eq "fast-release") {
        $buildArgs += @("--profile", "fast-release")
    } else {
        $buildArgs += "--release"
    }
    foreach ($target in $buildTargets) {
        $buildArgs += @("--bin", $target)
    }

    # Container management
    $existingContainer = $null
    $existingContainer = docker ps --filter "name=^/${CONTAINER_NAME}$" --format "{{.Names}}" 2>$null
    $stoppedContainer = $null
    $stoppedContainer = docker ps -a --filter "name=^/${CONTAINER_NAME}$" --filter "status=exited" --format "{{.Names}}" 2>$null

    if ($existingContainer -eq $CONTAINER_NAME) {
        Write-Host "  -> Using running container: $CONTAINER_NAME" -ForegroundColor DarkGray
    } elseif ($stoppedContainer -eq $CONTAINER_NAME) {
        Write-Host "  -> Starting existing container: $CONTAINER_NAME" -ForegroundColor DarkGray
        docker start $CONTAINER_NAME | Out-Null
        if ($LASTEXITCODE -ne 0) { throw "Failed to start container" }
    } else {
        Write-Host "  -> Creating new container: $CONTAINER_NAME" -ForegroundColor DarkGray

        docker run -d `
            --name $CONTAINER_NAME `
            -v "${unixPath}:/build" `
            -v "${CARGO_CACHE_VOLUME}:/root/.cargo" `
            -w /build `
            $IMAGE_NAME `
            tail -f /dev/null

        if ($LASTEXITCODE -ne 0) { throw "Failed to create container" }
    }

    # Clean cached garden binaries to force version update
    # x86 uses a separate target dir (target-linux-x86/) to avoid glibc conflicts
    # with the x64 builder which uses target-linux-x64/ and a different base image.
    Write-Host "  -> Cleaning cached binaries to ensure version update..." -ForegroundColor DarkGray
    $targetBase = "/build/target-linux-x86/$RUST_TARGET"
    docker exec $CONTAINER_NAME sh -c "rm -f $targetBase/debug/garden-* $targetBase/release/garden-* $targetBase/fast-release/garden-*" 2>$null | Out-Null
    docker exec $CONTAINER_NAME sh -c "rm -rf $targetBase/debug/build/garden-* $targetBase/release/build/garden-* $targetBase/fast-release/build/garden-*" 2>$null | Out-Null
    docker exec $CONTAINER_NAME sh -c "rm -rf $targetBase/debug/incremental/garden* $targetBase/release/incremental/garden* $targetBase/fast-release/incremental/garden*" 2>$null | Out-Null
    docker exec $CONTAINER_NAME sh -c "rm -rf $targetBase/debug/.fingerprint/garden-* $targetBase/release/.fingerprint/garden-* $targetBase/fast-release/.fingerprint/garden-*" 2>$null | Out-Null

    # Execute build with separate target directory
    docker exec -e CARGO_BUILD_NUMBER=$env:CARGO_BUILD_NUMBER -e CARGO_TARGET_DIR=/build/target-linux-x86 -e PKG_CONFIG_ALLOW_CROSS=1 -e PKG_CONFIG_PATH="/usr/lib/i386-linux-gnu/pkgconfig" -e PKG_CONFIG_SYSROOT_DIR="/" $CONTAINER_NAME $buildArgs

    if ($LASTEXITCODE -ne 0) { throw "Build failed" }

    # Copy binaries from container
    # Cross-compiled binaries are at target-linux-x86/{RUST_TARGET}/{profile}/{binary}
    Write-Host "  -> Copying binaries from container..." -ForegroundColor DarkGray
    $copyFailed = $false
    $containerBuildDir = "/build/target-linux-x86/$RUST_TARGET/$buildProfile"

    foreach ($target in $buildTargets) {
        docker cp "${CONTAINER_NAME}:${containerBuildDir}/${target}" "$LINUX_X86_DIR\$target" 2>$null
        if ($LASTEXITCODE -ne 0) {
            Write-Host "    Failed to copy $target" -ForegroundColor Red
            $copyFailed = $true
        }
    }

    if ($copyFailed) { throw "Failed to copy one or more binaries from container" }

    Write-Host "  Linux x86 binaries built`n" -ForegroundColor Green

} finally {
    Pop-Location
}

# Display results
Write-Host "╔════════════════════════════════════════════════════╗" -ForegroundColor Green
Write-Host "║   Linux x86 Build Complete!                        ║" -ForegroundColor Green
Write-Host "╚════════════════════════════════════════════════════╝`n" -ForegroundColor Green

Write-Host "Artifacts in $LINUX_X86_DIR`:" -ForegroundColor Cyan

$artifacts = Get-ChildItem $LINUX_X86_DIR -ErrorAction SilentlyContinue
if ($artifacts) {
    $artifacts | ForEach-Object {
        $sizeMB = [math]::Round($_.Length / 1MB, 2)
        $sizeStr = if ($sizeMB -lt 1) {
            "$([math]::Round($_.Length / 1KB, 0)) KB"
        } else {
            "$sizeMB MB"
        }

        # Verify binary type
        $marker = "-"
        try {
            $fileType = docker run --rm -v "${LINUX_X86_DIR}:/check" $IMAGE_NAME file "/check/$($_.Name)" 2>$null
            $isLinuxBinary = $fileType -match "ELF 32-bit.*Linux"
            $marker = if ($isLinuxBinary) { "OK" } else { "?" }
        } catch {
            $marker = "-"
        }

        Write-Host ("  {0} {1,-20} {2,10}" -f $marker, $_.Name, $sizeStr) -ForegroundColor $(if ($marker -eq "OK") { "Green" } else { "White" })
    }
} else {
    Write-Host "  (no artifacts found)" -ForegroundColor DarkGray
}

Write-Host "`nNext steps:" -ForegroundColor Yellow
Write-Host "  1. Install Debian x86 on the target machine"
Write-Host "  2. SCP binaries from dist/linux-x86/ to the stone"
Write-Host "  (Build container cached for next run)" -ForegroundColor DarkGray
Write-Host ""
