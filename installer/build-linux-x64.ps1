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

Write-Host "`n═══════════════════════════════════════════════════" -ForegroundColor Cyan
Write-Host " Linux x64 Build Pipeline" -ForegroundColor Cyan
Write-Host "═══════════════════════════════════════════════════`n" -ForegroundColor Cyan
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

    $stagingDir = Join-Path $DIST_DIR "staging\linux-x64"
    New-Item -ItemType Directory -Force -Path $stagingDir | Out-Null

    $packageName = "zen-garden-$Version-linux-x64"
    $packageDir = Join-Path $stagingDir $packageName
    $tarPath = Join-Path $stagingDir "$packageName.tar.gz"

    # Clean and create package directory
    if (Test-Path $packageDir) { Remove-Item $packageDir -Recurse -Force }
    New-Item -ItemType Directory -Path $packageDir -Force | Out-Null

    # Copy ALL available binaries (not just tier-specific ones)
    # This allows packages to include previously-built Companions even when only core was built
    $binaries = Get-PlatformBinaries -Config $config -Platform "linux"
    $includedCount = 0
    $skippedCount = 0
    foreach ($binary in $binaries) {
        $result = Copy-BinaryToStaging -SourceDir $LINUX_DIR -StagingRoot $packageDir -Binary $binary -Platform "linux"
        if ($result) { $includedCount++ } else { $skippedCount++ }
    }
    Write-Host "  Binaries: $includedCount included, $skippedCount not found" -ForegroundColor $(if ($skippedCount -gt 0) { 'Yellow' } else { 'Green' })

    # Copy external tools (pre-built binaries from external repos)
    $externalTools = @(Get-ExternalTools -Config $config -Platform "linux")
    $toolsIncluded = 0
    $toolsSkipped = 0
    foreach ($tool in $externalTools) {
        $result = Copy-ExternalToolToStaging -StagingRoot $packageDir -Tool $tool -Platform "linux"
        if ($result) { $toolsIncluded++ } else { $toolsSkipped++ }
    }
    if ($externalTools.Count -gt 0) {
        Write-Host "  External tools: $toolsIncluded included, $toolsSkipped not found" -ForegroundColor $(if ($toolsSkipped -gt 0) { 'Yellow' } else { 'DarkCyan' })
    }

    # Copy assets from config
    $assets = Get-PlatformAssets -Config $config -Platform "linux"
    foreach ($asset in $assets) {
        Copy-AssetToStaging -WorkspaceRoot $WORKSPACE_ROOT -StagingRoot $packageDir -Asset $asset
    }

    # Create package manifest
    $components = @{}
    foreach ($binary in $binaries) {
        $sourceFilename = $binary.Source
        $sourcePath = Join-Path $LINUX_DIR $sourceFilename
        if (Test-Path $sourcePath) {
            $hash = (Get-FileHash $sourcePath -Algorithm SHA256).Hash.ToLower()
            $relativePath = ($binary.Destination + $sourceFilename) -replace '\\', '/'
            $components[$binary.Source] = @{
                path = $relativePath
                sha256 = $hash
                size = (Get-Item $sourcePath).Length
                required = $binary.Required
            }
        }
    }

    # Add external tools to manifest
    foreach ($tool in $externalTools) {
        $filename = $tool.Binary
        $toolPath = Join-Path (Join-Path $packageDir $tool.Destination) $filename
        if (Test-Path $toolPath) {
            $hash = (Get-FileHash $toolPath -Algorithm SHA256).Hash.ToLower()
            $relativePath = ($tool.Destination + $filename) -replace '\\', '/'
            $components[$tool.Name] = @{
                path = $relativePath
                sha256 = $hash
                size = (Get-Item $toolPath).Length
                required = $false
                external = $true
            }
        }
    }

    $manifest = @{
        version = $Version
        platform = "linux"
        architecture = "x64"
        created = (Get-Date).ToUniversalTime().ToString("o")
        components = $components
    }
    # Write without BOM (UTF8 with BOM breaks JSON parsing on Linux)
    $jsonContent = $manifest | ConvertTo-Json -Depth 10
    [System.IO.File]::WriteAllText((Join-Path $packageDir "package.json"), $jsonContent, [System.Text.UTF8Encoding]::new($false))
    
    # Create tar.gz in staging area
    try {
        $tarFile = "$packageName.tar.gz"
        & tar -czf $tarFile -C $stagingDir $packageName 2>&1 | Out-Null
        if ($LASTEXITCODE -eq 0 -and (Test-Path $tarFile)) {
            Move-Item $tarFile $tarPath -Force
            $sizeMB = [math]::Round((Get-Item $tarPath).Length / 1MB, 2)
            Write-Host "`n✓ Package: $packageName.tar.gz ($sizeMB MB)" -ForegroundColor Green
            Write-Host "  Staged at: $stagingDir" -ForegroundColor DarkGray
        } else {
            throw "tar failed with exit code $LASTEXITCODE"
        }
    } finally {
        Remove-Item $packageDir -Recurse -Force -ErrorAction SilentlyContinue
        Remove-Item $tarFile -Force -ErrorAction SilentlyContinue
    }
}

Write-Host "`n✓ Linux x64 build complete" -ForegroundColor Green
