<#
.SYNOPSIS
    Master distribution coordinator for Zen Garden

.DESCRIPTION
    Coordinates Linux and Windows builds, consolidates packages.
    All configuration is read from dist.json.

    By default, builds only core binaries (moss + rake) for fast iteration.
    Use -Tier full or build-full.ps1 to build all binaries including Companions.

    Packages ALWAYS include all available binaries from dist/, so even when
    building core-only, previously-built Companions are included in the package.

.PARAMETER Tier
    Build tier: "core" (moss + rake only) or "full" (all binaries)
    Default: "core" for fast iteration.

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

.PARAMETER IncludeX86
    Also build Linux x86 (32-bit) binaries for legacy stones

.PARAMETER ForceRebuild
    Force rebuild of Docker container (Linux only)

.PARAMETER Jobs
    Number of parallel cargo jobs

.EXAMPLE
    .\build.ps1
    Build core (moss + rake) for both platforms (default, fast)

.EXAMPLE
    .\build.ps1 -Tier full
    Build all binaries including Companions

.EXAMPLE
    .\build.ps1 -SkipWindows -Release
    Build Linux only with full release profile

.EXAMPLE
    .\build.ps1 -IncludeX86
    Build for all platforms including Linux x86
#>

[CmdletBinding()]
param(
    [ValidateSet('core', 'full')]
    [string]$Tier = "core",

    [switch]$SkipLinux,
    [switch]$SkipWindows,
    [switch]$IncludeX86,
    [switch]$IncludeAndroid,
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
$version = "$($versionData.major).$($versionData.minor).$buildNumber"

Write-Host "`n+====================================================+" -ForegroundColor Cyan
Write-Host "|  Zen Garden Distribution Build                     |" -ForegroundColor Cyan
Write-Host "+====================================================+`n" -ForegroundColor Cyan
Write-Host "Version: $version" -ForegroundColor Cyan
Write-Host "Configuration: dist.json" -ForegroundColor Cyan
Write-Host ""

# Determine build profile
$buildProfile = if ($DebugBuild) { "debug" }
                elseif ($Release) { "release" }
                else { "fast-release" }

Write-Host "Build Tier: $Tier $(if ($Tier -eq 'core') { '(moss + rake only)' } else { '(all binaries)' })" -ForegroundColor Yellow
Write-Host "Build Profile: $buildProfile" -ForegroundColor Yellow
Write-Host ""

# -- Dependency resolution (single source of truth) -------------------
# Resolve dependencies ONCE on the host before any platform build.
# generate-lockfile ensures Cargo.lock matches Cargo.toml + path deps (koi)
# without upgrading pinned versions. fetch pre-downloads all crates.
# Platform builds use --frozen: no network, no lockfile writes.
Write-Host "Resolving dependencies..." -ForegroundColor Yellow
Push-Location $config.workspace.root
try {
    cargo generate-lockfile 2>&1 | Out-Null
    cargo fetch 2>&1 | Out-Null
    Write-Host "  Dependencies resolved. Cargo.lock is authoritative.`n" -ForegroundColor DarkGray
}
catch {
    Write-Host "  Warning: dependency resolution failed: $_`n" -ForegroundColor Yellow
}
finally {
    Pop-Location
}

# Destroy and recreate staging directory (ensures no stale packages)
$stagingRoot = Join-Path $config.workspace.dist "staging"
if (Test-Path $stagingRoot) { Remove-Item $stagingRoot -Recurse -Force }
New-Item -ItemType Directory -Path $stagingRoot -Force | Out-Null

# Track build results
$buildErrors = @()
$builtPlatforms = @()

# TODO: Parallel builds - Linux Docker and Windows native use different target dirs
# and could run simultaneously. Prerequisites:
#   1. Move Cargo.toml version update from build-windows-x64.ps1 into this script
#   2. Build Lantern frontend once here (it's platform-independent)
#   3. Launch Linux as Start-Job, run Windows in foreground, Wait-Job for Linux
# Estimated savings: ~40-50% wall time for full builds.

# Build Linux
if (-not $SkipLinux) {
    Write-Host "===================================================" -ForegroundColor Cyan
    Write-Host " Linux x64 Build" -ForegroundColor Cyan
    Write-Host "===================================================`n" -ForegroundColor Cyan

    $linuxScript = Join-Path $PSScriptRoot $config.'linux-x64'.buildScript
    try {
        & $linuxScript `
            -Version $version `
            -Tier $Tier `
            -DebugBuild:$DebugBuild `
            -Fast:($Fast -or (-not $DebugBuild -and -not $Release)) `
            -ForceRebuild:$ForceRebuild `
            -Jobs $Jobs
        
        if ($LASTEXITCODE -ne 0) {
            throw "Linux build failed with exit code $LASTEXITCODE"
        }
        
        $builtPlatforms += "linux-x64"
        Write-Host "OK Linux x64 build complete`n" -ForegroundColor Green
    }
    catch {
        $buildErrors += "Linux x64: $_"
        Write-Host "X Linux x64 build failed: $_`n" -ForegroundColor Red
    }
}

# Build Windows
if (-not $SkipWindows) {
    Write-Host "===================================================" -ForegroundColor Cyan
    Write-Host " Windows x64 Build" -ForegroundColor Cyan
    Write-Host "===================================================`n" -ForegroundColor Cyan

    $windowsScript = Join-Path $PSScriptRoot $config.'windows-x64'.buildScript
    try {
        & $windowsScript `
            -Version $version `
            -Tier $Tier `
            -DebugBuild:$DebugBuild `
            -Fast:($Fast -or (-not $DebugBuild -and -not $Release)) `
            -SkipTests `
            -Jobs $Jobs
        
        if ($LASTEXITCODE -ne 0) {
            throw "Windows build failed with exit code $LASTEXITCODE"
        }
        
        $builtPlatforms += "windows-x64"
        Write-Host "OK Windows x64 build complete`n" -ForegroundColor Green
    }
    catch {
        $buildErrors += "Windows x64: $_"
        Write-Host "X Windows x64 build failed: $_`n" -ForegroundColor Red
    }
}

# Build Linux x86
if ($IncludeX86) {
    Write-Host "===================================================" -ForegroundColor Magenta
    Write-Host " Linux x86 Build" -ForegroundColor Magenta
    Write-Host "===================================================`n" -ForegroundColor Magenta

    $x86Script = Join-Path $PSScriptRoot $config.'linux-x86'.buildScript
    try {
        & $x86Script `
            -Version $version `
            -Tier $Tier `
            -DebugBuild:$DebugBuild `
            -Fast:($Fast -or (-not $DebugBuild -and -not $Release)) `
            -ForceRebuild:$ForceRebuild `
            -Jobs $Jobs

        if ($LASTEXITCODE -ne 0) {
            throw "Linux x86 build failed with exit code $LASTEXITCODE"
        }

        $builtPlatforms += "linux-x86"
        Write-Host "OK Linux x86 build complete`n" -ForegroundColor Green
    }
    catch {
        $buildErrors += "Linux x86: $_"
        Write-Host "X Linux x86 build failed: $_`n" -ForegroundColor Red
    }
}

# Build Android (aarch64-musl, native phone Stone)
if ($IncludeAndroid) {
    Write-Host "===================================================" -ForegroundColor Magenta
    Write-Host " Android (aarch64-musl) Build" -ForegroundColor Magenta
    Write-Host "===================================================`n" -ForegroundColor Magenta

    $androidScript = Join-Path $PSScriptRoot $config.'android-arm64'.buildScript
    try {
        & $androidScript `
            -Version $version `
            -Tier $Tier `
            -DebugBuild:$DebugBuild `
            -Fast:($Fast -or (-not $DebugBuild -and -not $Release)) `
            -ForceRebuild:$ForceRebuild `
            -Jobs $Jobs

        if ($LASTEXITCODE -ne 0) {
            throw "Android build failed with exit code $LASTEXITCODE"
        }

        $builtPlatforms += "linux-arm64-musl"
        Write-Host "OK Android (aarch64-musl) build complete`n" -ForegroundColor Green
    }
    catch {
        $buildErrors += "Android (aarch64-musl): $_"
        Write-Host "X Android build failed: $_`n" -ForegroundColor Red
    }
}

# Check for build errors
if ($buildErrors.Count -gt 0) {
    Write-Host "+====================================================+" -ForegroundColor Red
    Write-Host "|  Build Failed                                      |" -ForegroundColor Red
    Write-Host "+====================================================+`n" -ForegroundColor Red
    foreach ($buildError in $buildErrors) {
        Write-Host "  X $buildError" -ForegroundColor Red
    }
    exit 1
}

if ($builtPlatforms.Count -eq 0) {
    Write-Host "No platforms selected for build" -ForegroundColor Yellow
    exit 0
}

# Consolidate packages
Write-Host "===================================================" -ForegroundColor Cyan
Write-Host " Package Consolidation" -ForegroundColor Cyan
Write-Host "===================================================`n" -ForegroundColor Cyan

# Clean and recreate packages directory
Write-Host "Cleaning packages directory..." -ForegroundColor Yellow
if (Test-Path $config.packages.outputDir) {
    Remove-Item $config.packages.outputDir -Recurse -Force
}
New-Item -ItemType Directory -Path $config.packages.outputDir -Force | Out-Null

$packagesMoved = 0

# Move Linux x64 package
if ($builtPlatforms -contains "linux-x64") {
    $linuxPackage = Get-ChildItem $config.staging.'linux-x64' -Filter "*.tar.gz" -ErrorAction SilentlyContinue | Select-Object -First 1

    if ($linuxPackage) {
        Move-Item $linuxPackage.FullName $config.packages.outputDir -Force
        $sizeMB = [math]::Round($linuxPackage.Length / 1MB, 2)
        Write-Host "  OK $($linuxPackage.Name) ($sizeMB MB)" -ForegroundColor Green
        $packagesMoved++
    } else {
        Write-Warning "Linux x64 package not found in staging: $($config.staging.'linux-x64')"
    }
}

# Move Windows x64 package
if ($builtPlatforms -contains "windows-x64") {
    $windowsPackage = Get-ChildItem $config.staging.'windows-x64' -Filter "*.zip" -ErrorAction SilentlyContinue | Select-Object -First 1

    if ($windowsPackage) {
        Move-Item $windowsPackage.FullName $config.packages.outputDir -Force
        $sizeMB = [math]::Round($windowsPackage.Length / 1MB, 2)
        Write-Host "  OK $($windowsPackage.Name) ($sizeMB MB)" -ForegroundColor Green
        $packagesMoved++
    } else {
        Write-Warning "Windows x64 package not found in staging: $($config.staging.'windows-x64')"
    }
}

# Move Linux x86 package
if ($builtPlatforms -contains "linux-x86") {
    $x86StagingDir = $config.staging.linuxX86
    if ($x86StagingDir -and (Test-Path $x86StagingDir)) {
        $x86Package = Get-ChildItem $x86StagingDir -Filter "*.tar.gz" -ErrorAction SilentlyContinue | Select-Object -First 1

        if ($x86Package) {
            Move-Item $x86Package.FullName $config.packages.outputDir -Force
            $sizeMB = [math]::Round($x86Package.Length / 1MB, 2)
            Write-Host "  OK $($x86Package.Name) ($sizeMB MB)" -ForegroundColor Green
            $packagesMoved++
        } else {
            Write-Warning "Linux x86 package not found in staging: $x86StagingDir"
        }
    }
}

# Move Android (linux-arm64-musl) package
if ($builtPlatforms -contains "linux-arm64-musl") {
    $armPackage = Get-ChildItem $config.staging.'linux-arm64-musl' -Filter "*.tar.gz" -ErrorAction SilentlyContinue | Select-Object -First 1

    if ($armPackage) {
        Move-Item $armPackage.FullName $config.packages.outputDir -Force
        $sizeMB = [math]::Round($armPackage.Length / 1MB, 2)
        Write-Host "  OK $($armPackage.Name) ($sizeMB MB)" -ForegroundColor Green
        $packagesMoved++
    } else {
        Write-Warning "Android (linux-arm64-musl) package not found in staging: $($config.staging.'linux-arm64-musl')"
    }
}

if ($packagesMoved -eq 0) {
    Write-Host "!  No packages found to consolidate (binaries built but not packaged)" -ForegroundColor Yellow
} else {
    Write-Host "`nPackages moved to: $($config.packages.outputDir)" -ForegroundColor Green
}

# Summary
Write-Host "`n+====================================================+" -ForegroundColor Green
Write-Host "|  Distribution Complete                             |" -ForegroundColor Green
Write-Host "+====================================================+`n" -ForegroundColor Green
Write-Host "Version: $version" -ForegroundColor Green
Write-Host "Tier: $Tier $(if ($Tier -eq 'core') { '(moss + rake built, all binaries packaged)' } else { '(all binaries built and packaged)' })" -ForegroundColor Green
Write-Host "Platforms Built: $($builtPlatforms -join ', ')" -ForegroundColor Green
if ($packagesMoved -gt 0) {
    Write-Host "Packages: $packagesMoved created" -ForegroundColor Green
}
Write-Host "`nBinaries available in:" -ForegroundColor Cyan
if ($builtPlatforms -contains "linux-x64") {
    Write-Host "  Linux x64:   $($config.workspace.dist)/linux-x64" -ForegroundColor Gray
}
if ($builtPlatforms -contains "linux-x86") {
    Write-Host "  Linux x86:   $($config.workspace.dist)/linux-x86" -ForegroundColor Gray
}
if ($builtPlatforms -contains "windows-x64") {
    Write-Host "  Windows x64: $($config.workspace.dist)/windows-x64" -ForegroundColor Gray
}
Write-Host "Location: $($config.packages.outputDir)" -ForegroundColor DarkGray
Write-Host ""
