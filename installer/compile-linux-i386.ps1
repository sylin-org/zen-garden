<#
.SYNOPSIS
    Compile Zen Garden Linux i386 (32-bit) binaries using Docker

.DESCRIPTION
    Cross-compiles Zen Garden binaries for i686-unknown-linux-gnu (32-bit Linux).
    Uses a dedicated Docker container with gcc-multilib and i386 libraries.
    Output goes to dist/linux-i386/ (separate from the amd64 dist/linux/).

    This is a parallel pipeline to compile-linux.ps1, not a replacement.
    Use this for 32-bit stones (e.g., Atom Z5xx machines).

.PARAMETER Targets
    List of cargo package names to build (e.g., "garden-moss", "garden-rake")
    If not specified, builds core binaries (moss + rake).

.PARAMETER DebugBuild
    Compile debug binaries instead of optimized release

.PARAMETER Fast
    Use fast-release profile (~40% faster compile, slightly larger binaries)

.PARAMETER ForceRebuild
    Force rebuild of Docker build container

.PARAMETER Jobs
    Number of parallel cargo jobs (default: number of CPUs)

.EXAMPLE
    .\compile-linux-i386.ps1
    # Build core binaries (moss + rake) for i386

.EXAMPLE
    .\compile-linux-i386.ps1 -Targets "garden-moss","garden-rake","garden-lantern"
    # Build specific binaries for i386

.EXAMPLE
    .\compile-linux-i386.ps1 -ForceRebuild
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
$LINUX_I386_DIR = Join-Path $DIST_DIR "linux-i386"
$IMAGE_NAME = "zen-garden-builder-i386:latest"
$CONTAINER_NAME = "zen-garden-builder-i386-container"

# Detect if running on Windows
$RunningOnWindows = if ($null -ne (Get-Variable -Name IsWindows -ValueOnly -ErrorAction SilentlyContinue)) {
    $IsWindows
} else {
    $env:OS -eq "Windows_NT"
}

# Create dist directory
New-Item -ItemType Directory -Force -Path $LINUX_I386_DIR | Out-Null

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
Write-Host "║   Zen Garden Linux i386 Build                      ║" -ForegroundColor Magenta
Write-Host "╚════════════════════════════════════════════════════╝`n" -ForegroundColor Magenta

Write-Host "Configuration:" -ForegroundColor Yellow
Write-Host "  Platform: Linux"
Write-Host "  Architecture: i386 ($RUST_TARGET)"
Write-Host "  Version: $version"
Write-Host "  Build Type: $buildTypeDesc"
Write-Host "  Parallel Jobs: $parallelJobs"
Write-Host "  Output Dir: $LINUX_I386_DIR"
Write-Host '  Build Method: Docker Container [cross-compilation]'
Write-Host ""

# Check Docker availability
try {
    docker version | Out-Null
} catch {
    Write-Host "Docker not available. Docker is required for i386 cross-compilation." -ForegroundColor Red
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

    Push-Location $WORKSPACE_ROOT
    try {
        docker build -f Dockerfile.build-i386 -t $IMAGE_NAME . --quiet
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

Write-Host "Building i386 binaries in container..." -ForegroundColor Cyan
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

    # Default to core binaries only (moss + rake) for i386
    $defaultTargets = @("garden-moss", "garden-rake")
    $buildTargets = if ($Targets -and $Targets.Count -gt 0) { $Targets } else { $defaultTargets }

    foreach ($target in $buildTargets) {
        Write-Host "  -> Building $target (i386)..."
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
            -v "zen-garden-cargo-cache-i386:/root/.cargo" `
            -w /build `
            $IMAGE_NAME `
            tail -f /dev/null

        if ($LASTEXITCODE -ne 0) { throw "Failed to create container" }
    }

    # Clean cached binaries to force version update
    # Cross-compiled binaries live under target/{RUST_TARGET}/{profile}/
    Write-Host "  -> Cleaning cached binaries to ensure version update..." -ForegroundColor DarkGray
    $targetBase = "/build/target/$RUST_TARGET"
    docker exec $CONTAINER_NAME sh -c "rm -f $targetBase/debug/garden-* $targetBase/release/garden-* $targetBase/fast-release/garden-*" 2>$null | Out-Null
    docker exec $CONTAINER_NAME sh -c "rm -rf $targetBase/debug/build/garden-* $targetBase/release/build/garden-* $targetBase/fast-release/build/garden-*" 2>$null | Out-Null
    docker exec $CONTAINER_NAME sh -c "rm -rf $targetBase/debug/incremental/garden* $targetBase/release/incremental/garden* $targetBase/fast-release/incremental/garden*" 2>$null | Out-Null
    docker exec $CONTAINER_NAME sh -c "rm -rf $targetBase/debug/.fingerprint/garden-* $targetBase/release/.fingerprint/garden-* $targetBase/fast-release/.fingerprint/garden-*" 2>$null | Out-Null

    # Execute build
    docker exec -e CARGO_BUILD_NUMBER=$env:CARGO_BUILD_NUMBER -e PKG_CONFIG_ALLOW_CROSS=1 -e PKG_CONFIG_PATH="/usr/lib/i386-linux-gnu/pkgconfig" -e PKG_CONFIG_SYSROOT_DIR="/" $CONTAINER_NAME $buildArgs

    if ($LASTEXITCODE -ne 0) { throw "Build failed" }

    # Copy binaries from container
    # Cross-compiled binaries are at target/{RUST_TARGET}/{profile}/{binary}
    Write-Host "  -> Copying binaries from container..." -ForegroundColor DarkGray
    $copyFailed = $false
    $containerBuildDir = "/build/target/$RUST_TARGET/$buildProfile"

    foreach ($target in $buildTargets) {
        docker cp "${CONTAINER_NAME}:${containerBuildDir}/${target}" "$LINUX_I386_DIR\$target" 2>$null
        if ($LASTEXITCODE -ne 0) {
            Write-Host "    Failed to copy $target" -ForegroundColor Red
            $copyFailed = $true
        }
    }

    if ($copyFailed) { throw "Failed to copy one or more binaries from container" }

    Write-Host "  Linux i386 binaries built`n" -ForegroundColor Green

} finally {
    Pop-Location
}

# Display results
Write-Host "╔════════════════════════════════════════════════════╗" -ForegroundColor Green
Write-Host "║   i386 Build Complete!                             ║" -ForegroundColor Green
Write-Host "╚════════════════════════════════════════════════════╝`n" -ForegroundColor Green

Write-Host "Artifacts in $LINUX_I386_DIR`:" -ForegroundColor Cyan

$artifacts = Get-ChildItem $LINUX_I386_DIR -ErrorAction SilentlyContinue
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
            $fileType = docker run --rm -v "${LINUX_I386_DIR}:/check" $IMAGE_NAME file "/check/$($_.Name)" 2>$null
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
Write-Host "  1. Install Debian i386 on the target machine"
Write-Host "  2. SCP binaries from dist/linux-i386/ to the stone"
Write-Host "  (Build container cached for next run)" -ForegroundColor DarkGray
Write-Host ""
