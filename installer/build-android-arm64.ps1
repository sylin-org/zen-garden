<#
.SYNOPSIS
    Build and package Zen Garden for Android (aarch64-unknown-linux-musl, native phone Stone).

.DESCRIPTION
    Android arm of the standard build pipeline (DEPLOY-0001): brings the phone Stone into the
    same package + HTTP-deploy flow as every other stone. Parallel to build-linux-arm64.ps1 —
    reuses DistConfig.psm1 / New-PlatformPackage verbatim. The only differences from the glibc
    ARM64 build are the compile script (compile-linux-arm64-musl.ps1 → fully-static musl) and the
    architecture tag (arm64-musl). The package keeps Platform="linux" so package.json.platform is
    "linux" — which the deploy handler already accepts on the phone (moss is target_os=linux).

    cricket is excluded on arm64-musl: it links GNU libasound, absent on Android/bionic. See
    docs/notes/cricket-android-audio-research.md for the target-agnostic follow-on.

.PARAMETER Version
    Version string (e.g., "0.2.202601251234").

.PARAMETER Tier
    "core" (moss + rake) or "full" (+ lantern, firefly). Default: core — a phone Stone needs little.

.PARAMETER DebugBuild / Release / Fast / ForceRebuild / SkipPackage / Jobs
    As build-linux-arm64.ps1.
#>

[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [string]$Version,

    [ValidateSet('core', 'full')]
    [string]$Tier = "core",

    [switch]$DebugBuild,
    [switch]$Release,
    [switch]$Fast,
    [switch]$ForceRebuild,
    [switch]$SkipPackage,
    [int]$Jobs = 0
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

Import-Module (Join-Path $PSScriptRoot "DistConfig.psm1") -Force

$WORKSPACE_ROOT = (Get-Item $PSScriptRoot).Parent.FullName
$DIST_DIR = Join-Path $WORKSPACE_ROOT "dist"
$MUSL_DIR = Join-Path $DIST_DIR "linux-arm64-musl"

$config = Get-DistConfig -ConfigPath (Join-Path $PSScriptRoot "dist.json")

$env:GARDEN_VERSION = $Version
$env:BUILD_NUMBER = ($Version -split '\.')[-1]
$env:CARGO_BUILD_NUMBER = $env:BUILD_NUMBER

Write-Host "`n===================================================" -ForegroundColor Cyan
Write-Host " Android (aarch64-musl) Build Pipeline" -ForegroundColor Cyan
Write-Host "===================================================`n" -ForegroundColor Cyan
Write-Host "Version: $Version" -ForegroundColor Cyan
Write-Host "Tier: $Tier" -ForegroundColor Cyan
Write-Host ""

# Targets for this tier, minus cricket (no ALSA on Android/bionic).
$buildTargets = @(Get-CargoBuildTargets -Config $config -Tier $Tier | Where-Object { $_ -ne 'garden-cricket' })
Write-Host "Building: $($buildTargets -join ', ')" -ForegroundColor Yellow

$buildScript = Join-Path $PSScriptRoot "compile-linux-arm64-musl.ps1"
$buildArgs = @{
    Version = $Version
    Targets = $buildTargets
}
if ($DebugBuild) { $buildArgs.Add('DebugBuild', $true) }
if ($Release) { $buildArgs.Add('Release', $true) }
if ($Fast -or (-not $DebugBuild -and -not $Release)) { $buildArgs.Add('Fast', $true) }
if ($ForceRebuild) { $buildArgs.Add('ForceRebuild', $true) }
if ($Jobs -gt 0) { $buildArgs.Add('Jobs', $Jobs) }

& $buildScript @buildArgs
if ($LASTEXITCODE -ne 0) {
    throw "Android (aarch64-musl) build failed"
}

if (-not $SkipPackage) {
    Write-Host "`nCreating deployment package..." -ForegroundColor Yellow
    New-PlatformPackage `
        -Version $Version `
        -Platform "linux" `
        -Architecture "arm64-musl" `
        -SourceDir $MUSL_DIR `
        -StagingBaseDir (Join-Path $DIST_DIR "staging") `
        -WorkspaceRoot $WORKSPACE_ROOT `
        -Config $config
}

Write-Host "`nOK Android (aarch64-musl) build complete" -ForegroundColor Green
