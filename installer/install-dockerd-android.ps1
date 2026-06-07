<#
.SYNOPSIS
    Install + start static Docker on a rooted LineageOS phone Stone (the "Stage 3a" bring-up).

.DESCRIPTION
    Downloads the static aarch64 Docker bundle and a pinned runc, pushes them to the phone,
    installs to /data/docker, writes /etc/docker/daemon.json, installs a Magisk boot service so
    dockerd (and then the moss container) come up on every reboot, starts dockerd, and verifies.

    runc is pinned (default 1.1.12): newer runc/containerd can hang on this kernel.

    This MUTATES the device (installs and runs a daemon). After it succeeds
    (`docker info` works), run deploy-android.ps1 to deploy the Stone.

.PARAMETER Serial
    ADB serial. Default: 89TY0BAV9.

.PARAMETER DockerVersion
    Static Docker bundle version. Default: 26.1.5 (bundles runc 1.1.12).

.PARAMETER RuncVersion
    runc release to pin. Default: 1.1.12.

.PARAMETER Adb
    Path to adb.exe (auto-detected).

.PARAMETER SkipDownload
    Reuse already-downloaded artifacts in the temp cache.
#>

[CmdletBinding()]
param(
    [string]$Serial = "89TY0BAV9",
    [string]$DockerVersion = "",
    [string]$RuncVersion = "1.1.12",
    [string]$Adb,
    [switch]$SkipDownload
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$ANDROID_DIR = Join-Path $PSScriptRoot "android"
$cache = Join-Path $env:TEMP "zg-docker"
New-Item -ItemType Directory -Force -Path $cache | Out-Null
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

# Auto-detect the latest published static aarch64 Docker version if not pinned. The static
# 'stable' directory prunes old releases, so a hardcoded version eventually 404s.
if (-not $DockerVersion) {
    $idx = Invoke-WebRequest -Uri "https://download.docker.com/linux/static/stable/aarch64/" -UseBasicParsing
    $DockerVersion = [regex]::Matches($idx.Content, 'docker-(\d+\.\d+\.\d+)\.tgz') |
        ForEach-Object { $_.Groups[1].Value } | Sort-Object { [version]$_ } -Unique | Select-Object -Last 1
    if (-not $DockerVersion) { Write-Host "ERROR: could not detect a Docker static version" -ForegroundColor Red; exit 1 }
    Write-Host "Auto-detected latest static Docker: $DockerVersion" -ForegroundColor DarkGray
}
$bundle = Join-Path $cache "docker-$DockerVersion.tgz"
$runc = Join-Path $cache "runc.arm64"

# ── Resolve adb ─────────────────────────────────────────────────────────
if (-not $Adb) {
    $c = Get-ChildItem "C:\Users\*\AppData\Local\Microsoft\WinGet\Packages\Google.PlatformTools_*\platform-tools\adb.exe" -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($c) { $Adb = $c.FullName } elseif (Get-Command adb -ErrorAction SilentlyContinue) { $Adb = (Get-Command adb).Source }
}
if (-not $Adb -or -not (Test-Path $Adb)) { Write-Host "ERROR: adb not found." -ForegroundColor Red; exit 1 }

Write-Host "`n===================================================" -ForegroundColor Cyan
Write-Host " Install Docker on Android Stone (Stage 3a)" -ForegroundColor Cyan
Write-Host "===================================================`n" -ForegroundColor Cyan
Write-Host "  Device:  $Serial"
Write-Host "  Docker:  $DockerVersion (static aarch64)"
Write-Host "  runc:    $RuncVersion (pinned)"
Write-Host ""

$state = (& $Adb -s $Serial get-state 2>&1).Trim()
if ($state -ne "device") { Write-Host "ERROR: device state '$state' (expected 'device')." -ForegroundColor Red; exit 1 }

# ── Download artifacts ──────────────────────────────────────────────────
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
if (-not $SkipDownload -or -not (Test-Path $bundle)) {
    $url = "https://download.docker.com/linux/static/stable/aarch64/docker-$DockerVersion.tgz"
    Write-Host "Downloading $url" -ForegroundColor DarkGray
    Invoke-WebRequest -Uri $url -OutFile $bundle -UseBasicParsing
}
if (-not $SkipDownload -or -not (Test-Path $runc)) {
    $url = "https://github.com/opencontainers/runc/releases/download/v$RuncVersion/runc.arm64"
    Write-Host "Downloading $url" -ForegroundColor DarkGray
    Invoke-WebRequest -Uri $url -OutFile $runc -UseBasicParsing
}
Write-Host "  bundle: $([math]::Round((Get-Item $bundle).Length/1MB,1)) MB; runc: $([math]::Round((Get-Item $runc).Length/1MB,1)) MB" -ForegroundColor Green

# ── Push artifacts + scripts ────────────────────────────────────────────
Write-Host "`nPushing to /data/local/tmp/ ..." -ForegroundColor Cyan
& $Adb -s $Serial push $bundle /data/local/tmp/docker.tgz
& $Adb -s $Serial push $runc /data/local/tmp/runc.arm64
& $Adb -s $Serial push (Join-Path $ANDROID_DIR "install-dockerd.sh") /data/local/tmp/install-dockerd.sh
& $Adb -s $Serial push (Join-Path $ANDROID_DIR "dockerd-service.sh") /data/local/tmp/dockerd-service.sh
& $Adb -s $Serial push (Join-Path $ANDROID_DIR "garden-moss-service.sh") /data/local/tmp/garden-moss-service.sh
& $Adb -s $Serial push (Join-Path $ANDROID_DIR "verify-dockerd.sh") /data/local/tmp/verify-dockerd.sh

# ── Install (root) ──────────────────────────────────────────────────────
Write-Host "`nInstalling Docker (root)..." -ForegroundColor Cyan
& $Adb -s $Serial shell "su -c 'sh /data/local/tmp/install-dockerd.sh /data/local/tmp/docker.tgz /data/local/tmp/runc.arm64'"

# ── Install boot services (dockerd + moss container) ────────────────────
Write-Host "`nInstalling Magisk boot services..." -ForegroundColor Cyan
& $Adb -s $Serial shell "su -c 'mkdir -p /data/adb/service.d && cp /data/local/tmp/dockerd-service.sh /data/adb/service.d/dockerd.sh && cp /data/local/tmp/garden-moss-service.sh /data/adb/service.d/garden-moss.sh && chmod 0755 /data/adb/service.d/dockerd.sh /data/adb/service.d/garden-moss.sh && echo BOOT_SERVICES_OK'"

# ── Start dockerd now ───────────────────────────────────────────────────
Write-Host "`nStarting dockerd..." -ForegroundColor Cyan
& $Adb -s $Serial shell "su -c 'sh /data/adb/service.d/dockerd.sh'"
Start-Sleep -Seconds 6

# ── Verify ──────────────────────────────────────────────────────────────
Write-Host "`nVerifying..." -ForegroundColor Cyan
& $Adb -s $Serial shell "su -c 'sh /data/local/tmp/verify-dockerd.sh'"

Write-Host "`n---------------------------------------------------" -ForegroundColor DarkGray
Write-Host "If 'docker info' reported a Server section, dockerd is up." -ForegroundColor Green
Write-Host "Next: installer/deploy-android.ps1   (deploy the Stone)" -ForegroundColor Yellow
Write-Host ""
