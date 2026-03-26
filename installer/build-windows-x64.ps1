<#
.SYNOPSIS
    Build and package Zen Garden Windows distribution

.DESCRIPTION
    Complete Windows build pipeline:
    - Clean Cargo cache (incremental, fingerprints, build outputs)
    - Build Windows binaries natively (only tier-specified binaries)
    - Create deployment package (zip with ALL available binaries, manifests)

    The package always includes all binaries found in dist/windows-x64/, even if only
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

.PARAMETER SkipTests
    Skip running tests before build

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
    [switch]$SkipTests,
    [switch]$SkipPackage,
    [int]$Jobs = 0
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

# Import config module
Import-Module (Join-Path $PSScriptRoot "DistConfig.psm1") -Force

$WORKSPACE_ROOT = (Get-Item $PSScriptRoot).Parent.FullName
$DIST_DIR = Join-Path $WORKSPACE_ROOT "dist"
$WINDOWS_X64_DIR = Join-Path $DIST_DIR "windows-x64"

# Load configuration
$config = Get-DistConfig -ConfigPath (Join-Path $PSScriptRoot "dist.json")

# Set environment variables for build
$env:GARDEN_VERSION = $Version
$env:BUILD_NUMBER = ($Version -split '\.')[-1]
$env:CARGO_BUILD_NUMBER = $env:BUILD_NUMBER

Write-Host "`n===================================================" -ForegroundColor Cyan
Write-Host " Windows x64 Build Pipeline" -ForegroundColor Cyan
Write-Host "===================================================`n" -ForegroundColor Cyan
Write-Host "Version: $Version" -ForegroundColor Cyan
Write-Host "Tier: $Tier $(if ($Tier -eq 'core') { '(moss + rake only)' } else { '(all binaries)' })" -ForegroundColor Cyan
Write-Host "Profile: $(if ($DebugBuild) { 'debug' } elseif ($Release) { 'release' } else { 'fast-release' })" -ForegroundColor Cyan
Write-Host ""

# Version update detection: build.rs declares cargo:rerun-if-env-changed=CARGO_BUILD_NUMBER
# so Cargo automatically re-runs build scripts and recompiles affected crates when the
# build number changes. No manual cache cleaning needed - incremental compilation works.
# The Cargo.toml version update below also triggers Cargo fingerprint invalidation.

# Update Cargo.toml files with version
Write-Host "Updating Cargo.toml files..." -ForegroundColor DarkGray
$cargoFiles = @(
    (Join-Path $WORKSPACE_ROOT "src\moss\Cargo.toml"),
    (Join-Path $WORKSPACE_ROOT "src\rake\Cargo.toml"),
    (Join-Path $WORKSPACE_ROOT "src\lantern\Cargo.toml"),
    (Join-Path $WORKSPACE_ROOT "src\common\Cargo.toml")
)

$versionMajorMinor = ($Version -split '\.')[0..1] -join '.'
foreach ($file in $cargoFiles) {
    if (Test-Path $file) {
        $lines = Get-Content $file
        $inPackage = $false
        $updated = $lines | ForEach-Object {
            # Track which TOML section we're in
            if ($_ -match '^\[package\]') { $inPackage = $true }
            elseif ($_ -match '^\[') { $inPackage = $false }

            # Only replace the [package] version, not dependency versions
            if ($inPackage -and $_ -match '^version\s*=\s*"[\d\.]+"' -and $_ -notmatch 'rust-version') {
                "version = `"$versionMajorMinor.0`""
            } else {
                $_
            }
        }
        Set-Content $file ($updated -join "`n")
    }
}
Write-Host ""

# Get build targets for this tier
$buildTargets = Get-CargoBuildTargets -Config $config -Tier $Tier
Write-Host "Building: $($buildTargets -join ', ')" -ForegroundColor Yellow

# Build Windows binaries (only tier-specified targets)
$buildScript = Join-Path $PSScriptRoot "compile-windows-x64.ps1"
$buildArgs = @{
    Targets = $buildTargets
}
if ($DebugBuild) { $buildArgs.Add('DebugBuild', $true) }
if ($Release) { $buildArgs.Add('Release', $true) }
if ($Fast -or (-not $DebugBuild -and -not $Release)) { $buildArgs.Add('Fast', $true) }
if ($SkipTests) { $buildArgs.Add('SkipTests', $true) }
if ($Jobs -gt 0) { $buildArgs.Add('Jobs', $Jobs) }

& $buildScript @buildArgs
if ($LASTEXITCODE -ne 0) {
    throw "Windows build failed"
}

# Create package (includes ALL available binaries, not just those built in this tier)
if (-not $SkipPackage) {
    Write-Host "`nCreating deployment package..." -ForegroundColor Yellow
    Write-Host "  (Including all available binaries from dist/windows-x64/)" -ForegroundColor DarkGray

    New-PlatformPackage `
        -Version $Version `
        -Platform "windows" `
        -Architecture "x64" `
        -SourceDir $WINDOWS_X64_DIR `
        -StagingBaseDir (Join-Path $DIST_DIR "staging") `
        -WorkspaceRoot $WORKSPACE_ROOT `
        -Config $config
}

Write-Host "`nOK Windows x64 build complete" -ForegroundColor Green
