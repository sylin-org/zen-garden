<#
.SYNOPSIS
    Build complete Zen Garden distribution (Linux + Windows)

.DESCRIPTION
    Orchestrates builds for all platforms:
    - Linux (garden-moss, garden-rake) via build-linux.ps1
    - Windows (garden-moss.exe, garden-rake.exe) via build-windows.ps1

    This is the main entry point for full distribution builds.
    Default: fast-release profile (thin LTO, parallel codegen) - best for iteration.

.PARAMETER DebugBuild
    Build debug binaries (fastest compile, largest size, no optimization)

.PARAMETER Release
    Build full-release binaries (full LTO, codegen-units=1)
    Slower build but smallest binaries. Use for final production builds.

.PARAMETER SkipTests
    Skip running tests before build (default: tests are skipped)

.PARAMETER RunTests
    Run tests before build (overrides default skip)

.PARAMETER SkipLinux
    Skip Linux build (build Windows only)

.PARAMETER SkipWindows
    Skip Windows build (build Linux only)

.PARAMETER ForceRebuild
    Force rebuild of Docker build container (Linux only)

.PARAMETER Jobs
    Number of parallel cargo jobs (default: number of CPUs)

.PARAMETER SkipPackages
    Skip creating deployment packages after build

.EXAMPLE
    .\dist.ps1
    # Default: fast-release, skip tests, all platforms

.EXAMPLE
    .\dist.ps1 -Release
    # Full LTO release (smallest binaries, slower build)

.EXAMPLE
    .\dist.ps1 -RunTests
    # Fast-release with tests

.EXAMPLE
    .\dist.ps1 -DebugBuild
    # Debug binaries (fastest compile, largest size)

.EXAMPLE
    .\dist.ps1 -SkipWindows
    # Build Linux binaries only

.EXAMPLE
    .\dist.ps1 -Parallel
    # Build Linux (Docker) and Windows (native) simultaneously
#>

[CmdletBinding()]
param(
    [switch]$DebugBuild,
    [switch]$Release,
    [switch]$SkipTests,
    [switch]$RunTests,
    [switch]$SkipLinux,
    [switch]$SkipWindows,
    [switch]$ForceRebuild,
    [switch]$SkipPackages,
    [switch]$Parallel,
    [int]$Jobs = 0
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

# Detect if running on Windows (works on both PowerShell 5.1 and 7+)
$script:RunningOnWindows = if ($null -ne (Get-Variable -Name IsWindows -ErrorAction SilentlyContinue)) {
    $IsWindows
} else {
    $env:OS -eq "Windows_NT"
}

$WORKSPACE_ROOT = (Get-Item $PSScriptRoot).Parent.FullName
$DIST_DIR = Join-Path $WORKSPACE_ROOT "dist"
$INSTALLER_DIR = $PSScriptRoot

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
Write-Host "║   Zen Garden Distribution Build                   ║" -ForegroundColor Cyan
Write-Host "╚════════════════════════════════════════════════════╝`n" -ForegroundColor Cyan

Write-Host "Version: $version" -ForegroundColor Cyan
Write-Host "  Phase: $major.$minor - $($versionData.description)" -ForegroundColor DarkGray
Write-Host "  Moment: $revision ($(Get-Date -Format 'yyyy-MM-dd HH:mm'))" -ForegroundColor DarkGray
Write-Host ""

Write-Host "Platform Selection:" -ForegroundColor Yellow
Write-Host "  Linux Build: $(if ($SkipLinux) { '❌ Skipped' } else { '✓ Enabled' })"
Write-Host "  Windows Build: $(if ($SkipWindows) { '❌ Skipped' } else { '✓ Enabled' })"
Write-Host "  Parallel Mode: $(if ($Parallel -and -not $SkipLinux -and -not $SkipWindows -and $script:RunningOnWindows) { '✓ Enabled' } else { '○ Sequential' })"
Write-Host ""

# Set version for build scripts
$env:GARDEN_VERSION = $version
$env:BUILD_NUMBER = $revision
$env:CARGO_BUILD_NUMBER = $revision  # For Rust build.rs

# Force rebuild of binaries to pick up new BUILD_NUMBER
# Cargo's incremental cache doesn't detect CARGO_BUILD_NUMBER changes,
# so we need to clean the final artifacts to force recompilation with new version
Write-Host "Cleaning build artifacts to ensure version update..." -ForegroundColor DarkGray
$targetDirs = @(
    (Join-Path $WORKSPACE_ROOT "target\fast-release"),
    (Join-Path $WORKSPACE_ROOT "target\release"),
    (Join-Path $WORKSPACE_ROOT "target\debug")
)
foreach ($targetDir in $targetDirs) {
    if (Test-Path $targetDir) {
        Get-ChildItem $targetDir -Filter "garden-*" -File -ErrorAction SilentlyContinue | Remove-Item -Force -ErrorAction SilentlyContinue
    }
}

# Update Cargo.toml files with version
Write-Host "Updating Cargo.toml files with version $major.$minor..." -ForegroundColor DarkGray
$cargoFiles = @(
    (Join-Path $WORKSPACE_ROOT "src\moss\Cargo.toml"),
    (Join-Path $WORKSPACE_ROOT "src\rake\Cargo.toml"),
    (Join-Path $WORKSPACE_ROOT "src\lantern\Cargo.toml"),
    (Join-Path $WORKSPACE_ROOT "src\common\Cargo.toml")
)

foreach ($file in $cargoFiles) {
    if (Test-Path $file) {
        $lines = Get-Content $file
        $updated = $lines | ForEach-Object {
            if ($_ -match '^version\s*=\s*"[\d\.]+"' -and $_ -notmatch 'rust-version') {
                "version = `"$major.$minor.0`""
            } else {
                $_
            }
        }
        Set-Content $file ($updated -join "`n")
    }
}
Write-Host ""

# Create dist directory
New-Item -ItemType Directory -Force -Path $DIST_DIR | Out-Null

$buildErrors = @()

# Prepare build arguments
$linuxArgs = @{}
if ($DebugBuild) { $linuxArgs.Add('DebugBuild', $true) }
if (-not $Release -and -not $DebugBuild) { $linuxArgs.Add('Fast', $true) }
if ($ForceRebuild) { $linuxArgs.Add('ForceRebuild', $true) }
if ($Jobs -gt 0) { $linuxArgs.Add('Jobs', $Jobs) }

$windowsArgs = @{}
if ($DebugBuild) { $windowsArgs['DebugBuild'] = $true }
if (-not $Release -and -not $DebugBuild) { $windowsArgs['Fast'] = $true }
if (-not $RunTests) { $windowsArgs['SkipTests'] = $true }
if ($Jobs -gt 0) { $windowsArgs['Jobs'] = $Jobs }

$linuxScript = Join-Path $INSTALLER_DIR "build-linux.ps1"
$windowsScript = Join-Path $INSTALLER_DIR "build-windows.ps1"

# Determine if we should run in parallel
$runParallel = $Parallel -and -not $SkipLinux -and -not $SkipWindows -and $script:RunningOnWindows

if ($runParallel) {
    # ═══════════════════════════════════════════════════════════════════
    # PARALLEL BUILD MODE
    # ═══════════════════════════════════════════════════════════════════
    Write-Host "═══════════════════════════════════════════════════" -ForegroundColor Cyan
    Write-Host " Parallel Build: Linux (Docker) + Windows (Native)" -ForegroundColor Cyan
    Write-Host "═══════════════════════════════════════════════════`n" -ForegroundColor Cyan

    # Create log files for output capture
    $linuxLogFile = Join-Path $DIST_DIR "build-linux.log"
    $windowsLogFile = Join-Path $DIST_DIR "build-windows.log"

    Write-Host "Starting builds in parallel..." -ForegroundColor Yellow
    Write-Host "  Linux log:   $linuxLogFile" -ForegroundColor DarkGray
    Write-Host "  Windows log: $windowsLogFile" -ForegroundColor DarkGray
    Write-Host ""

    $startTime = Get-Date

    # Start Linux build as background job
    $linuxJob = Start-Job -ScriptBlock {
        param($script, $buildArgs, $envVars)
        # Restore environment variables in job
        $env:GARDEN_VERSION = $envVars.GARDEN_VERSION
        $env:BUILD_NUMBER = $envVars.BUILD_NUMBER
        $env:CARGO_BUILD_NUMBER = $envVars.CARGO_BUILD_NUMBER

        Set-Location (Split-Path $script -Parent)
        & $script @buildArgs 2>&1
    } -ArgumentList $linuxScript, $linuxArgs, @{
        GARDEN_VERSION = $env:GARDEN_VERSION
        BUILD_NUMBER = $env:BUILD_NUMBER
        CARGO_BUILD_NUMBER = $env:CARGO_BUILD_NUMBER
    }

    # Start Windows build as background job
    $windowsJob = Start-Job -ScriptBlock {
        param($script, $buildArgs, $envVars)
        # Restore environment variables in job
        $env:GARDEN_VERSION = $envVars.GARDEN_VERSION
        $env:BUILD_NUMBER = $envVars.BUILD_NUMBER
        $env:CARGO_BUILD_NUMBER = $envVars.CARGO_BUILD_NUMBER

        Set-Location (Split-Path $script -Parent)
        & $script @buildArgs 2>&1
    } -ArgumentList $windowsScript, $windowsArgs, @{
        GARDEN_VERSION = $env:GARDEN_VERSION
        BUILD_NUMBER = $env:BUILD_NUMBER
        CARGO_BUILD_NUMBER = $env:CARGO_BUILD_NUMBER
    }

    # Monitor progress
    $linuxDone = $false
    $windowsDone = $false

    while (-not $linuxDone -or -not $windowsDone) {
        Start-Sleep -Seconds 2

        if (-not $linuxDone -and $linuxJob.State -ne 'Running') {
            $linuxDone = $true
            $elapsed = [math]::Round(((Get-Date) - $startTime).TotalSeconds)
            if ($linuxJob.State -eq 'Completed') {
                Write-Host "  ✓ Linux build completed (${elapsed}s)" -ForegroundColor Green
            } else {
                Write-Host "  ✗ Linux build failed (${elapsed}s)" -ForegroundColor Red
            }
        }

        if (-not $windowsDone -and $windowsJob.State -ne 'Running') {
            $windowsDone = $true
            $elapsed = [math]::Round(((Get-Date) - $startTime).TotalSeconds)
            if ($windowsJob.State -eq 'Completed') {
                Write-Host "  ✓ Windows build completed (${elapsed}s)" -ForegroundColor Green
            } else {
                Write-Host "  ✗ Windows build failed (${elapsed}s)" -ForegroundColor Red
            }
        }
    }

    # Collect results
    $linuxOutput = Receive-Job -Job $linuxJob
    $windowsOutput = Receive-Job -Job $windowsJob

    # Save logs
    $linuxOutput | Out-File -FilePath $linuxLogFile -Encoding UTF8
    $windowsOutput | Out-File -FilePath $windowsLogFile -Encoding UTF8

    # Check for errors in output (since exit codes aren't reliable in jobs)
    $linuxFailed = $linuxJob.State -eq 'Failed' -or ($linuxOutput -match 'error\[E\d+\]|FAILED|Build failed')
    $windowsFailed = $windowsJob.State -eq 'Failed' -or ($windowsOutput -match 'error\[E\d+\]|FAILED|Build failed')

    if ($linuxFailed) {
        $buildErrors += "Linux build failed (see $linuxLogFile)"
    }
    if ($windowsFailed) {
        $buildErrors += "Windows build failed (see $windowsLogFile)"
    }

    # Cleanup jobs
    Remove-Job -Job $linuxJob, $windowsJob -Force

    $totalTime = [math]::Round(((Get-Date) - $startTime).TotalSeconds)
    Write-Host "`nParallel build completed in ${totalTime}s`n" -ForegroundColor Cyan

} else {
    # ═══════════════════════════════════════════════════════════════════
    # SEQUENTIAL BUILD MODE
    # ═══════════════════════════════════════════════════════════════════

    # Build Linux binaries (via Docker)
    if (-not $SkipLinux) {
        Write-Host "═══════════════════════════════════════════════════" -ForegroundColor Cyan
        Write-Host " Phase 1: Linux Build (Docker)" -ForegroundColor Cyan
        Write-Host "═══════════════════════════════════════════════════`n" -ForegroundColor Cyan

        try {
            & $linuxScript @linuxArgs
            if ($LASTEXITCODE -eq 0 -or $null -eq $LASTEXITCODE) {
                Write-Host "✓ Linux build completed`n" -ForegroundColor Green
            } else {
                $buildErrors += "Linux build failed with exit code $LASTEXITCODE"
                Write-Host "✗ Linux build failed with exit code $LASTEXITCODE`n" -ForegroundColor Red
            }
        } catch {
            $buildErrors += "Linux build: $_"
            Write-Host "✗ Linux build failed: $_`n" -ForegroundColor Red
        }
    } else {
        Write-Host "Skipping Linux build (use -SkipLinux=`$false to enable)`n" -ForegroundColor DarkGray
    }

    # Build Windows binaries (native)
    if (-not $SkipWindows) {
        if (-not $script:RunningOnWindows) {
            Write-Host "Skipping Windows build (not on Windows host)`n" -ForegroundColor DarkGray
        } else {
            Write-Host "═══════════════════════════════════════════════════" -ForegroundColor Cyan
            Write-Host " Phase 2: Windows Build (Native)" -ForegroundColor Cyan
            Write-Host "═══════════════════════════════════════════════════`n" -ForegroundColor Cyan

            & $windowsScript @windowsArgs

            if ($LASTEXITCODE -eq 0 -or $null -eq $LASTEXITCODE) {
                Write-Host "✓ Windows build completed`n" -ForegroundColor Green
            } else {
                $buildErrors += "Windows build failed with exit code $LASTEXITCODE"
                Write-Host "✗ Windows build failed with exit code $LASTEXITCODE`n" -ForegroundColor Red
            }
        }
    } else {
        Write-Host "Skipping Windows build (use -SkipWindows=`$false to enable)`n" -ForegroundColor DarkGray
    }
}

# Create deployment packages
if (-not $SkipPackages -and $buildErrors.Count -eq 0) {
    Write-Host "═══════════════════════════════════════════════════" -ForegroundColor Cyan
    Write-Host " Phase 3: Create Deployment Packages" -ForegroundColor Cyan
    Write-Host "═══════════════════════════════════════════════════`n" -ForegroundColor Cyan

    $packagesDir = Join-Path $DIST_DIR "packages"
    New-Item -ItemType Directory -Force -Path $packagesDir | Out-Null

    # Clean up old packages (keep only latest)
    Get-ChildItem $packagesDir -File -ErrorAction SilentlyContinue | Remove-Item -Force

    $manifestsDir = Join-Path $WORKSPACE_ROOT "manifests"
    $linuxDir = Join-Path $DIST_DIR "linux"
    $windowsDir = Join-Path $DIST_DIR "windows"

    # Helper function to create package manifest
    function ConvertTo-UnixLineEndings {
        <#
        .SYNOPSIS
            Convert CRLF line endings to LF for Unix shell scripts
        .DESCRIPTION
            Ensures shell scripts work correctly on Linux by converting Windows
            CRLF line endings to Unix LF. This prevents "required file not found"
            errors when the shebang line has CR characters.
        #>
        param(
            [Parameter(Mandatory)]
            [string]$Path
        )

        if (Test-Path $Path) {
            $content = Get-Content $Path -Raw
            if ($content.Contains("`r`n")) {
                $content = $content -replace "`r`n", "`n"
                [System.IO.File]::WriteAllText($Path, $content, [System.Text.UTF8Encoding]::new($false))
                Write-Host "    → Fixed line endings: $(Split-Path -Leaf $Path)" -ForegroundColor DarkGray
            }
        }
    }

    function New-PackageManifest {
        param(
            [string]$Platform,
            [string]$BinDir,
            [string]$ManifestsDir,
            [string]$ScriptsDir,
            [string]$Version,
            [string]$Description
        )

        $components = @{}
        $binExt = if ($Platform -eq "windows") { ".exe" } else { "" }

        foreach ($name in @("garden-moss", "garden-rake", "garden-lantern")) {
            $binaryPath = Join-Path $BinDir "$name$binExt"
            if (Test-Path $binaryPath) {
                $hash = (Get-FileHash $binaryPath -Algorithm SHA256).Hash.ToLower()
                $size = (Get-Item $binaryPath).Length
                $components[$name] = @{
                    path = "bin/$name$binExt"
                    sha256 = $hash
                    size = $size
                    required = $name -in @("garden-moss", "garden-rake")
                }
            }
        }

        $manifests = @{}
        if (Test-Path $ManifestsDir) {
            # Include all manifest files
            Get-ChildItem $ManifestsDir -Recurse -File | ForEach-Object {
                $relativePath = $_.FullName.Replace("$ManifestsDir\", "").Replace("\", "/")
                $hash = (Get-FileHash $_.FullName -Algorithm SHA256).Hash.ToLower()
                $manifests[$relativePath] = $hash
            }
        }

        $scripts = @{}
        if ($ScriptsDir -and (Test-Path $ScriptsDir)) {
            # Include deployment scripts (Linux only)
            foreach ($scriptName in @("moss-update-helper.sh", "garden-upgrade.sh")) {
                $scriptPath = Join-Path $ScriptsDir $scriptName
                if (Test-Path $scriptPath) {
                    $hash = (Get-FileHash $scriptPath -Algorithm SHA256).Hash.ToLower()
                    $size = (Get-Item $scriptPath).Length
                    $scripts[$scriptName] = @{
                        path = "scripts/$scriptName"
                        sha256 = $hash
                        size = $size
                        target = "/usr/local/bin/$scriptName"
                    }
                }
            }
        }

        return @{
            version = $Version
            platform = $Platform
            architecture = "amd64"
            created = (Get-Date).ToUniversalTime().ToString("o")
            components = $components
            manifests = $manifests
            scripts = $scripts
            minimumVersion = $null
            notes = $Description
        }
    }

    # Create Linux package
    if ((Test-Path $linuxDir) -and (Get-ChildItem $linuxDir -File -ErrorAction SilentlyContinue)) {
        Write-Host "Creating Linux package..." -ForegroundColor DarkGray

        $packageName = "zen-garden-$version-linux-amd64"
        $packageDir = Join-Path $env:TEMP $packageName
        $tarPath = Join-Path $packagesDir "$packageName.tar.gz"
        $scriptsDir = $INSTALLER_DIR  # Scripts are in the installer directory

        # Clean and create package directory
        if (Test-Path $packageDir) { Remove-Item $packageDir -Recurse -Force }
        New-Item -ItemType Directory -Path $packageDir -Force | Out-Null
        New-Item -ItemType Directory -Path (Join-Path $packageDir "bin") -Force | Out-Null

        # Copy binaries
        Get-ChildItem $linuxDir -File | ForEach-Object {
            Copy-Item $_.FullName (Join-Path $packageDir "bin")
        }

        # Copy manifests if they exist
        if (Test-Path $manifestsDir) {
            Copy-Item $manifestsDir (Join-Path $packageDir "manifests") -Recurse
        }

        # Copy deployment scripts (Linux only) and ensure Unix line endings
        $scriptsPackageDir = Join-Path $packageDir "scripts"
        New-Item -ItemType Directory -Path $scriptsPackageDir -Force | Out-Null
        foreach ($scriptName in @("moss-update-helper.sh", "garden-upgrade.sh")) {
            $scriptPath = Join-Path $scriptsDir $scriptName
            if (Test-Path $scriptPath) {
                $destPath = Join-Path $scriptsPackageDir $scriptName
                Copy-Item $scriptPath $destPath
                # Ensure Unix line endings (LF only, no CRLF)
                ConvertTo-UnixLineEndings -Path $destPath
            }
        }

        # Create manifest
        $manifest = New-PackageManifest -Platform "linux" -BinDir $linuxDir -ManifestsDir $manifestsDir -ScriptsDir $scriptsDir -Version $version -Description $versionData.description
        $manifest | ConvertTo-Json -Depth 10 | Set-Content (Join-Path $packageDir "package.json") -Encoding UTF8

        # Create tar.gz using tar (available on Windows 10+)
        # Use -C to change directory and relative paths to avoid Windows path issues
        try {
            $tarFile = "$packageName.tar.gz"
            & tar -czf $tarFile -C $env:TEMP $packageName 2>&1 | Out-Null
            if ($LASTEXITCODE -eq 0 -and (Test-Path $tarFile)) {
                Move-Item $tarFile $tarPath -Force
                $sizeMB = [math]::Round((Get-Item $tarPath).Length / 1MB, 2)
                Write-Host "  ✓ $packageName.tar.gz ($sizeMB MB)" -ForegroundColor Green
            } else {
                Write-Host "  ✗ Failed to create Linux package (tar error: $LASTEXITCODE)" -ForegroundColor Red
                $buildErrors += "Linux package creation failed"
            }
        } finally {
            Remove-Item $packageDir -Recurse -Force -ErrorAction SilentlyContinue
            Remove-Item $tarFile -Force -ErrorAction SilentlyContinue
        }
    }

    # Create Windows package
    if ((Test-Path $windowsDir) -and (Get-ChildItem $windowsDir -File -ErrorAction SilentlyContinue)) {
        Write-Host "Creating Windows package..." -ForegroundColor DarkGray

        $packageName = "zen-garden-$version-windows-amd64"
        $packageDir = Join-Path $env:TEMP $packageName
        $zipPath = Join-Path $packagesDir "$packageName.zip"

        # Clean and create package directory
        if (Test-Path $packageDir) { Remove-Item $packageDir -Recurse -Force }
        New-Item -ItemType Directory -Path $packageDir -Force | Out-Null
        New-Item -ItemType Directory -Path (Join-Path $packageDir "bin") -Force | Out-Null

        # Copy binaries
        Get-ChildItem $windowsDir -File | ForEach-Object {
            Copy-Item $_.FullName (Join-Path $packageDir "bin")
        }

        # Copy manifests if they exist
        if (Test-Path $manifestsDir) {
            Copy-Item $manifestsDir (Join-Path $packageDir "manifests") -Recurse
        }

        # Create manifest (no scripts for Windows packages)
        $manifest = New-PackageManifest -Platform "windows" -BinDir $windowsDir -ManifestsDir $manifestsDir -ScriptsDir "" -Version $version -Description $versionData.description
        $manifest | ConvertTo-Json -Depth 10 | Set-Content (Join-Path $packageDir "package.json") -Encoding UTF8

        # Create zip
        if (Test-Path $zipPath) { Remove-Item $zipPath -Force }
        Compress-Archive -Path $packageDir -DestinationPath $zipPath -Force

        $sizeMB = [math]::Round((Get-Item $zipPath).Length / 1MB, 2)
        Write-Host "  ✓ $packageName.zip ($sizeMB MB)" -ForegroundColor Green

        Remove-Item $packageDir -Recurse -Force -ErrorAction SilentlyContinue
    }

    Write-Host ""
} elseif ($SkipPackages) {
    Write-Host "Skipping package creation (use -SkipPackages:`$false to enable)`n" -ForegroundColor DarkGray
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
}

Write-Host "Distribution artifacts:" -ForegroundColor Cyan

$linuxDir = Join-Path $DIST_DIR "linux"
$windowsDir = Join-Path $DIST_DIR "windows"
$packagesDir = Join-Path $DIST_DIR "packages"
$linuxArtifacts = Get-ChildItem $linuxDir -File -ErrorAction SilentlyContinue
$windowsArtifacts = Get-ChildItem $windowsDir -File -ErrorAction SilentlyContinue
$packageArtifacts = Get-ChildItem $packagesDir -File -ErrorAction SilentlyContinue

$artifacts = @($linuxArtifacts) + @($windowsArtifacts) + @($packageArtifacts)
if ($artifacts) {
    if ($linuxArtifacts) {
        Write-Host "`n  Linux ($linuxDir):" -ForegroundColor Cyan
        $linuxArtifacts | ForEach-Object {
            $sizeMB = [math]::Round($_.Length / 1MB, 2)
            $sizeStr = if ($sizeMB -lt 1) {
                "$([math]::Round($_.Length / 1KB, 0)) KB"
            } else {
                "$sizeMB MB"
            }
            Write-Host ("    ✓ {0,-18} {1,10}" -f $_.Name, $sizeStr) -ForegroundColor Green
        }
    }

    if ($windowsArtifacts) {
        Write-Host "`n  Windows ($windowsDir):" -ForegroundColor Cyan
        $windowsArtifacts | ForEach-Object {
            $sizeMB = [math]::Round($_.Length / 1MB, 2)
            $sizeStr = if ($sizeMB -lt 1) {
                "$([math]::Round($_.Length / 1KB, 0)) KB"
            } else {
                "$sizeMB MB"
            }
            Write-Host ("    ✓ {0,-18} {1,10}" -f $_.Name, $sizeStr) -ForegroundColor Green
        }
    }

    if ($packageArtifacts) {
        Write-Host "`n  Packages ($packagesDir):" -ForegroundColor Cyan
        $packageArtifacts | ForEach-Object {
            $sizeMB = [math]::Round($_.Length / 1MB, 2)
            $sizeStr = if ($sizeMB -lt 1) {
                "$([math]::Round($_.Length / 1KB, 0)) KB"
            } else {
                "$sizeMB MB"
            }
            Write-Host ("    ✓ {0,-35} {1,10}" -f $_.Name, $sizeStr) -ForegroundColor Green
        }
    }
} else {
    Write-Host "  (no artifacts found)" -ForegroundColor DarkGray
}

Write-Host "`nNext steps:" -ForegroundColor Yellow
if ($packageArtifacts) {
    Write-Host "  Package deployment:" -ForegroundColor Cyan
    Write-Host "    cd installer; .\push2all.ps1 -UsePackage"
}
if ($linuxArtifacts) {
    Write-Host "  Linux USB installer:" -ForegroundColor Cyan
    Write-Host "    cd installer; .\NewStone.ps1 -UsbDrive G:"
}
if ($windowsArtifacts) {
    Write-Host "  Windows testing:" -ForegroundColor Cyan
    Write-Host "    .\dist\windows\garden-rake.exe list"
    if (Test-Path "$windowsDir\garden-moss.exe") {
        Write-Host "    .\dist\windows\garden-moss.exe --help"
    }
}
Write-Host ""

# Exit with error if any build failed
if ($buildErrors.Count -gt 0) {
    exit 1
}

