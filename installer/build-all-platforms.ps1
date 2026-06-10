<#
.SYNOPSIS
    Build Zen Garden — interactive platform menu (or -Platform for scripting).

.DESCRIPTION
    Operator-friendly wrapper over build.ps1. Run with no -Platform to get a menu; pass -Platform to
    script it. Covers every fleet platform: Linux x64, Linux x86 (32-bit), Windows x64, and Android
    (aarch64-musl, the native phone Stone).

    Menu:
      [1 / Enter] All platforms   Linux x64 + Linux x86 + Windows x64 + Android
      [2]         64-bit only      Linux x64 + Windows x64
      [3]         Linux x64
      [4]         Linux x86 (32-bit)
      [5]         Windows x64
      [6]         Android (arm64-musl)

.PARAMETER Platform
    What to build, skipping the menu: all | 64bit | linux-x64 | linux-x86 | windows-x64 | android.

.PARAMETER Tier
    Build tier: "core" (moss, rake) or "full" (+ companions). Default: "full".

.PARAMETER DebugBuild
    Debug binaries (fast compile, large).

.PARAMETER Release
    Full-release profile (full LTO).

.PARAMETER Fast
    fast-release profile (thin LTO).

.PARAMETER ForceRebuild
    Force rebuild of build containers / clean target caches.

.PARAMETER Jobs
    Parallel cargo jobs (0 = CPU count).

.PARAMETER DryRun
    Print the resolved build.ps1 invocation and exit without building.

.EXAMPLE
    .\build-all-platforms.ps1
    Show the menu (Enter = all platforms).

.EXAMPLE
    .\build-all-platforms.ps1 -Platform 64bit -Fast
    Build Linux x64 + Windows x64 (fast-release), no prompt.

.EXAMPLE
    .\build-all-platforms.ps1 -Platform android
    Build only the Android (aarch64-musl) package.
#>

[CmdletBinding()]
param(
    [ValidateSet('all', '64bit', 'linux-x64', 'linux-x86', 'windows-x64', 'android')]
    [string]$Platform,

    [ValidateSet('core', 'full')]
    [string]$Tier = "full",

    [switch]$DebugBuild,
    [switch]$Release,
    [switch]$Fast,
    [switch]$ForceRebuild,
    [int]$Jobs = 0,
    [switch]$DryRun
)

# --- Interactive menu (only when -Platform was not supplied) -------------------------------------
while (-not $Platform) {
    Write-Host ""
    Write-Host "==================================================" -ForegroundColor Cyan
    Write-Host "  Zen Garden - Build Platforms" -ForegroundColor Cyan
    Write-Host "==================================================" -ForegroundColor Cyan
    Write-Host ""
    Write-Host "  [1] " -NoNewline -ForegroundColor Green
    Write-Host "All platforms   " -NoNewline
    Write-Host "Linux x64 + Linux x86 + Windows x64 + Android" -ForegroundColor DarkGray
    Write-Host "  [2] " -NoNewline -ForegroundColor Green
    Write-Host "64-bit only     " -NoNewline
    Write-Host "Linux x64 + Windows x64" -ForegroundColor DarkGray
    Write-Host ""
    Write-Host "  Individual platforms:" -ForegroundColor DarkGray
    Write-Host "  [3] Linux x64"
    Write-Host "  [4] Linux x86 (32-bit)"
    Write-Host "  [5] Windows x64"
    Write-Host "  [6] Android (arm64-musl)"
    Write-Host ""
    Write-Host "  [Q] Quit"
    Write-Host ""
    $choice = Read-Host "Select [1]"
    if ([string]::IsNullOrWhiteSpace($choice)) { $choice = "1" }

    switch ($choice.Trim().ToUpper()) {
        "1" { $Platform = "all" }
        "2" { $Platform = "64bit" }
        "3" { $Platform = "linux-x64" }
        "4" { $Platform = "linux-x86" }
        "5" { $Platform = "windows-x64" }
        "6" { $Platform = "android" }
        "Q" { Write-Host "Cancelled." -ForegroundColor Yellow; exit 0 }
        default { Write-Host "  Invalid selection: '$choice'`n" -ForegroundColor Red }
    }
}

# --- Map the selection to build.ps1 flags -------------------------------------------------------
# build.ps1 builds linux-x64 unless -SkipLinux, windows-x64 unless -SkipWindows, x86 only with
# -IncludeX86, and android only with -IncludeAndroid. Every combination is expressible.
$buildArgs = @{ Tier = $Tier; Jobs = $Jobs }
if ($DebugBuild)   { $buildArgs.DebugBuild = $true }
if ($Release)      { $buildArgs.Release = $true }
if ($Fast)         { $buildArgs.Fast = $true }
if ($ForceRebuild) { $buildArgs.ForceRebuild = $true }

switch ($Platform) {
    "all"         { $buildArgs.IncludeX86 = $true; $buildArgs.IncludeAndroid = $true }
    "64bit"       { }  # default: linux-x64 + windows-x64
    "linux-x64"   { $buildArgs.SkipWindows = $true }
    "linux-x86"   { $buildArgs.SkipLinux = $true; $buildArgs.SkipWindows = $true; $buildArgs.IncludeX86 = $true }
    "windows-x64" { $buildArgs.SkipLinux = $true }
    "android"     { $buildArgs.SkipLinux = $true; $buildArgs.SkipWindows = $true; $buildArgs.IncludeAndroid = $true }
}

$platformLabels = @{
    "all"         = "All platforms (Linux x64, Linux x86, Windows x64, Android)"
    "64bit"       = "64-bit (Linux x64 + Windows x64)"
    "linux-x64"   = "Linux x64"
    "linux-x86"   = "Linux x86 (32-bit)"
    "windows-x64" = "Windows x64"
    "android"     = "Android (arm64-musl)"
}

Write-Host "`n-> Building: $($platformLabels[$Platform])`n" -ForegroundColor Cyan

if ($DryRun) {
    $flagStr = ($buildArgs.GetEnumerator() | Sort-Object Name | ForEach-Object {
        if ($_.Value -is [bool] -or $_.Value -is [switch]) { "-$($_.Key)" } else { "-$($_.Key) $($_.Value)" }
    }) -join " "
    Write-Host "DryRun: would invoke build.ps1 $flagStr" -ForegroundColor Yellow
    exit 0
}

$scriptPath = Join-Path $PSScriptRoot "build.ps1"
& $scriptPath @buildArgs
exit $LASTEXITCODE
