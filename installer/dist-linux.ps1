<#
.SYNOPSIS
    Build and package Zen Garden Linux distribution

.DESCRIPTION
    Complete Linux build pipeline:
    - Clean Cargo cache (incremental, fingerprints, build outputs)
    - Build Linux binaries via Docker
    - Create deployment package (tar.gz with binaries, manifests, scripts)

.PARAMETER Version
    Version string (e.g., "0.1.202601251234")

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
$LINUX_DIR = Join-Path $DIST_DIR "linux"

# Load configuration
$config = Get-DistConfig -ConfigPath (Join-Path $PSScriptRoot "dist.json")

# Set environment variables for build
$env:GARDEN_VERSION = $Version
$env:BUILD_NUMBER = ($Version -split '\.')[-1]
$env:CARGO_BUILD_NUMBER = $env:BUILD_NUMBER

Write-Host "`n═══════════════════════════════════════════════════" -ForegroundColor Cyan
Write-Host " Linux Build Pipeline" -ForegroundColor Cyan
Write-Host "═══════════════════════════════════════════════════`n" -ForegroundColor Cyan
Write-Host "Version: $Version" -ForegroundColor Cyan
Write-Host "Profile: $(if ($DebugBuild) { 'debug' } elseif ($Release) { 'release' } else { 'fast-release' })" -ForegroundColor Cyan
Write-Host ""

# Build Linux binaries
$buildScript = Join-Path $PSScriptRoot "build-linux.ps1"
$buildArgs = @{}
if ($DebugBuild) { $buildArgs.Add('DebugBuild', $true) }
if ($Release) { $buildArgs.Add('Release', $true) }
if ($Fast -or (-not $DebugBuild -and -not $Release)) { $buildArgs.Add('Fast', $true) }
if ($ForceRebuild) { $buildArgs.Add('ForceRebuild', $true) }
if ($Jobs -gt 0) { $buildArgs.Add('Jobs', $Jobs) }

Write-Host "Building Linux binaries..." -ForegroundColor Yellow
& $buildScript @buildArgs
if ($LASTEXITCODE -ne 0) {
    throw "Linux build failed"
}

# Create package
if (-not $SkipPackage) {
    Write-Host "`nCreating deployment package..." -ForegroundColor Yellow
    
    # Use local staging area
    $stagingDir = Join-Path $DIST_DIR "staging\linux"
    New-Item -ItemType Directory -Force -Path $stagingDir | Out-Null
    
    $packageName = "zen-garden-$Version-linux-amd64"
    $packageDir = Join-Path $stagingDir $packageName
    $tarPath = Join-Path $stagingDir "$packageName.tar.gz"
    
    # Clean and create package directory
    if (Test-Path $packageDir) { Remove-Item $packageDir -Recurse -Force }
    New-Item -ItemType Directory -Path $packageDir -Force | Out-Null
    
    # Copy binaries from config
    $binaries = Get-PlatformBinaries -Config $config -Platform "linux"
    foreach ($binary in $binaries) {
        Copy-BinaryToStaging -SourceDir $LINUX_DIR -StagingRoot $packageDir -Binary $binary -Platform "linux" | Out-Null
    }
    
    # Copy assets from config
    $assets = Get-PlatformAssets -Config $config -Platform "linux"
    foreach ($asset in $assets) {
        Copy-AssetToStaging -WorkspaceRoot $WORKSPACE_ROOT -StagingRoot $packageDir -Asset $asset
    }
    
    # Write dependencies file for adapters
    Write-DependenciesFile -Config $config -StagingRoot $packageDir -Platform "linux" | Out-Null

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
    
    $manifest = @{
        version = $Version
        platform = "linux"
        architecture = "amd64"
        created = (Get-Date).ToUniversalTime().ToString("o")
        components = $components
    }
    $manifest | ConvertTo-Json -Depth 10 | Set-Content (Join-Path $packageDir "package.json") -Encoding UTF8
    
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

Write-Host "`n✓ Linux build complete" -ForegroundColor Green
