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

.PARAMETER Description
    Version description

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
    
    [Parameter(Mandatory)]
    [string]$Description,
    
    [switch]$DebugBuild,
    [switch]$Release,
    [switch]$Fast,
    [switch]$SkipTests,
    [switch]$SkipPackage,
    [int]$Jobs = 0
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$WORKSPACE_ROOT = (Get-Item $PSScriptRoot).Parent.FullName
$DIST_DIR = Join-Path $WORKSPACE_ROOT "dist"
$WINDOWS_DIR = Join-Path $DIST_DIR "windows"
$MANIFESTS_DIR = Join-Path $WORKSPACE_ROOT "manifests"

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
    
    # Use temp staging area instead of dist/packages directly
    $stagingDir = Join-Path $env:TEMP "zen-garden-staging-windows"
    New-Item -ItemType Directory -Force -Path $stagingDir | Out-Null
    
    $packageName = "zen-garden-$Version-windows-amd64"
    $packageDir = Join-Path $env:TEMP $packageName
    $zipPath = Join-Path $stagingDir "$packageName.zip"
    
    # Clean and create package directory
    if (Test-Path $packageDir) { Remove-Item $packageDir -Recurse -Force }
    New-Item -ItemType Directory -Path $packageDir -Force | Out-Null
    New-Item -ItemType Directory -Path (Join-Path $packageDir "bin") -Force | Out-Null
    
    # Copy binaries (adapters go in bin/adapters/ subdirectory)
    Get-ChildItem $WINDOWS_DIR -File | ForEach-Object {
        if ($_.BaseName -like "*cricket*") {
            $adaptersDir = Join-Path (Join-Path $packageDir "bin") "adapters"
            New-Item -ItemType Directory -Path $adaptersDir -Force | Out-Null
            Copy-Item $_.FullName $adaptersDir
            Write-Host "  + bin\adapters\$($_.Name)" -ForegroundColor DarkGray
        } else {
            Copy-Item $_.FullName (Join-Path $packageDir "bin")
            Write-Host "  + bin\$($_.Name)" -ForegroundColor DarkGray
        }
    }
    
    # Copy manifests
    if (Test-Path $MANIFESTS_DIR) {
        Copy-Item $MANIFESTS_DIR (Join-Path $packageDir "manifests") -Recurse
        $manifestCount = (Get-ChildItem (Join-Path $packageDir "manifests") -Recurse -File).Count
        Write-Host "  + $manifestCount manifests" -ForegroundColor DarkGray
    }
    
    # Create package manifest
    $components = @{}
    Get-ChildItem $WINDOWS_DIR -File | ForEach-Object {
        $hash = (Get-FileHash $_.FullName -Algorithm SHA256).Hash.ToLower()
        $pathPrefix = if ($_.BaseName -like "*cricket*") { "bin\adapters" } else { "bin" }
        $components[$_.BaseName] = @{
            path = "$pathPrefix\$($_.Name)"
            sha256 = $hash
            size = $_.Length
            required = $_.BaseName -in @("garden-moss", "garden-rake")
        }
    }
    
    $manifest = @{
        version = $Version
        platform = "windows"
        architecture = "amd64"
        created = (Get-Date).ToUniversalTime().ToString("o")
        components = $components
        notes = $Description
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
