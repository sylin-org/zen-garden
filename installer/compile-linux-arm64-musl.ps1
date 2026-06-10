<#
.SYNOPSIS
    Compile fully-static aarch64 musl Zen Garden binaries (native phone Stone) via cross-compilation.

.DESCRIPTION
    Builds Zen Garden binaries as fully-static aarch64-unknown-linux-musl executables that run
    natively on Android/bionic (moss is the HOST daemon, not a container — STONE-0001).

    Cross-compiled with the messense/rust-musl-cross:aarch64-musl toolchain (x64 host ->
    aarch64-musl) — the toolchain that produced the binaries currently running on the phone Stone.
    (Previously this built inside an Alpine arm64 container under QEMU, which was ~10x slower and
    emulation-fragile.)

    moss and cricket compile with --no-default-features to drop optional native deps absent on
    Android (udev/libudev for moss; rodio/libasound for cricket -> null audio backend). rake and
    other targets build normally. Output: dist/linux-arm64-musl/.

    Parallel pipeline to compile-linux-arm64.ps1 (glibc binaries for ARM64 *Linux* stones such as
    Raspberry Pi). This musl pipeline is specifically for Android.

.PARAMETER Version
    Version string (sets GARDEN_VERSION / CARGO_BUILD_NUMBER embedded in the binary).

.PARAMETER Targets
    Cargo package names to build. Default: garden-moss, garden-rake.

.PARAMETER DebugBuild
    Debug binaries (fast compile; large).

.PARAMETER Fast
    fast-release profile (thin LTO).

.PARAMETER Release
    Full-release profile (full LTO; default if neither Debug nor Fast).

.PARAMETER ForceRebuild
    Clear the cross target volume for a clean build.

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
$IMAGE = "messense/rust-musl-cross:aarch64-musl"
$CARGO_CACHE_VOLUME = "zen-cargo-musl-cross"
$TARGET_VOLUME = "zen-target-musl-cross"

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
Write-Host "|   Zen Garden Linux ARM64 musl Build (cross)        |" -ForegroundColor Magenta
Write-Host "+====================================================+`n" -ForegroundColor Magenta
Write-Host "  Architecture: aarch64-musl (static, runs on Android/bionic)"
Write-Host "  Toolchain: $IMAGE"
Write-Host "  Profile: $buildProfile   Jobs: $parallelJobs"
Write-Host "  Output: $OUT_DIR`n"

try { docker version | Out-Null } catch { Write-Host "Docker not available." -ForegroundColor Red; exit 1 }

if ($ForceRebuild) {
    Write-Host "ForceRebuild: clearing target volume $TARGET_VOLUME" -ForegroundColor Yellow
    docker volume rm $TARGET_VOLUME 2>$null | Out-Null
}

# Mount paths (workspace + sibling koi + output dir)
if ($RunningOnWindows) {
    $unixPath = "/$($WORKSPACE_ROOT.Substring(0,1).ToLower())" + $WORKSPACE_ROOT.Substring(2).Replace('\', '/')
    $koiHostPath = (Resolve-Path (Join-Path $WORKSPACE_ROOT "../koi")).Path
    $koiUnixPath = "/$($koiHostPath.Substring(0,1).ToLower())" + $koiHostPath.Substring(2).Replace('\', '/')
    $outUnixPath = "/$($OUT_DIR.Substring(0,1).ToLower())" + $OUT_DIR.Substring(2).Replace('\', '/')
} else {
    $unixPath = $WORKSPACE_ROOT
    $koiUnixPath = (Resolve-Path (Join-Path $WORKSPACE_ROOT "../koi")).Path
    $outUnixPath = $OUT_DIR
}

$defaultTargets = @("garden-moss", "garden-rake")
$buildTargets = if ($Targets -and $Targets.Count -gt 0) { $Targets } else { $defaultTargets }

$profileArgs = @()
if ($buildProfile -eq "fast-release") { $profileArgs = @("--profile", "fast-release") }
elseif ($buildProfile -eq "release") { $profileArgs = @("--release") }

# Populate the cargo cache (idempotent; --frozen requires every lockfile crate to be present).
Write-Host "  -> cargo fetch (populate cross cache)..." -ForegroundColor DarkGray
docker run --rm `
    -v "${unixPath}:/build:ro" -v "${koiUnixPath}:/koi:ro" `
    -v "${CARGO_CACHE_VOLUME}:/root/.cargo" `
    -w /build $IMAGE cargo fetch --manifest-path /build/Cargo.toml 2>&1 | Out-Null

# moss/cricket need --no-default-features (no libudev/libasound on musl). rake builds normally.
# Separate invocations: --no-default-features cannot span packages.
foreach ($target in $buildTargets) {
    Write-Host "  -> Building $target (aarch64-musl, cross)..." -ForegroundColor Cyan
    $cargoArgs = @("cargo", "build", "--frozen", "--target", $RUST_TARGET, "-j", "$parallelJobs") + $profileArgs
    if ($target -eq "garden-moss" -or $target -eq "garden-cricket") { $cargoArgs += "--no-default-features" }
    $cargoArgs += @("--bin", $target)

    $dockerArgs = @(
        "run", "--rm",
        "-v", "${unixPath}:/build:ro",
        "-v", "${koiUnixPath}:/koi:ro",
        "-v", "${CARGO_CACHE_VOLUME}:/root/.cargo",
        "-v", "${TARGET_VOLUME}:/target",
        "-w", "/build",
        "-e", "CARGO_TARGET_DIR=/target",
        "-e", "CARGO_BUILD_NUMBER=$env:CARGO_BUILD_NUMBER",
        $IMAGE
    ) + $cargoArgs
    docker @dockerArgs
    if ($LASTEXITCODE -ne 0) { throw "Build failed for $target" }
}

# Copy binaries out of the target volume (via a throwaway alpine).
$containerBuildDir = "/target/$RUST_TARGET/$buildProfile"
foreach ($target in $buildTargets) {
    docker run --rm -v "${TARGET_VOLUME}:/target" -v "${outUnixPath}:/out" alpine:latest `
        cp "$containerBuildDir/$target" "/out/$target"
    if ($LASTEXITCODE -ne 0) { throw "Failed to copy $target from target volume" }
}
Write-Host "  Binaries copied`n" -ForegroundColor Green

# Report
Write-Host "+====================================================+" -ForegroundColor Green
Write-Host "|   Linux ARM64 musl Build Complete!                 |" -ForegroundColor Green
Write-Host "+====================================================+`n" -ForegroundColor Green
Get-ChildItem $OUT_DIR -ErrorAction SilentlyContinue | ForEach-Object {
    $sizeMB = [math]::Round($_.Length / 1MB, 2)
    Write-Host ("  {0,-16} {1,8} MB" -f $_.Name, $sizeMB) -ForegroundColor Green
}
Write-Host "`nNext: packaged by build-android-arm64.ps1 / deployed via deploy.ps1 (HTTP)`n" -ForegroundColor Yellow
