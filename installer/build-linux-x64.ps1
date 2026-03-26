<#
.SYNOPSIS
    Build and package Zen Garden Linux distribution

.DESCRIPTION
    Complete Linux build pipeline:
    - Clean Cargo cache (incremental, fingerprints, build outputs)
    - Build Linux binaries via Docker (only tier-specified binaries)
    - Create deployment package (tar.gz with ALL available binaries, manifests, scripts)

    The package always includes all binaries found in dist/linux-x64/, even if only
    core binaries were built. This allows fast iteration on core components while
    including previously-built Companions in the package.

.PARAMETER Version
    Version string (e.g., "0.1.202601251234")

.PARAMETER Tier
    Build tier: "core" (moss + rake only) or "full" (all binaries)
    Default: "full".

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
    [string]$Tier = "full",

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
$LINUX_DIR = Join-Path $DIST_DIR "linux-x64"

# Load configuration
$config = Get-DistConfig -ConfigPath (Join-Path $PSScriptRoot "dist.json")

# Set environment variables for build
$env:GARDEN_VERSION = $Version
$env:BUILD_NUMBER = ($Version -split '\.')[-1]
$env:CARGO_BUILD_NUMBER = $env:BUILD_NUMBER

Write-Host "`n===================================================" -ForegroundColor Cyan
Write-Host " Linux x64 Build Pipeline" -ForegroundColor Cyan
Write-Host "===================================================`n" -ForegroundColor Cyan
Write-Host "Version: $Version" -ForegroundColor Cyan
Write-Host "Tier: $Tier $(if ($Tier -eq 'core') { '(moss + rake only)' } else { '(all binaries)' })" -ForegroundColor Cyan
Write-Host "Profile: $(if ($DebugBuild) { 'debug' } elseif ($Release) { 'release' } else { 'fast-release' })" -ForegroundColor Cyan
Write-Host ""

# Get build targets for this tier
$buildTargets = Get-CargoBuildTargets -Config $config -Tier $Tier
Write-Host "Building: $($buildTargets -join ', ')" -ForegroundColor Yellow

# Build Linux binaries (only tier-specified targets)
$buildScript = Join-Path $PSScriptRoot "compile-linux-x64.ps1"
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
    throw "Linux build failed"
}

# Create package (includes ALL available binaries, not just those built in this tier)
if (-not $SkipPackage) {
    Write-Host "`nCreating deployment package..." -ForegroundColor Yellow
    Write-Host "  (Including all available binaries from dist/linux-x64/)" -ForegroundColor DarkGray

    New-PlatformPackage `
        -Version $Version `
        -Platform "linux" `
        -Architecture "x64" `
        -SourceDir $LINUX_DIR `
        -StagingBaseDir (Join-Path $DIST_DIR "staging") `
        -WorkspaceRoot $WORKSPACE_ROOT `
        -Config $config
}

Write-Host "`nOK Linux x64 build complete" -ForegroundColor Green
