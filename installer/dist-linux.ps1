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

.PARAMETER Description
    Version description

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
    
    [Parameter(Mandatory)]
    [string]$Description,
    
    [switch]$DebugBuild,
    [switch]$Release,
    [switch]$Fast,
    [switch]$ForceRebuild,
    [switch]$SkipPackage,
    [int]$Jobs = 0
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$WORKSPACE_ROOT = (Get-Item $PSScriptRoot).Parent.FullName
$DIST_DIR = Join-Path $WORKSPACE_ROOT "dist"
$LINUX_DIR = Join-Path $DIST_DIR "linux"
$MANIFESTS_DIR = Join-Path $WORKSPACE_ROOT "manifests"

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
    
    # Use temp staging area instead of dist/packages directly
    $stagingDir = Join-Path $env:TEMP "zen-garden-staging-linux"
    New-Item -ItemType Directory -Force -Path $stagingDir | Out-Null
    
    $packageName = "zen-garden-$Version-linux-amd64"
    $packageDir = Join-Path $env:TEMP $packageName
    $tarPath = Join-Path $stagingDir "$packageName.tar.gz"
    
    # Clean and create package directory
    if (Test-Path $packageDir) { Remove-Item $packageDir -Recurse -Force }
    New-Item -ItemType Directory -Path $packageDir -Force | Out-Null
    New-Item -ItemType Directory -Path (Join-Path $packageDir "bin") -Force | Out-Null
    
    # Copy binaries (adapters go in bin/adapters/ subdirectory)
    Get-ChildItem $LINUX_DIR -File | ForEach-Object {
        if ($_.BaseName -like "*cricket*") {
            $adaptersDir = Join-Path (Join-Path $packageDir "bin") "adapters"
            New-Item -ItemType Directory -Path $adaptersDir -Force | Out-Null
            Copy-Item $_.FullName $adaptersDir
            Write-Host "  + bin/adapters/$($_.Name)" -ForegroundColor DarkGray
        } else {
            Copy-Item $_.FullName (Join-Path $packageDir "bin")
            Write-Host "  + bin/$($_.Name)" -ForegroundColor DarkGray
        }
    }
    
    # Copy manifests
    if (Test-Path $MANIFESTS_DIR) {
        Copy-Item $MANIFESTS_DIR (Join-Path $packageDir "manifests") -Recurse
        $manifestCount = (Get-ChildItem (Join-Path $packageDir "manifests") -Recurse -File).Count
        Write-Host "  + $manifestCount manifests" -ForegroundColor DarkGray
    }
    
    # Copy deployment scripts and ensure Unix line endings
    $scriptsPackageDir = Join-Path $packageDir "scripts"
    New-Item -ItemType Directory -Path $scriptsPackageDir -Force | Out-Null
    foreach ($scriptName in @("moss-update-helper.sh", "garden-upgrade.sh")) {
        $scriptPath = Join-Path $PSScriptRoot $scriptName
        if (Test-Path $scriptPath) {
            $destPath = Join-Path $scriptsPackageDir $scriptName
            $content = Get-Content $scriptPath -Raw
            $content = $content -replace "`r`n", "`n"
            [System.IO.File]::WriteAllText($destPath, $content, [System.Text.UTF8Encoding]::new($false))
            Write-Host "  + scripts/$scriptName (LF)" -ForegroundColor DarkGray
        }
    }
    
    # Create package manifest
    $components = @{}
    Get-ChildItem $LINUX_DIR -File | ForEach-Object {
        $hash = (Get-FileHash $_.FullName -Algorithm SHA256).Hash.ToLower()
        $pathPrefix = if ($_.BaseName -like "*cricket*") { "bin/adapters" } else { "bin" }
        $components[$_.BaseName] = @{
            path = "$pathPrefix/$($_.Name)"
            sha256 = $hash
            size = $_.Length
            required = $_.BaseName -in @("garden-moss", "garden-rake")
        }
    }
    
    $manifest = @{
        version = $Version
        platform = "linux"
        architecture = "amd64"
        created = (Get-Date).ToUniversalTime().ToString("o")
        components = $components
        notes = $Description
    }
    $manifest | ConvertTo-Json -Depth 10 | Set-Content (Join-Path $packageDir "package.json") -Encoding UTF8
    
    # Create tar.gz in staging area
    try {
        $tarFile = "$packageName.tar.gz"
        & tar -czf $tarFile -C $env:TEMP $packageName 2>&1 | Out-Null
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
