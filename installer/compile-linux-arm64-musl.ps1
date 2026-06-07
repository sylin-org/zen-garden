<#
.SYNOPSIS
    Compile fully-static aarch64 musl Zen Garden binaries (native phone Stone) using Docker.

.DESCRIPTION
    Builds garden-moss (and garden-rake) as fully-static aarch64-unknown-linux-musl binaries
    that run natively on Android/bionic — moss is the HOST daemon, not a container (STONE-0001).

    Built musl-natively inside an Alpine arm64 container under QEMU (sidesteps cross-toolchain
    pain). moss is compiled with --no-default-features to drop the optional `udev` feature
    (no libudev on Android; the storage monitor falls back to polling). garden-rake builds
    normally. Output: dist/linux-arm64-musl/.

    Parallel pipeline to compile-linux-arm64.ps1 (which produces glibc binaries for ARM64
    *Linux* stones such as Raspberry Pi). This musl pipeline is specifically for Android.

.PARAMETER Targets
    Cargo package names to build. Default: garden-moss, garden-rake.

.PARAMETER DebugBuild
    Debug binaries (faster compile under emulation; large).

.PARAMETER Fast
    fast-release profile (thin LTO).

.PARAMETER Release
    Full-release profile (full LTO; default if neither Debug nor Fast).

.PARAMETER ForceRebuild
    Rebuild the Docker builder image.

.PARAMETER Jobs
    Parallel cargo jobs (default: CPU count).
#>

[CmdletBinding()]
param(
    [string]$Version,
    [string[]]$Targets,
    [switch]$DebugBuild,
    [switch]$Fast,
    [switch]$Release,
    [switch]$ForceRebuild,
    [int]$Jobs = 0
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$RUST_TARGET = "aarch64-unknown-linux-musl"
$WORKSPACE_ROOT = (Get-Item $PSScriptRoot).Parent.FullName
$DIST_DIR = Join-Path $WORKSPACE_ROOT "dist"
$OUT_DIR = Join-Path $DIST_DIR "linux-arm64-musl"
$IMAGE_NAME = "zen-builder-linux-arm64-musl:latest"
$CONTAINER_NAME = "zen-builder-linux-arm64-musl"
$CARGO_CACHE_VOLUME = "zen-cargo-cache-linux-arm64-musl"
$TARGET_VOLUME = "zen-target-linux-arm64-musl"

$RunningOnWindows = if ($null -ne (Get-Variable -Name IsWindows -ValueOnly -ErrorAction SilentlyContinue)) { $IsWindows } else { $env:OS -eq "Windows_NT" }

New-Item -ItemType Directory -Force -Path $OUT_DIR | Out-Null

# Version / build number (embedded via build.rs rerun-if-env-changed=CARGO_BUILD_NUMBER)
if ($Version) {
    $env:GARDEN_VERSION = $Version
    $parts = $Version.Split('.')
    if ($parts.Length -ge 3) { $env:BUILD_NUMBER = $parts[2]; $env:CARGO_BUILD_NUMBER = $parts[2] }
} elseif (-not $env:CARGO_BUILD_NUMBER) {
    $revision = (Get-Date).ToString("yyyyMMddHHmm")
    $env:GARDEN_VERSION = "0.1.$revision"; $env:CARGO_BUILD_NUMBER = $revision
}

$buildProfile = if ($DebugBuild) { "debug" } elseif ($Fast) { "fast-release" } else { "release" }
$parallelJobs = if ($Jobs -gt 0) { $Jobs } else { [Environment]::ProcessorCount }

Write-Host "`n+====================================================+" -ForegroundColor Magenta
Write-Host "|   Zen Garden Linux ARM64 musl Build (native)       |" -ForegroundColor Magenta
Write-Host "+====================================================+`n" -ForegroundColor Magenta
Write-Host "  Architecture: aarch64-musl (static, runs on Android/bionic)"
Write-Host "  Profile: $buildProfile   Jobs: $parallelJobs"
Write-Host "  Output: $OUT_DIR`n"

try { docker version | Out-Null } catch { Write-Host "Docker not available." -ForegroundColor Red; exit 1 }

# Ensure arm64 emulation (the builder image + container run under QEMU on an x64 host).
$emuOk = $false
try { docker run --rm --platform linux/arm64 arm64v8/debian:bookworm-slim true 2>&1 | Out-Null; if ($LASTEXITCODE -eq 0) { $emuOk = $true } } catch {}
if (-not $emuOk) {
    Write-Host "Registering arm64 binfmt handlers..." -ForegroundColor DarkGray
    docker run --privileged --rm tonistiigi/binfmt --install arm64 | Out-Null
}

# Build / reuse the Alpine arm64 musl builder image
$existingImage = $null
try { $existingImage = docker images -q $IMAGE_NAME 2>&1 | Where-Object { $_ -is [string] } } catch {}
$lockfileStale = $false
$markerFile = Join-Path $DIST_DIR ".builder-lock-hash-arm64-musl"
$lockHash = (Get-FileHash (Join-Path $WORKSPACE_ROOT "Cargo.lock") -Algorithm SHA256).Hash.Substring(0, 16)
if ($existingImage -and -not $ForceRebuild -and (Test-Path $markerFile)) {
    if ((Get-Content $markerFile -ErrorAction SilentlyContinue) -ne $lockHash) { $lockfileStale = $true }
}
if ($existingImage -and -not $ForceRebuild -and -not $lockfileStale) {
    Write-Host "Build Container: using existing image $IMAGE_NAME" -ForegroundColor Green
} else {
    if ($ForceRebuild -or $lockfileStale) {
        docker rm -f $CONTAINER_NAME 2>$null | Out-Null
        docker volume rm $CARGO_CACHE_VOLUME 2>$null | Out-Null
    }
    Write-Host "Build Container: building image $IMAGE_NAME" -ForegroundColor Yellow
    Push-Location $WORKSPACE_ROOT
    try {
        docker buildx build --platform linux/arm64 -f Dockerfile.linux-arm64-musl -t $IMAGE_NAME --load .
        if ($LASTEXITCODE -ne 0) { throw "Docker image build failed" }
        New-Item -ItemType Directory -Path $DIST_DIR -Force | Out-Null
        $lockHash | Out-File $markerFile -NoNewline
    } finally { Pop-Location }
}

# Mount paths (workspace + sibling koi)
if ($RunningOnWindows) {
    $unixPath = "/$($WORKSPACE_ROOT.Substring(0,1).ToLower())" + $WORKSPACE_ROOT.Substring(2).Replace('\', '/')
    $koiHostPath = (Resolve-Path (Join-Path $WORKSPACE_ROOT "../koi")).Path
    $koiUnixPath = "/$($koiHostPath.Substring(0,1).ToLower())" + $koiHostPath.Substring(2).Replace('\', '/')
} else {
    $unixPath = $WORKSPACE_ROOT
    $koiUnixPath = (Resolve-Path (Join-Path $WORKSPACE_ROOT "../koi")).Path
}

$defaultTargets = @("garden-moss", "garden-rake")
$buildTargets = if ($Targets -and $Targets.Count -gt 0) { $Targets } else { $defaultTargets }

Push-Location $WORKSPACE_ROOT
try {
    # Create / reuse the (emulated arm64) builder container
    $running = docker ps --filter "name=^/${CONTAINER_NAME}$" --format "{{.Names}}" 2>$null
    $stopped = docker ps -a --filter "name=^/${CONTAINER_NAME}$" --filter "status=exited" --format "{{.Names}}" 2>$null
    if ($running -eq $CONTAINER_NAME) {
        Write-Host "  -> Using running container" -ForegroundColor DarkGray
    } elseif ($stopped -eq $CONTAINER_NAME) {
        docker start $CONTAINER_NAME | Out-Null
    } else {
        Write-Host "  -> Creating builder container (arm64/QEMU)" -ForegroundColor DarkGray
        docker run -d --platform linux/arm64 `
            --name $CONTAINER_NAME `
            -v "${unixPath}:/build:ro" `
            -v "${koiUnixPath}:/koi:ro" `
            -v "${CARGO_CACHE_VOLUME}:/root/.cargo" `
            -v "${TARGET_VOLUME}:/target" `
            -w /build `
            $IMAGE_NAME `
            tail -f /dev/null
        if ($LASTEXITCODE -ne 0) { throw "Failed to create builder container" }
    }

    # Ensure the container cargo cache has all lockfile crates (first run after a dep change).
    docker exec $CONTAINER_NAME cargo fetch --manifest-path /build/Cargo.toml 2>&1 | Out-Null

    # Profile flag
    $profileArgs = @()
    if ($buildProfile -eq "fast-release") { $profileArgs = @("--profile", "fast-release") }
    elseif ($buildProfile -eq "release") { $profileArgs = @("--release") }

    # moss: --no-default-features drops the optional udev feature (no libudev on Android).
    # rake: builds normally. Separate invocations: --no-default-features cannot span packages.
    foreach ($target in $buildTargets) {
        Write-Host "  -> Building $target (aarch64-musl, static)..." -ForegroundColor Cyan
        $cargoArgs = @("cargo", "build", "--frozen", "--target", $RUST_TARGET, "-j", "$parallelJobs") + $profileArgs
        if ($target -eq "garden-moss") { $cargoArgs += "--no-default-features" }
        $cargoArgs += @("--bin", $target)
        docker exec -e CARGO_BUILD_NUMBER=$env:CARGO_BUILD_NUMBER -e CARGO_TARGET_DIR=/target $CONTAINER_NAME $cargoArgs
        if ($LASTEXITCODE -ne 0) { throw "Build failed for $target" }
    }

    # Copy binaries out
    $containerBuildDir = "/target/$RUST_TARGET/$buildProfile"
    foreach ($target in $buildTargets) {
        docker cp "${CONTAINER_NAME}:${containerBuildDir}/${target}" "$OUT_DIR\$target" 2>$null
        if ($LASTEXITCODE -ne 0) { throw "Failed to copy $target from container" }
    }
    Write-Host "  Binaries copied`n" -ForegroundColor Green
} finally { Pop-Location }

# Report + verify static aarch64
Write-Host "+====================================================+" -ForegroundColor Green
Write-Host "|   Linux ARM64 musl Build Complete!                 |" -ForegroundColor Green
Write-Host "+====================================================+`n" -ForegroundColor Green
Get-ChildItem $OUT_DIR -ErrorAction SilentlyContinue | ForEach-Object {
    $sizeMB = [math]::Round($_.Length / 1MB, 2)
    $marker = "-"
    try {
        $ft = docker run --rm --platform linux/arm64 -v "${OUT_DIR}:/check" $IMAGE_NAME file "/check/$($_.Name)" 2>$null
        $marker = if ($ft -match "ARM aarch64" -and $ft -match "statically linked") { "OK(static)" } elseif ($ft -match "ARM aarch64") { "OK(dynamic?)" } else { "?" }
    } catch {}
    Write-Host ("  {0,-12} {1,-16} {2,8} MB" -f $marker, $_.Name, $sizeMB) -ForegroundColor $(if ($marker -like "OK*") { "Green" } else { "White" })
}
Write-Host "`nNext: installer/deploy-android.ps1 (push native moss + start)`n" -ForegroundColor Yellow
