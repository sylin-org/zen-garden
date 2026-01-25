<#
.SYNOPSIS
    Build complete Zen Garden distribution (orchestrator)

.DESCRIPTION
    Minimal orchestrator that:
    1. Loads version from version.json
    2. Calls dist-linux.ps1 for Linux build
    3. Calls dist-windows.ps1 for Windows build
    4. Summarizes results

    All platform-specific logic lives in dist-linux.ps1 and dist-windows.ps1.

.PARAMETER DebugBuild
    Build debug binaries (fastest compile, largest size)

.PARAMETER Release
    Build full-release binaries (full LTO, smallest size)

.PARAMETER Fast
    Use fast-release profile (default, thin LTO)

.PARAMETER SkipLinux
    Skip Linux build

.PARAMETER SkipWindows
    Skip Windows build

.PARAMETER SkipTests
    Skip tests in Windows build

.PARAMETER SkipPackages
    Skip creating deployment packages

.PARAMETER ForceRebuild
    Force rebuild of Docker container (Linux only)

.PARAMETER Jobs
    Number of parallel cargo jobs

.EXAMPLE
    .\dist.ps1
    # Default: fast-release, all platforms

.EXAMPLE
    .\dist.ps1 -Release
    # Full LTO release (smallest binaries)

.EXAMPLE
    .\dist.ps1 -SkipWindows
    # Linux only
#>

[CmdletBinding()]
param(
    [switch]$DebugBuild,
    [switch]$Release,
    [switch]$Fast,
    [switch]$SkipLinux,
    [switch]$SkipWindows,
    [switch]$SkipTests,
    [switch]$SkipPackages,
    [switch]$ForceRebuild,
    [int]$Jobs = 0
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$WORKSPACE_ROOT = (Get-Item $PSScriptRoot).Parent.FullName
$DIST_DIR = Join-Path $WORKSPACE_ROOT "dist"

# Load version from version.json
$versionFile = Join-Path $WORKSPACE_ROOT "version.json"
if (-not (Test-Path $versionFile)) {
    Write-Error "version.json not found at $versionFile"
    exit 1
}

$versionData = Get-Content $versionFile | ConvertFrom-Json
$major = $versionData.major
$minor = $versionData.minor
$revision = (Get-Date).ToString("yyyyMMddHHmm")
$version = "$major.$minor.$revision"

Write-Host "`n╔════════════════════════════════════════════════════╗" -ForegroundColor Cyan
Write-Host "║   Zen Garden Distribution Build                    ║" -ForegroundColor Cyan
Write-Host "╚════════════════════════════════════════════════════╝`n" -ForegroundColor Cyan

Write-Host "Version: $version" -ForegroundColor Cyan
Write-Host "  Phase: $major.$minor - $($versionData.description)" -ForegroundColor DarkGray
Write-Host "  Moment: $revision ($(Get-Date -Format 'yyyy-MM-dd HH:mm'))" -ForegroundColor DarkGray
Write-Host ""

Write-Host "Platform Selection:" -ForegroundColor Yellow
Write-Host "  Linux Build: $(if ($SkipLinux) { '❌ Skipped' } else { '✓ Enabled' })"
Write-Host "  Windows Build: $(if ($SkipWindows) { '❌ Skipped' } else { '✓ Enabled' })"
Write-Host ""

# Create dist directory
New-Item -ItemType Directory -Force -Path $DIST_DIR | Out-Null

$buildErrors = @()

# Set environment variable for build number
$env:CARGO_BUILD_NUMBER = $revision

# Prepare common arguments
$commonArgs = @{
    Version = $version
    Description = $versionData.description
}
if ($DebugBuild) { $commonArgs.Add('DebugBuild', $true) }
if ($Release) { $commonArgs.Add('Release', $true) }
if ($Fast -or (-not $DebugBuild -and -not $Release)) { $commonArgs.Add('Fast', $true) }
if ($SkipPackages) { $commonArgs.Add('SkipPackage', $true) }
if ($Jobs -gt 0) { $commonArgs.Add('Jobs', $Jobs) }

$linuxScript = Join-Path $PSScriptRoot "dist-linux.ps1"
$windowsScript = Join-Path $PSScriptRoot "dist-windows.ps1"

$linuxArgs = $commonArgs.Clone()
if ($ForceRebuild) { $linuxArgs.Add('ForceRebuild', $true) }

$windowsArgs = $commonArgs.Clone()
if ($SkipTests) { $windowsArgs.Add('SkipTests', $true) }

# Sequential builds
if (-not $SkipLinux) {
    try {
        & $linuxScript @linuxArgs
        if ($LASTEXITCODE -ne 0) {
            $buildErrors += "Linux build failed"
        }
    } catch {
        Write-Host "✗ Linux build failed: $_" -ForegroundColor Red
        $buildErrors += "Linux build failed: $_"
    }
}

if (-not $SkipWindows) {
    try {
        & $windowsScript @windowsArgs
        if ($LASTEXITCODE -ne 0) {
        $buildErrors += "Windows build failed"
    }
} catch {
        Write-Host "✗ Windows build failed: $_" -ForegroundColor Red
        $buildErrors += "Windows build failed: $_"
    }
}

# Consolidate packages after successful builds
if ($buildErrors.Count -eq 0 -and -not $SkipPackages) {
    Write-Host "`nConsolidating packages..." -ForegroundColor Cyan
    
    $packagesDir = Join-Path $DIST_DIR "packages"
    $linuxStagingDir = Join-Path $env:TEMP "zen-garden-staging-linux"
    $windowsStagingDir = Join-Path $env:TEMP "zen-garden-staging-windows"
    
    # Clean dist/packages directory
    if (Test-Path $packagesDir) {
        Write-Host "  Cleaning old packages..." -ForegroundColor DarkGray
        Remove-Item $packagesDir -Recurse -Force
    }
    New-Item -ItemType Directory -Force -Path $packagesDir | Out-Null
    
    # Move staged packages to final location
    $packagesMoved = 0
    if (-not $SkipLinux -and (Test-Path $linuxStagingDir)) {
        Get-ChildItem $linuxStagingDir -File | ForEach-Object {
            Move-Item $_.FullName $packagesDir -Force
            $sizeMB = [math]::Round($_.Length / 1MB, 2)
            Write-Host "  ✓ $($_.Name) ($sizeMB MB)" -ForegroundColor Green
            $packagesMoved++
        }
        Remove-Item $linuxStagingDir -Recurse -Force -ErrorAction SilentlyContinue
    }
    
    if (-not $SkipWindows -and (Test-Path $windowsStagingDir)) {
        Get-ChildItem $windowsStagingDir -File | ForEach-Object {
            Move-Item $_.FullName $packagesDir -Force
            $sizeMB = [math]::Round($_.Length / 1MB, 2)
            Write-Host "  ✓ $($_.Name) ($sizeMB MB)" -ForegroundColor Green
            $packagesMoved++
        }
        Remove-Item $windowsStagingDir -Recurse -Force -ErrorAction SilentlyContinue
    }
    
    if ($packagesMoved -gt 0) {
        Write-Host "`n✓ $packagesMoved package(s) ready in dist/packages" -ForegroundColor Green
    }
}

# Summary
Write-Host "`n╔════════════════════════════════════════════════════╗" -ForegroundColor $(if ($buildErrors.Count -gt 0) { "Yellow" } else { "Green" })
Write-Host "║   Build Summary                                    ║" -ForegroundColor $(if ($buildErrors.Count -gt 0) { "Yellow" } else { "Green" })
Write-Host "╚════════════════════════════════════════════════════╝`n" -ForegroundColor $(if ($buildErrors.Count -gt 0) { "Yellow" } else { "Green" })

if ($buildErrors.Count -gt 0) {
    Write-Host "Build completed with errors:" -ForegroundColor Yellow
    foreach ($buildError in $buildErrors) {
        Write-Host "  ✗ $buildError" -ForegroundColor Red
    }
    Write-Host ""
    exit 1
}

Write-Host "Distribution artifacts:" -ForegroundColor Cyan

$linuxDir = Join-Path $DIST_DIR "linux"
$windowsDir = Join-Path $DIST_DIR "windows"
$packagesDir = Join-Path $DIST_DIR "packages"
$linuxArtifacts = Get-ChildItem $linuxDir -File -ErrorAction SilentlyContinue
$windowsArtifacts = Get-ChildItem $windowsDir -File -ErrorAction SilentlyContinue
$packageArtifacts = Get-ChildItem $packagesDir -File -ErrorAction SilentlyContinue

if ($linuxArtifacts) {
    Write-Host "`n  Linux ($linuxDir):" -ForegroundColor Cyan
    $linuxArtifacts | ForEach-Object {
        $sizeMB = [math]::Round($_.Length / 1MB, 2)
        $sizeStr = if ($sizeMB -lt 1) { "$([math]::Round($_.Length / 1KB, 0)) KB" } else { "$sizeMB MB" }
        Write-Host ("    ✓ {0,-18} {1,10}" -f $_.Name, $sizeStr) -ForegroundColor Green
    }
}

if ($windowsArtifacts) {
    Write-Host "`n  Windows ($windowsDir):" -ForegroundColor Cyan
    $windowsArtifacts | ForEach-Object {
        $sizeMB = [math]::Round($_.Length / 1MB, 2)
        $sizeStr = if ($sizeMB -lt 1) { "$([math]::Round($_.Length / 1KB, 0)) KB" } else { "$sizeMB MB" }
        Write-Host ("    ✓ {0,-18} {1,10}" -f $_.Name, $sizeStr) -ForegroundColor Green
    }
}

if ($packageArtifacts) {
    Write-Host "`n  Packages ($packagesDir):" -ForegroundColor Cyan
    $packageArtifacts | ForEach-Object {
        $sizeMB = [math]::Round($_.Length / 1MB, 2)
        $sizeStr = if ($sizeMB -lt 1) { "$([math]::Round($_.Length / 1KB, 0)) KB" } else { "$sizeMB MB" }
        Write-Host ("    ✓ {0,-35} {1,10}" -f $_.Name, $sizeStr) -ForegroundColor Green
    }
}

Write-Host "`nNext steps:" -ForegroundColor Yellow
if ($packageArtifacts) {
    Write-Host "  cd installer; .\push2all.ps1 -UsePackage" -ForegroundColor Cyan
}
if ($linuxArtifacts) {
    Write-Host "  cd installer; .\NewStone.ps1 -UsbDrive G:" -ForegroundColor Cyan
}
Write-Host ""
