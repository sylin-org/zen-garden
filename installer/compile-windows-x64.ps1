<#
.SYNOPSIS
    Compile Zen Garden Windows binaries natively

.DESCRIPTION
    Compiles garden-moss.exe and garden-rake.exe for Windows using the MSVC toolchain.
    Requires Rust with x86_64-pc-windows-msvc target installed.

.PARAMETER Targets
    List of cargo package names to build (e.g., "garden-moss", "garden-rake")
    If not specified, builds all binaries.

.PARAMETER DebugBuild
    Compile debug binaries instead of optimized release (default: release)

.PARAMETER Fast
    Use fast-release profile (~40% faster compile, ~5-10% larger binaries)
    Uses thin LTO and parallel codegen for faster iteration

.PARAMETER SkipTests
    Skip running tests before build

.PARAMETER Jobs
    Number of parallel cargo jobs (default: number of CPUs)

.EXAMPLE
    .\compile-windows-x64.ps1
    # Build all binaries for Windows (default)

.EXAMPLE
    .\compile-windows-x64.ps1 -Targets "garden-moss","garden-rake"
    # Build only moss and rake (core tier)

.EXAMPLE
    .\compile-windows-x64.ps1 -Fast
    # Build with fast-release profile (~40% faster, slightly larger binaries)

.EXAMPLE
    .\compile-windows-x64.ps1 -DebugBuild
    # Compile debug binaries (faster compile, larger size)

.EXAMPLE
    .\compile-windows-x64.ps1 -SkipTests
    # Fast build without tests
#>

[CmdletBinding()]
param(
    [string]$Version,
    [string[]]$Targets,
    [switch]$DebugBuild,
    [switch]$Fast,
    [switch]$SkipTests,
    [int]$Jobs = 0
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

# Detect if running on Windows (works in both Windows PowerShell 5.x and PowerShell Core 6+)
$RunningOnWindows = if ($null -ne (Get-Variable -Name IsWindows -ValueOnly -ErrorAction SilentlyContinue)) {
    $IsWindows
} else {
    $env:OS -eq "Windows_NT"
}

$WORKSPACE_ROOT = (Get-Item $PSScriptRoot).Parent.FullName
$DIST_DIR = Join-Path $WORKSPACE_ROOT "dist"
$WINDOWS_X64_DIR = Join-Path $DIST_DIR "windows-x64"

Write-Host "`n╔════════════════════════════════════════════════════╗" -ForegroundColor Cyan
Write-Host "║   Zen Garden Windows x64 Build                     ║" -ForegroundColor Cyan
Write-Host "╚════════════════════════════════════════════════════╝`n" -ForegroundColor Cyan

if (-not $RunningOnWindows) {
    Write-Host "✗ This script must run on Windows." -ForegroundColor Red
    Write-Host "  For Linux builds, use compile-linux-x64.ps1" -ForegroundColor Yellow
    exit 1
}

# Determine build type (default: release for production)
# Priority: DebugBuild > Fast > Release
$buildProfile = if ($DebugBuild) {
    "debug"
} elseif ($Fast) {
    "fast-release"  # Custom profile in Cargo.toml
} else {
    "release"
}

# Use version from parameter if provided, otherwise generate default
if ($Version) {
    $env:GARDEN_VERSION = $Version
    # Extract build number from version (assumes format: major.minor.buildNumber)
    $parts = $Version.Split('.')
    if ($parts.Length -ge 3) {
        $env:BUILD_NUMBER = $parts[2]
        $env:CARGO_BUILD_NUMBER = $parts[2]
    }
} elseif (-not $env:GARDEN_VERSION) {
    $revision = (Get-Date).ToString("yyyyMMddHHmm")
    $env:GARDEN_VERSION = "0.1.$revision"
    $env:BUILD_NUMBER = $revision
    $env:CARGO_BUILD_NUMBER = $revision
    Write-Host "⚠ Version not set by parent, using default: $env:GARDEN_VERSION" -ForegroundColor Yellow
}
$version = $env:GARDEN_VERSION

# Determine parallel jobs
$parallelJobs = if ($Jobs -gt 0) { $Jobs } else { [Environment]::ProcessorCount }

Write-Host "Configuration:" -ForegroundColor Yellow
Write-Host "  Platform: Windows"
Write-Host "  Version: $version"
$buildTypeDesc = switch ($buildProfile) {
    "debug" { "Debug (fastest compile, largest binary)" }
    "fast-release" { "Fast-Release (thin LTO, ~40% faster compile)" }
    default { "Release (full LTO, smallest binary)" }
}
Write-Host "  Build Type: $buildTypeDesc"
Write-Host "  Parallel Jobs: $parallelJobs"
Write-Host "  Output Dir: $WINDOWS_X64_DIR"
Write-Host ""

# Create dist directories
New-Item -ItemType Directory -Force -Path $WINDOWS_X64_DIR | Out-Null

# Run tests
if (-not $SkipTests) {
    Write-Host "Running tests..." -ForegroundColor Yellow
    Push-Location $WORKSPACE_ROOT
    try {
        cargo test --frozen --workspace --target x86_64-pc-windows-msvc
        if ($LASTEXITCODE -ne 0) {
            throw "Tests failed with exit code $LASTEXITCODE"
        }
        Write-Host "✓ All tests passed`n" -ForegroundColor Green
    } finally {
        Pop-Location
    }
} else {
    Write-Host "⚠ Skipping tests`n" -ForegroundColor DarkYellow
}

# Check if MSVC target installed
Write-Host "Checking Rust toolchain..." -ForegroundColor Yellow
$installedTargets = rustup target list --installed 2>$null
if ($installedTargets -notcontains "x86_64-pc-windows-msvc") {
    Write-Host "  Installing x86_64-pc-windows-msvc target..."
    rustup target add x86_64-pc-windows-msvc
    if ($LASTEXITCODE -ne 0) { throw "Failed to install Windows target" }
}
Write-Host "  ✓ x86_64-pc-windows-msvc target ready`n" -ForegroundColor Green

# Determine which binaries to build
$defaultTargets = @("garden-moss", "garden-rake", "garden-lantern", "garden-cricket", "garden-firefly")
$buildTargets = if ($Targets -and $Targets.Count -gt 0) { $Targets } else { $defaultTargets }

# Build Lantern frontend (if lantern is in the build targets)
if ($buildTargets -contains "garden-lantern") {
    $frontendDir = Join-Path $WORKSPACE_ROOT "src\lantern\frontend"
    if (Test-Path (Join-Path $frontendDir "package.json")) {
        Write-Host "Building Lantern frontend SPA..." -ForegroundColor Yellow

        # Prefer bun, fall back to npm
        $hasBun = Get-Command bun -ErrorAction SilentlyContinue
        $hasNpm = Get-Command npm -ErrorAction SilentlyContinue

        Push-Location $frontendDir
        try {
            if ($hasBun) {
                Write-Host "  Using bun..." -ForegroundColor DarkGray
                bun install --frozen-lockfile 2>$null
                if ($LASTEXITCODE -ne 0) { bun install }
                & .\node_modules\.bin\vite build
            } elseif ($hasNpm) {
                Write-Host "  Using npm..." -ForegroundColor DarkGray
                npm ci 2>$null
                if ($LASTEXITCODE -ne 0) { npm install }
                npx vite build
            } else {
                Write-Host "  ⚠ Neither bun nor npm found — skipping frontend build" -ForegroundColor Yellow
                Write-Host "    Lantern will embed whatever is in frontend/dist/" -ForegroundColor DarkGray
            }

            if ($LASTEXITCODE -eq 0 -and (Test-Path (Join-Path $frontendDir "dist\index.html"))) {
                Write-Host "  ✓ Lantern frontend built`n" -ForegroundColor Green
            } elseif ($LASTEXITCODE -ne 0) {
                Write-Host "  ⚠ Frontend build failed (exit code $LASTEXITCODE) — continuing with cargo build`n" -ForegroundColor Yellow
            }
        } finally {
            Pop-Location
        }
    }
}

# Build Windows binaries
Write-Host "Building Windows binaries..." -ForegroundColor Cyan
foreach ($target in $buildTargets) {
    Write-Host "  → Building $target.exe..."
}

Push-Location $WORKSPACE_ROOT
try {
    # Build common args: profile and parallel jobs
    $commonArgs = @("-j", "$parallelJobs")
    if ($buildProfile -eq "debug") {
        # Debug build - no profile flag needed
    } elseif ($buildProfile -eq "fast-release") {
        $commonArgs += @("--profile", "fast-release")
    } else {
        $commonArgs += "--release"
    }

    # Build all targets
    $buildArgs = @("build", "--frozen") + $commonArgs + @("--target", "x86_64-pc-windows-msvc")
    foreach ($target in $buildTargets) {
        $buildArgs += @("--bin", $target)
    }

    $env:CARGO_TARGET_DIR = Join-Path $WORKSPACE_ROOT "target-windows-x64"
    cargo @buildArgs

    if ($LASTEXITCODE -ne 0) {
        Write-Host "  ⚠ Build failed with exit code $LASTEXITCODE" -ForegroundColor Yellow
    }

    # Copy binaries from target-windows-x64 to dist/windows-x64/
    $srcDir = Join-Path $WORKSPACE_ROOT "target-windows-x64\x86_64-pc-windows-msvc\$buildProfile"

    # Kill any running dist executables before copying (Windows locks running .exe files)
    foreach ($target in $buildTargets) {
        $destPath = "$WINDOWS_X64_DIR\$target.exe"
        if (Test-Path $destPath) {
            $procs = Get-Process -ErrorAction SilentlyContinue | Where-Object {
                try { $_.Path -and [IO.Path]::GetFullPath($_.Path) -eq [IO.Path]::GetFullPath($destPath) } catch { $false }
            }
            foreach ($proc in $procs) {
                Write-Host "  ⚠ Killing $($proc.ProcessName) (PID $($proc.Id)) — file is locked" -ForegroundColor Yellow
                $proc | Stop-Process -Force
                Start-Sleep -Milliseconds 200
            }
        }
    }

    foreach ($target in $buildTargets) {
        $srcPath = "$srcDir\$target.exe"
        if (Test-Path $srcPath) {
            Copy-Item $srcPath "$WINDOWS_X64_DIR\$target.exe" -Force
            Write-Host "  ✓ $target.exe built" -ForegroundColor Green
        } else {
            Write-Host "  ⚠ $target.exe not found (build may have failed)" -ForegroundColor Yellow
        }
    }

    Write-Host "`n✓ Windows binaries built`n" -ForegroundColor Green

} finally {
    Pop-Location
}

# Display results
Write-Host "╔════════════════════════════════════════════════════╗" -ForegroundColor Green
Write-Host "║   Build Complete!                                  ║" -ForegroundColor Green
Write-Host "╚════════════════════════════════════════════════════╝`n" -ForegroundColor Green

Write-Host "Artifacts in $WINDOWS_X64_DIR`:" -ForegroundColor Cyan

$artifacts = Get-ChildItem $WINDOWS_X64_DIR -Filter "*.exe" -ErrorAction SilentlyContinue
if ($artifacts) {
    $artifacts | ForEach-Object {
        $sizeMB = [math]::Round($_.Length / 1MB, 2)
        $sizeStr = if ($sizeMB -lt 1) {
            "$([math]::Round($_.Length / 1KB, 0)) KB"
        } else {
            "$sizeMB MB"
        }
        
        Write-Host ("  ✓ {0,-20} {1,10}" -f $_.Name, $sizeStr) -ForegroundColor Green
    }
} else {
    Write-Host "  (no Windows artifacts found)" -ForegroundColor DarkGray
}

Write-Host "`nNext steps:" -ForegroundColor Yellow
Write-Host "  1. Test garden-rake.exe: .\dist\windows-x64\garden-rake.exe list"
if (Test-Path "$WINDOWS_X64_DIR\garden-moss.exe") {
    Write-Host "  2. Test garden-moss.exe (requires admin): .\dist\windows-x64\garden-moss.exe --help"
}
Write-Host ""
