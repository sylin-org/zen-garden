<#
.SYNOPSIS
    Build and package Zen Garden Linux ARM64 (aarch64) distribution

.DESCRIPTION
    Complete Linux ARM64 build pipeline:
    - Build Linux ARM64 binaries via Docker cross-compilation (tier-specified binaries)
    - Create deployment package (tar.gz with ALL available binaries, manifests, scripts)

    Parallel pipeline to build-linux-x64.ps1 — reuses DistConfig.psm1 / New-PlatformPackage
    verbatim (architecture is a free string; the package is named ...-linux-arm64.tar.gz).
    The resulting package is consumable by the standard deploy path (deploy-single-stone.ps1)
    once a Stone is running, and by deploy-android.ps1 for first-boot ADB bootstrap.

    The package always includes all binaries found in dist/linux-arm64/, even if only
    core binaries were built.

.PARAMETER Version
    Version string (e.g., "0.1.202601251234")

.PARAMETER Tier
    Build tier: "core" (moss + rake only) or "full" (all binaries)
    Default: "core" — a phone Stone only needs moss + rake.

.PARAMETER DebugBuild
    Build debug binaries

.PARAMETER Release
    Build full-release binaries (full LTO)

.PARAMETER Fast
    Use fast-release profile (default, thin LTO)

.PARAMETER ForceRebuild
    Force rebuild of Docker container

.PARAMETER SkipPackage
    Skip creating deployment package

.PARAMETER Jobs
    Number of parallel cargo jobs
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

# Import config module
Import-Module (Join-Path $PSScriptRoot "DistConfig.psm1") -Force

$WORKSPACE_ROOT = (Get-Item $PSScriptRoot).Parent.FullName
$DIST_DIR = Join-Path $WORKSPACE_ROOT "dist"
$LINUX_DIR = Join-Path $DIST_DIR "linux-arm64"

# Load configuration
$config = Get-DistConfig -ConfigPath (Join-Path $PSScriptRoot "dist.json")

# Set environment variables for build
$env:GARDEN_VERSION = $Version
$env:BUILD_NUMBER = ($Version -split '\.')[-1]
$env:CARGO_BUILD_NUMBER = $env:BUILD_NUMBER

Write-Host "`n===================================================" -ForegroundColor Cyan
Write-Host " Linux ARM64 Build Pipeline" -ForegroundColor Cyan
Write-Host "===================================================`n" -ForegroundColor Cyan
Write-Host "Version: $Version" -ForegroundColor Cyan
Write-Host "Tier: $Tier $(if ($Tier -eq 'core') { '(moss + rake only)' } else { '(all binaries)' })" -ForegroundColor Cyan
Write-Host "Profile: $(if ($DebugBuild) { 'debug' } elseif ($Release) { 'release' } else { 'fast-release' })" -ForegroundColor Cyan
Write-Host ""

# Get build targets for this tier
$buildTargets = Get-CargoBuildTargets -Config $config -Tier $Tier
Write-Host "Building: $($buildTargets -join ', ')" -ForegroundColor Yellow

# Build Linux ARM64 binaries (only tier-specified targets)
$buildScript = Join-Path $PSScriptRoot "compile-linux-arm64.ps1"
$buildArgs = @{
    Targets = $buildTargets
}
if ($DebugBuild) { $buildArgs.Add('DebugBuild', $true) }
if ($Release) { $buildArgs.Add('Release', $true) }
if ($Fast -or (-not $DebugBuild -and -not $Release)) { $buildArgs.Add('Fast', $true) }
if ($ForceRebuild) { $buildArgs.Add('ForceRebuild', $true) }
if ($Jobs -gt 0) { $buildArgs.Add('Jobs', $Jobs) }

& $buildScript @buildArgs
if ($LASTEXITCODE -ne 0) {
    throw "Linux ARM64 build failed"
}

# Create package (includes ALL available binaries, not just those built in this tier)
if (-not $SkipPackage) {
    Write-Host "`nCreating deployment package..." -ForegroundColor Yellow
    Write-Host "  (Including all available binaries from dist/linux-arm64/)" -ForegroundColor DarkGray

    New-PlatformPackage `
        -Version $Version `
        -Platform "linux" `
        -Architecture "arm64" `
        -SourceDir $LINUX_DIR `
        -StagingBaseDir (Join-Path $DIST_DIR "staging") `
        -WorkspaceRoot $WORKSPACE_ROOT `
        -Config $config
}

Write-Host "`nOK Linux ARM64 build complete" -ForegroundColor Green
