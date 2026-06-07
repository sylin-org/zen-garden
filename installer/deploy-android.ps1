<#
.SYNOPSIS
    Deploy a native Zen Garden Stone (garden-moss) to a rooted Android (LineageOS) phone over ADB.

.DESCRIPTION
    Moss is the HOST management daemon — it runs natively on the device (static-musl ELF), not
    in a container (see STONE-0001 / NewStone-linux-x64.ps1). This script pushes the static-musl
    garden-moss/garden-rake binaries, installs a Magisk boot service (Android has no systemd),
    and starts moss. Moss coordinates the host's dockerd over /var/run/docker.sock and
    orchestrates offerings (MongoDB, etc.) as containers.

    Prerequisite: dockerd already installed + running on the phone (installer/install-dockerd-android.ps1).
    Build the binaries first with installer/compile-linux-arm64-musl.ps1.

.PARAMETER Serial
    ADB serial. Default: 89TY0BAV9.

.PARAMETER SourceDir
    Directory holding the static-musl binaries. Default: dist/linux-arm64-musl.

.PARAMETER Adb
    Path to adb.exe (auto-detected).
#>

[CmdletBinding()]
param(
    [string]$Serial = "89TY0BAV9",
    [string]$SourceDir,
    [string]$Adb
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$WORKSPACE_ROOT = (Get-Item $PSScriptRoot).Parent.FullName
$ANDROID_DIR = Join-Path $PSScriptRoot "android"
if (-not $SourceDir) { $SourceDir = Join-Path $WORKSPACE_ROOT "dist\linux-arm64-musl" }

# ── Resolve adb ─────────────────────────────────────────────────────────
if (-not $Adb) {
    $c = Get-ChildItem "C:\Users\*\AppData\Local\Microsoft\WinGet\Packages\Google.PlatformTools_*\platform-tools\adb.exe" -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($c) { $Adb = $c.FullName } elseif (Get-Command adb -ErrorAction SilentlyContinue) { $Adb = (Get-Command adb).Source }
}
if (-not $Adb -or -not (Test-Path $Adb)) { Write-Host "ERROR: adb not found." -ForegroundColor Red; exit 1 }

$moss = Join-Path $SourceDir "garden-moss"
$rake = Join-Path $SourceDir "garden-rake"

Write-Host "`n===================================================" -ForegroundColor Cyan
Write-Host " Zen Garden Android Stone Deploy (native moss)" -ForegroundColor Cyan
Write-Host "===================================================`n" -ForegroundColor Cyan
Write-Host "  Device: $Serial" -ForegroundColor Cyan
Write-Host "  Binary: $moss" -ForegroundColor DarkGray
Write-Host ""

if (-not (Test-Path $moss)) {
    Write-Host "ERROR: $moss not found." -ForegroundColor Red
    Write-Host "  Build it first: installer/compile-linux-arm64-musl.ps1" -ForegroundColor Yellow
    exit 1
}

# ── Device reachability ─────────────────────────────────────────────────
$state = (& $Adb -s $Serial get-state 2>&1).Trim()
if ($state -ne "device") {
    Write-Host "ERROR: device '$Serial' state is '$state' (expected 'device')." -ForegroundColor Red
    Write-Host "  If 'unauthorized', tap 'Allow USB debugging' on the phone, then retry." -ForegroundColor Yellow
    exit 1
}

# ── Pre-flight: dockerd up (moss coordinates Docker) ────────────────────
Write-Host "Checking Docker daemon on the phone..." -ForegroundColor DarkGray
$dockerCheck = & $Adb -s $Serial shell "su -c 'export PATH=/data/docker/bin:`$PATH DOCKER_HOST=unix:///data/docker/docker.sock; command -v docker >/dev/null 2>&1 && docker info >/dev/null 2>&1 && echo DOCKER_OK || echo DOCKER_MISSING'" 2>&1
if ($dockerCheck -notmatch "DOCKER_OK") {
    Write-Host "BLOCKED: dockerd is not running on the phone." -ForegroundColor Red
    Write-Host "  Run installer/install-dockerd-android.ps1 first (moss needs the Docker socket)." -ForegroundColor Yellow
    exit 2
}
Write-Host "  OK dockerd is up" -ForegroundColor Green

# ── Push binaries + scripts ─────────────────────────────────────────────
Write-Host "`nPushing to /data/local/tmp/ ..." -ForegroundColor Cyan
& $Adb -s $Serial push $moss /data/local/tmp/garden-moss
if (Test-Path $rake) { & $Adb -s $Serial push $rake /data/local/tmp/garden-rake }
& $Adb -s $Serial push (Join-Path $ANDROID_DIR "garden-moss-service.sh") /data/local/tmp/garden-moss-service.sh
& $Adb -s $Serial push (Join-Path $ANDROID_DIR "deploy-moss-native.sh") /data/local/tmp/deploy-moss-native.sh

# ── Run native deploy (root) ────────────────────────────────────────────
Write-Host "`nDeploying native moss (root)..." -ForegroundColor Cyan
& $Adb -s $Serial shell "su -c 'sh /data/local/tmp/deploy-moss-native.sh'"

Write-Host "`n---------------------------------------------------" -ForegroundColor DarkGray
Write-Host "If you saw DEPLOY_OK and a :7185 health response, the Stone is up." -ForegroundColor Green
Write-Host "On-device: su -c '/data/garden-rake discover'  (garden-rake is a native static binary)" -ForegroundColor Yellow
Write-Host "Discovery from another box still needs the phone on the LAN (USB-Ethernet)." -ForegroundColor DarkGray
Write-Host ""
