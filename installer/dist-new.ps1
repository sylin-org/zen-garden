<#
.SYNOPSIS
    Master distribution coordinator for Zen Garden

.DESCRIPTION
    Coordinates Linux and Windows builds, consolidates packages.
    All configuration is read from dist.json.

.PARAMETER SkipLinux
    Skip Linux build

.PARAMETER SkipWindows
    Skip Windows build

.PARAMETER DebugBuild
    Build debug binaries

.PARAMETER Release
    Build full-release binaries (full LTO)

.PARAMETER Fast
    Use fast-release profile (default, thin LTO)

.PARAMETER ForceRebuild
    Force rebuild of Docker container (Linux only)

.PARAMETER Jobs
    Number of parallel cargo jobs

.EXAMPLE
    .\dist.ps1
    Build both platforms with fast-release profile

.EXAMPLE
    .\dist.ps1 -SkipWindows -Release
    Build Linux only with full release profile
#>

[CmdletBinding()]
param(
    [switch]$SkipLinux,
    [switch]$SkipWindows,
    [switch]$DebugBuild,
    [switch]$Release,
    [switch]$Fast,
    [switch]$ForceRebuild,
    [int]$Jobs = 0
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

# Import configuration module
Import-Module (Join-Path $PSScriptRoot "DistConfig.psm1") -Force

# Load configuration
$config = Get-DistConfig

# Generate version
$versionFile = Join-Path $config.workspace.root "version.json"
$versionData = Get-Content $versionFile | ConvertFrom-Json
$buildNumber = Get-Date -Format "yyyyMMddHHmm"
$version = "$($versionData.version).$buildNumber"

Write-Host "`n╔════════════════════════════════════════════════════╗" -ForegroundColor Cyan
Write-Host "║  Zen Garden Distribution Build                     ║" -ForegroundColor Cyan
Write-Host "╚════════════════════════════════════════════════════╝`n" -ForegroundColor Cyan
Write-Host "Version: $version" -ForegroundColor Cyan
Write-Host "Configuration: dist.json" -ForegroundColor Cyan
Write-Host ""

# Determine build profile
$profile = if ($DebugBuild) { "debug" } 
           elseif ($Release) { "release" } 
           else { "fast-release" }

Write-Host "Build Profile: $profile" -ForegroundColor Yellow
Write-Host ""

# Track build results
$buildErrors = @()
$builtPlatforms = @()

# Build Linux
if (-not $SkipLinux) {
    Write-Host "═══════════════════════════════════════════════════" -ForegroundColor Cyan
    Write-Host " Linux Build" -ForegroundColor Cyan
    Write-Host "═══════════════════════════════════════════════════`n" -ForegroundColor Cyan
    
    $linuxScript = Join-Path $PSScriptRoot $config.linux.buildScript
    try {
        & $linuxScript -Version $version -Description "Automated build $buildNumber" `
            -DebugBuild:$DebugBuild -Release:$Release -Fast:($Fast -or (-not $DebugBuild -and -not $Release)) `
            -ForceRebuild:$ForceRebuild -Jobs $Jobs -SkipPackage
        
        if ($LASTEXITCODE -ne 0) {
            throw "Linux build failed with exit code $LASTEXITCODE"
        }
        
        $builtPlatforms += "linux"
        Write-Host "✓ Linux build complete`n" -ForegroundColor Green
    }
    catch {
        $buildErrors += "Linux: $_"
        Write-Host "✗ Linux build failed: $_`n" -ForegroundColor Red
    }
}

# Build Windows
if (-not $SkipWindows) {
    Write-Host "═══════════════════════════════════════════════════" -ForegroundColor Cyan
    Write-Host " Windows Build" -ForegroundColor Cyan
    Write-Host "═══════════════════════════════════════════════════`n" -ForegroundColor Cyan
    
    $windowsScript = Join-Path $PSScriptRoot $config.windows.buildScript
    try {
        & $windowsScript -Version $version -Description "Automated build $buildNumber" `
            -DebugBuild:$DebugBuild -Release:$Release -Fast:($Fast -or (-not $DebugBuild -and -not $Release)) `
            -Jobs $Jobs -SkipPackage
        
        if ($LASTEXITCODE -ne 0) {
            throw "Windows build failed with exit code $LASTEXITCODE"
        }
        
        $builtPlatforms += "windows"
        Write-Host "✓ Windows build complete`n" -ForegroundColor Green
    }
    catch {
        $buildErrors += "Windows: $_"
        Write-Host "✗ Windows build failed: $_`n" -ForegroundColor Red
    }
}

# Check for build errors
if ($buildErrors.Count -gt 0) {
    Write-Host "╔════════════════════════════════════════════════════╗" -ForegroundColor Red
    Write-Host "║  Build Failed                                      ║" -ForegroundColor Red
    Write-Host "╚════════════════════════════════════════════════════╝`n" -ForegroundColor Red
    foreach ($error in $buildErrors) {
        Write-Host "  ✗ $error" -ForegroundColor Red
    }
    exit 1
}

if ($builtPlatforms.Count -eq 0) {
    Write-Host "No platforms selected for build" -ForegroundColor Yellow
    exit 0
}

# Consolidate packages
Write-Host "═══════════════════════════════════════════════════" -ForegroundColor Cyan
Write-Host " Package Consolidation" -ForegroundColor Cyan
Write-Host "═══════════════════════════════════════════════════`n" -ForegroundColor Cyan

# Clean and recreate packages directory
Write-Host "Cleaning packages directory..." -ForegroundColor Yellow
if (Test-Path $config.packages.outputDir) {
    Remove-Item $config.packages.outputDir -Recurse -Force
}
New-Item -ItemType Directory -Path $config.packages.outputDir -Force | Out-Null

$packagesMoved = 0

# Move Linux package
if ($builtPlatforms -contains "linux") {
    $linuxStaging = $config.staging.linux -replace '\$\{TEMP\}', $env:TEMP
    $linuxPackage = Get-ChildItem $linuxStaging -Filter "*.tar.gz" -ErrorAction SilentlyContinue | Select-Object -First 1
    
    if ($linuxPackage) {
        Move-Item $linuxPackage.FullName $config.packages.outputDir -Force
        $sizeMB = [math]::Round($linuxPackage.Length / 1MB, 2)
        Write-Host "  ✓ $($linuxPackage.Name) ($sizeMB MB)" -ForegroundColor Green
        $packagesMoved++
    } else {
        Write-Warning "Linux package not found in staging"
    }
}

# Move Windows package
if ($builtPlatforms -contains "windows") {
    $windowsStaging = $config.staging.windows -replace '\$\{TEMP\}', $env:TEMP
    $windowsPackage = Get-ChildItem $windowsStaging -Filter "*.zip" -ErrorAction SilentlyContinue | Select-Object -First 1
    
    if ($windowsPackage) {
        Move-Item $windowsPackage.FullName $config.packages.outputDir -Force
        $sizeMB = [math]::Round($windowsPackage.Length / 1MB, 2)
        Write-Host "  ✓ $($windowsPackage.Name) ($sizeMB MB)" -ForegroundColor Green
        $packagesMoved++
    } else {
        Write-Warning "Windows package not found in staging"
    }
}

if ($packagesMoved -eq 0) {
    Write-Host "⚠️  No packages found to consolidate" -ForegroundColor Yellow
    exit 1
}

# Summary
Write-Host "`n╔════════════════════════════════════════════════════╗" -ForegroundColor Green
Write-Host "║  Distribution Complete                             ║" -ForegroundColor Green
Write-Host "╚════════════════════════════════════════════════════╝`n" -ForegroundColor Green
Write-Host "Version: $version" -ForegroundColor Green
Write-Host "Packages: $packagesMoved created" -ForegroundColor Green
Write-Host "Location: $($config.packages.outputDir)" -ForegroundColor DarkGray
Write-Host ""
