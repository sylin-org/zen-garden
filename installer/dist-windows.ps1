<#
.SYNOPSIS
    Build and package Zen Garden Windows distribution

.DESCRIPTION
    Complete Windows build pipeline:
    - Clean Cargo cache (incremental, fingerprints, build outputs)
    - Build Windows binaries natively
    - Create deployment package (zip with binaries, manifests)

.PARAMETER Version
    Version string (e.g., "0.1.202601251234")

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
$WINDOWS_DIR = Join-Path $DIST_DIR "windows"

# Load configuration
$config = Get-DistConfig -ConfigPath (Join-Path $PSScriptRoot "dist.json")

# Set environment variables for build
$env:GARDEN_VERSION = $Version
$env:BUILD_NUMBER = ($Version -split '\.')[-1]
$env:CARGO_BUILD_NUMBER = $env:BUILD_NUMBER

Write-Host "`n═══════════════════════════════════════════════════" -ForegroundColor Cyan
Write-Host " Windows Build Pipeline" -ForegroundColor Cyan
Write-Host "═══════════════════════════════════════════════════`n" -ForegroundColor Cyan
Write-Host "Version: $Version" -ForegroundColor Cyan
Write-Host "Profile: $(if ($DebugBuild) { 'debug' } elseif ($Release) { 'release' } else { 'fast-release' })" -ForegroundColor Cyan
Write-Host ""

# Clean Windows Cargo cache to ensure version update
Write-Host "Cleaning Cargo cache for version update..." -ForegroundColor DarkGray
$targetProfiles = @("fast-release", "release", "debug")
foreach ($profile in $targetProfiles) {
    $profileDir = Join-Path $WORKSPACE_ROOT "target\$profile"
    if (Test-Path $profileDir) {
        # 1. Final binaries
        Get-ChildItem $profileDir -Filter "garden-*" -File -ErrorAction SilentlyContinue | Remove-Item -Force -ErrorAction SilentlyContinue
        
        # 2. Build outputs
        $buildDir = Join-Path $profileDir "build"
        if (Test-Path $buildDir) {
            Get-ChildItem $buildDir -Filter "garden-*" -Directory -ErrorAction SilentlyContinue | Remove-Item -Recurse -Force -ErrorAction SilentlyContinue
        }
        
        # 3. Incremental cache
        $incrementalDir = Join-Path $profileDir "incremental"
        if (Test-Path $incrementalDir) {
            Get-ChildItem $incrementalDir -Filter "garden*" -Directory -ErrorAction SilentlyContinue | Remove-Item -Recurse -Force -ErrorAction SilentlyContinue
        }
        
        # 4. Fingerprints
        $fingerprintDir = Join-Path $profileDir ".fingerprint"
        if (Test-Path $fingerprintDir) {
            Get-ChildItem $fingerprintDir -Filter "garden-*" -Directory -ErrorAction SilentlyContinue | Remove-Item -Recurse -Force -ErrorAction SilentlyContinue
        }
    }
}
Write-Host ""

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
        $updated = $lines | ForEach-Object {
            if ($_ -match '^version\s*=\s*"[\d\.]+"' -and $_ -notmatch 'rust-version') {
                "version = `"$versionMajorMinor.0`""
            } else {
                $_
            }
        }
        Set-Content $file ($updated -join "`n")
    }
}
Write-Host ""

# Build Windows binaries
$buildScript = Join-Path $PSScriptRoot "build-windows.ps1"
$buildArgs = @{}
if ($DebugBuild) { $buildArgs.Add('DebugBuild', $true) }
if ($Release) { $buildArgs.Add('Release', $true) }
if ($Fast -or (-not $DebugBuild -and -not $Release)) { $buildArgs.Add('Fast', $true) }
if ($SkipTests) { $buildArgs.Add('SkipTests', $true) }
if ($Jobs -gt 0) { $buildArgs.Add('Jobs', $Jobs) }

Write-Host "Building Windows binaries..." -ForegroundColor Yellow
& $buildScript @buildArgs
if ($LASTEXITCODE -ne 0) {
    throw "Windows build failed"
}

# Create package
if (-not $SkipPackage) {
    Write-Host "`nCreating deployment package..." -ForegroundColor Yellow
    
    # Use local staging area
    $stagingDir = Join-Path $DIST_DIR "staging\windows"
    New-Item -ItemType Directory -Force -Path $stagingDir | Out-Null
    
    $packageName = "zen-garden-$Version-windows-amd64"
    $packageDir = Join-Path $stagingDir $packageName
    $zipPath = Join-Path $stagingDir "$packageName.zip"
    
    # Clean and create package directory
    if (Test-Path $packageDir) { Remove-Item $packageDir -Recurse -Force }
    New-Item -ItemType Directory -Path $packageDir -Force | Out-Null
    
    # Copy binaries from config
    $binaries = Get-PlatformBinaries -Config $config -Platform "windows"
    foreach ($binary in $binaries) {
        Copy-BinaryToStaging -SourceDir $WINDOWS_DIR -StagingRoot $packageDir -Binary $binary -Platform "windows" | Out-Null
    }
    
    # Copy assets from config
    $assets = Get-PlatformAssets -Config $config -Platform "windows"
    foreach ($asset in $assets) {
        Copy-AssetToStaging -WorkspaceRoot $WORKSPACE_ROOT -StagingRoot $packageDir -Asset $asset
    }
    
    # Create package manifest
    $components = @{}
    foreach ($binary in $binaries) {
        $sourceFilename = $binary.Source + ".exe"
        $sourcePath = Join-Path $WINDOWS_DIR $sourceFilename
        if (Test-Path $sourcePath) {
            $hash = (Get-FileHash $sourcePath -Algorithm SHA256).Hash.ToLower()
            $relativePath = $binary.Destination + $sourceFilename
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
        platform = "windows"
        architecture = "amd64"
        created = (Get-Date).ToUniversalTime().ToString("o")
        components = $components
    }
    $manifest | ConvertTo-Json -Depth 10 | Set-Content (Join-Path $packageDir "package.json") -Encoding UTF8
    
    # Create zip in staging area
    if (Test-Path $zipPath) { Remove-Item $zipPath -Force }
    Compress-Archive -Path $packageDir -DestinationPath $zipPath -Force
    
    $sizeMB = [math]::Round((Get-Item $zipPath).Length / 1MB, 2)
    Write-Host "`n✓ Package: $packageName.zip ($sizeMB MB)" -ForegroundColor Green
    Write-Host "  Staged at: $stagingDir" -ForegroundColor DarkGray
    
    Remove-Item $packageDir -Recurse -Force -ErrorAction SilentlyContinue
}

Write-Host "`n✓ Windows build complete" -ForegroundColor Green
