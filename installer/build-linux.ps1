<#
.SYNOPSIS
    Build Zen Garden Linux binaries using Docker

.DESCRIPTION
    Intelligently builds moss and garden-rake binaries for Linux:
    - On Windows: Uses Docker container for Linux cross-compilation
    - On Linux: Can build natively or use Docker for consistency
    - Detects existing build container and reuses it (perennial)
    - Only rebuilds container when Dockerfile changes or forced

.PARAMETER DebugBuild
    Build debug binaries instead of optimized release (default: release)

.PARAMETER Fast
    Use fast-release profile (~40% faster compile, ~5-10% larger binaries)
    Uses thin LTO and parallel codegen for faster iteration

.PARAMETER ForceRebuild
    Force rebuild of Docker build container

.PARAMETER Native
    On Linux: build natively instead of using Docker

.PARAMETER CheckUpdates
    Check for outdated dependencies before building

.PARAMETER Jobs
    Number of parallel cargo jobs (default: number of CPUs)

.EXAMPLE
    .\build-linux.ps1
    # Build optimized release binaries using Docker (default, reuses existing image)

.EXAMPLE
    .\build-linux.ps1 -Fast
    # Build with fast-release profile (~40% faster, slightly larger binaries)

.EXAMPLE
    .\build-linux.ps1 -DebugBuild
    # Build debug binaries (faster compile, larger size)

.EXAMPLE
    .\build-linux.ps1 -ForceRebuild
    # Rebuild Docker image and compile debug binaries

.EXAMPLE
    .\build-linux.ps1 -Native -Release
    # On Linux: build natively without Docker

.EXAMPLE
    .\build-linux.ps1 -CheckUpdates
    # Check for outdated crates before building
#>

[CmdletBinding()]
param(
    [string]$Version,
    [switch]$DebugBuild,
    [switch]$Fast,
    [switch]$ForceRebuild,
    [switch]$Native,
    [switch]$CheckUpdates,
    [int]$Jobs = 0
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

# Detect if running on Windows (works in both Windows PowerShell 5.x and PowerShell Core 6+)
$RunningOnWindows = if ($null -ne (Get-Variable -Name IsWindows -ValueOnly -ErrorAction SilentlyContinue)) {
    $IsWindows
}
else {
    $env:OS -eq "Windows_NT"
}

$WORKSPACE_ROOT = (Get-Item $PSScriptRoot).Parent.FullName
$DIST_DIR = Join-Path $WORKSPACE_ROOT "dist"
$LINUX_DIR = Join-Path $DIST_DIR "linux"
$IMAGE_NAME = "zen-garden-builder:latest"

# Detect platform (handle Windows PowerShell which lacks $PSVersionTable.Platform)
$IsLinuxHost = $false
if ($PSVersionTable.PSVersion.Major -ge 6) {
    # PowerShell Core has Platform property
    $IsLinuxHost = $PSVersionTable.Platform -eq "Unix" -and $PSVersionTable.OS -match "Linux"
}
$IsWslHost = $null -ne $env:WSL_DISTRO_NAME
$UseDocker = -not ($IsLinuxHost -and $Native)

# Create dist directories
New-Item -ItemType Directory -Force -Path $LINUX_DIR | Out-Null

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
    Write-Host ""
}
$version = $env:GARDEN_VERSION

# Determine build profile description
$buildProfile = if ($DebugBuild) { "debug" } elseif ($Fast) { "fast-release" } else { "release" }
$buildTypeDesc = switch ($buildProfile) {
    "debug" { "Debug (fastest compile, largest binary)" }
    "fast-release" { "Fast-Release (thin LTO, ~40% faster compile)" }
    default { "Release (full LTO, smallest binary)" }
}

# Determine parallel jobs
$parallelJobs = if ($Jobs -gt 0) { $Jobs } else { [Environment]::ProcessorCount }

Write-Host "`n╔════════════════════════════════════════════════════╗" -ForegroundColor Cyan
Write-Host "║   Zen Garden Linux Build                           ║" -ForegroundColor Cyan
Write-Host "╚════════════════════════════════════════════════════╝`n" -ForegroundColor Cyan

Write-Host "Configuration:" -ForegroundColor Yellow
Write-Host "  Platform: Linux"
Write-Host "  Version: $version"
Write-Host "  Build Type: $buildTypeDesc"
Write-Host "  Parallel Jobs: $parallelJobs"
Write-Host "  Output Dir: $LINUX_DIR"
Write-Host "  Build Method: $(if ($UseDocker) { 'Docker Container' } else { 'Native' })"
if ($IsWslHost) { Write-Host "  Environment: WSL ($env:WSL_DISTRO_NAME)" }
Write-Host ""

if ($UseDocker) {
    # Docker-based build (Windows, or Linux with Docker preference)
    
    # Check Docker availability
    try {
        docker version | Out-Null
    }
    catch {
        Write-Host "✗ Docker not available." -ForegroundColor Red
        if ($RunningOnWindows) {
            Write-Host "  Install Docker Desktop: https://www.docker.com/products/docker-desktop/" -ForegroundColor Yellow
        }
        else {
            Write-Host "  Install Docker Engine or use -Native flag for native build" -ForegroundColor Yellow
        }
        exit 1
    }
    
    # Check if perennial build image exists
    $existingImage = docker images -q $IMAGE_NAME 2>$null

    if ($existingImage -and -not $ForceRebuild) {
        Write-Host "Build Container:" -ForegroundColor Yellow
        Write-Host "  ✓ Using existing image: $IMAGE_NAME" -ForegroundColor Green
        Write-Host "    (Use -ForceRebuild to recreate)" -ForegroundColor DarkGray
        Write-Host ""
    }
    else {
        Write-Host "Build Container:" -ForegroundColor Yellow
        Write-Host "  $(if ($ForceRebuild) { 'Rebuilding' } else { 'Creating' }) image: $IMAGE_NAME"
        
        Push-Location $WORKSPACE_ROOT
        try {
            docker build -f Dockerfile.build -t $IMAGE_NAME . --quiet
            if ($LASTEXITCODE -ne 0) { throw "Docker build failed" }
            Write-Host "  ✓ Image ready`n" -ForegroundColor Green
        }
        finally {
            Pop-Location
        }
    }
    
    # Determine build type (default: release for production)
    # Priority: DebugBuild > Fast > Release
    $buildProfile = if ($DebugBuild) {
        "debug"
    }
    elseif ($Fast) {
        "fast-release"  # Custom profile in Cargo.toml
    }
    else {
        "release"
    }

    # Determine parallel jobs (Docker container typically sees host CPUs)
    $parallelJobs = if ($Jobs -gt 0) { $Jobs } else { [Environment]::ProcessorCount }

    # Docker-based build
    Write-Host "Building binaries in container..." -ForegroundColor Cyan
    $buildTypeDesc = switch ($buildProfile) {
        "debug" { "Debug" }
        "fast-release" { "Fast-Release (thin LTO)" }
        default { "Release (full LTO)" }
    }
    Write-Host "  Build Type: $buildTypeDesc, Jobs: $parallelJobs" -ForegroundColor DarkGray
    
    # Determine volume mount path (Windows uses /drive/path format)
    if ($RunningOnWindows) {
        $driveLetter = $WORKSPACE_ROOT.Substring(0, 1).ToLower()
        $unixPath = "/$driveLetter" + $WORKSPACE_ROOT.Substring(2).Replace('\', '/')
    }
    else {
        $unixPath = $WORKSPACE_ROOT
    }
    
    Push-Location $WORKSPACE_ROOT
    try {
        Write-Host "  → Building garden-moss (Linux daemon)..."
        Write-Host "  → Building garden-lantern (Linux service registry)..."
        Write-Host "  → Building garden-rake (Linux CLI)..."
        Write-Host "  → Building garden-cricket (Audio adapter)..."
        
        # Generate build number if not already set by parent script
        if (-not $env:CARGO_BUILD_NUMBER) {
            $env:CARGO_BUILD_NUMBER = (Get-Date).ToString("yyyyMMdd.HHmm")
            Write-Host "  Build Number: $env:CARGO_BUILD_NUMBER" -ForegroundColor Cyan
        }
        
        # Build all four binaries in one container run for efficiency
        $buildArgs = @("cargo", "build", "-j", "$parallelJobs")
        if ($buildProfile -eq "debug") {
            # Debug build - no profile flag needed
        }
        elseif ($buildProfile -eq "fast-release") {
            $buildArgs += @("--profile", "fast-release")
        }
        else {
            $buildArgs += "--release"
        }
        $buildArgs += @("--bin", "garden-moss", "--bin", "garden-lantern", "--bin", "garden-rake", "--bin", "garden-cricket")
        
        $containerName = "zen-garden-builder-container"
        
        # Check if container already exists and is running
        $existingContainer = docker ps --filter "name=^/${containerName}$" --format "{{.Names}}" 2>$null
        $stoppedContainer = docker ps -a --filter "name=^/${containerName}$" --filter "status=exited" --format "{{.Names}}" 2>$null
        
        if ($existingContainer -eq $containerName) {
            Write-Host "  → Using running container: $containerName" -ForegroundColor DarkGray
        }
        elseif ($stoppedContainer -eq $containerName) {
            Write-Host "  → Starting existing container: $containerName" -ForegroundColor DarkGray
            docker start $containerName | Out-Null
            if ($LASTEXITCODE -ne 0) { throw "Failed to start container" }
        }
        else {
            Write-Host "  → Creating new container: $containerName" -ForegroundColor DarkGray
            
            docker run -d `
                --name $containerName `
                -v "${unixPath}:/build" `
                -v "zen-garden-cargo-cache:/root/.cargo" `
                -w /build `
                $IMAGE_NAME `
                tail -f /dev/null
            
            if ($LASTEXITCODE -ne 0) { throw "Failed to create container" }
        }
        
        # Check for outdated dependencies if requested
        if ($CheckUpdates) {
            Write-Host "`n  Checking for outdated dependencies..." -ForegroundColor Yellow
            docker exec $containerName cargo outdated --workspace --root-deps-only 2>$null
            if ($LASTEXITCODE -ne 0) {
                Write-Host "  → cargo-outdated not installed, installing..." -ForegroundColor DarkYellow
                docker exec $containerName cargo install cargo-outdated
            }
            Write-Host ""
        }
        
        # Clean build artifacts to force version update
        # (Cargo cache doesn't detect CARGO_BUILD_NUMBER changes)
        # Must clean:
        # 1. Final binaries in target/{profile}/
        # 2. Build script outputs in target/{profile}/build/garden-*/
        # 3. Incremental cache in target/{profile}/incremental/garden*/
        Write-Host "  → Cleaning cached binaries to ensure version update..." -ForegroundColor DarkGray
        docker exec $containerName sh -c "rm -f /build/target/debug/garden-* /build/target/release/garden-* /build/target/fast-release/garden-*" 2>$null | Out-Null
        docker exec $containerName sh -c "rm -rf /build/target/debug/build/garden-* /build/target/release/build/garden-* /build/target/fast-release/build/garden-*" 2>$null | Out-Null
        docker exec $containerName sh -c "rm -rf /build/target/debug/incremental/garden* /build/target/release/incremental/garden* /build/target/fast-release/incremental/garden*" 2>$null | Out-Null
        docker exec $containerName sh -c "rm -rf /build/target/debug/.fingerprint/garden-* /build/target/release/.fingerprint/garden-* /build/target/fast-release/.fingerprint/garden-*" 2>$null | Out-Null
        
        # Execute build in the persistent container with build number
        docker exec -e CARGO_BUILD_NUMBER=$env:CARGO_BUILD_NUMBER $containerName $buildArgs
        
        if ($LASTEXITCODE -ne 0) { throw "Build failed" }
        
        # Copy binaries from Docker container to dist/linux/
        # Use docker cp because volume mount may not reflect changes immediately on Windows
        Write-Host "  → Copying binaries from container..." -ForegroundColor DarkGray
        
        $copyFailed = $false
        
        docker cp "${containerName}:/build/target/${buildProfile}/garden-lantern" "$LINUX_DIR\garden-lantern" 2>$null
        if ($LASTEXITCODE -ne 0) {
            Write-Host "    ✗ Failed to copy garden-lantern" -ForegroundColor Red
            $copyFailed = $true
        }
        
        docker cp "${containerName}:/build/target/${buildProfile}/garden-moss" "$LINUX_DIR\garden-moss" 2>$null
        if ($LASTEXITCODE -ne 0) {
            Write-Host "    ✗ Failed to copy garden-moss" -ForegroundColor Red
            $copyFailed = $true
        }
        
        docker cp "${containerName}:/build/target/${buildProfile}/garden-rake" "$LINUX_DIR\garden-rake" 2>$null
        if ($LASTEXITCODE -ne 0) {
            Write-Host "    ✗ Failed to copy garden-rake" -ForegroundColor Red
            $copyFailed = $true
        }
        
        docker cp "${containerName}:/build/target/${buildProfile}/garden-cricket" "$LINUX_DIR\garden-cricket" 2>$null
        if ($LASTEXITCODE -ne 0) {
            Write-Host "    ✗ Failed to copy garden-cricket" -ForegroundColor Red
            $copyFailed = $true
        }
        
        if ($copyFailed) { throw "Failed to copy one or more binaries from container" }
        
        Write-Host "  ✓ Linux binaries built`n" -ForegroundColor Green
        
    }
    finally {
        Pop-Location
    }
    
}
else {
    # Native Linux build
    Write-Host "Building binaries natively..." -ForegroundColor Cyan

    # Determine build type (default: release for production)
    # Priority: DebugBuild > Fast > Release
    $buildProfile = if ($DebugBuild) {
        "debug"
    }
    elseif ($Fast) {
        "fast-release"  # Custom profile in Cargo.toml
    }
    else {
        "release"
    }

    # Determine parallel jobs
    $parallelJobs = if ($Jobs -gt 0) { $Jobs } else { [Environment]::ProcessorCount }

    $buildTypeDesc = switch ($buildProfile) {
        "debug" { "Debug" }
        "fast-release" { "Fast-Release (thin LTO)" }
        default { "Release (full LTO)" }
    }
    Write-Host "  Build Type: $buildTypeDesc, Jobs: $parallelJobs" -ForegroundColor DarkGray

    Push-Location $WORKSPACE_ROOT
    try {
        # Clean build artifacts to force version update (native path)
        Write-Host "  → Cleaning cached binaries to ensure version update..." -ForegroundColor DarkGray
        $targetProfileDirs = @("debug", "release", "fast-release")
        foreach ($profile in $targetProfileDirs) {
            $targetPath = Join-Path (Join-Path $WORKSPACE_ROOT "target") $profile
            if (Test-Path $targetPath) {
                Get-ChildItem $targetPath -Filter "garden-*" -File -ErrorAction SilentlyContinue | Remove-Item -Force -ErrorAction SilentlyContinue
                $buildPath = Join-Path $targetPath "build"
                if (Test-Path $buildPath) {
                    Get-ChildItem $buildPath -Filter "garden-*" -Directory -ErrorAction SilentlyContinue | Remove-Item -Recurse -Force -ErrorAction SilentlyContinue
                }
                $incrementalPath = Join-Path $targetPath "incremental"
                if (Test-Path $incrementalPath) {
                    Get-ChildItem $incrementalPath -Filter "garden*" -Directory -ErrorAction SilentlyContinue | Remove-Item -Recurse -Force -ErrorAction SilentlyContinue
                }
                $fingerprintPath = Join-Path $targetPath ".fingerprint"
                if (Test-Path $fingerprintPath) {
                    Get-ChildItem $fingerprintPath -Filter "garden-*" -Directory -ErrorAction SilentlyContinue | Remove-Item -Recurse -Force -ErrorAction SilentlyContinue
                }
            }
        }
        
        Write-Host "  → Building garden-moss (Linux daemon)..."
        Write-Host "  → Building garden-lantern (Linux service registry)..."
        Write-Host "  → Building garden-rake (Linux CLI)..."
        Write-Host "  → Building garden-cricket (Audio adapter)..."

        $buildArgs = @("build", "-j", "$parallelJobs")
        if ($buildProfile -eq "debug") {
            # Debug build - no profile flag needed
        }
        elseif ($buildProfile -eq "fast-release") {
            $buildArgs += @("--profile", "fast-release")
        }
        else {
            $buildArgs += "--release"
        }
        $buildArgs += @("--bin", "garden-moss", "--bin", "garden-lantern", "--bin", "garden-rake", "--bin", "garden-cricket")

        cargo @buildArgs
        
        if ($LASTEXITCODE -ne 0) { throw "Build failed" }
        
        # Copy binaries from target to dist/linux/
        $srcDir = Join-Path (Join-Path $WORKSPACE_ROOT "target") $buildProfile
        Copy-Item "$srcDir/garden-lantern" "$LINUX_DIR/garden-lantern-$version" -Force
        Copy-Item "$srcDir/garden-moss" "$LINUX_DIR/garden-moss-$version" -Force
        Copy-Item "$srcDir/garden-rake" "$LINUX_DIR/garden-rake-$version" -Force
        Copy-Item "$srcDir/garden-cricket" "$LINUX_DIR/garden-cricket-$version" -Force
        # Also create unversioned copies for convenience
        Copy-Item "$LINUX_DIR/garden-lantern-$version" "$LINUX_DIR/garden-lantern" -Force
        Copy-Item "$LINUX_DIR/garden-moss-$version" "$LINUX_DIR/garden-moss" -Force
        Copy-Item "$LINUX_DIR/garden-rake-$version" "$LINUX_DIR/garden-rake" -Force
        Copy-Item "$LINUX_DIR/garden-cricket-$version" "$LINUX_DIR/garden-cricket" -Force
        
        Write-Host "  ✓ Binaries built`n" -ForegroundColor Green
        
    }
    finally {
        Pop-Location
    }
}

# Display results
Write-Host "╔════════════════════════════════════════════════════╗" -ForegroundColor Green
Write-Host "║   Build Complete!                                  ║" -ForegroundColor Green
Write-Host "╚════════════════════════════════════════════════════╝`n" -ForegroundColor Green

Write-Host "Artifacts in $LINUX_DIR`:" -ForegroundColor Cyan

$artifacts = Get-ChildItem $LINUX_DIR -ErrorAction SilentlyContinue
if ($artifacts) {
    $artifacts | ForEach-Object {
        $sizeMB = [math]::Round($_.Length / 1MB, 2)
        $sizeStr = if ($sizeMB -lt 1) {
            "$([math]::Round($_.Length / 1KB, 0)) KB"
        }
        else {
            "$sizeMB MB"
        }
        
        # Verify binary type (platform-conditional)
        $marker = "-"
        if ($UseDocker -and $existingImage) {
            try {
                $fileType = docker run --rm -v "${LINUX_DIR}:/check" $IMAGE_NAME file "/check/$($_.Name)" 2>$null
                $isLinuxBinary = $fileType -match "ELF.*Linux"
                $marker = if ($isLinuxBinary) { "✓" } else { "?" }
            }
            catch {
                $marker = "-"
            }
        }
        elseif ($IsLinuxHost) {
            $fileType = file $_.FullName 2>$null
            $isLinuxBinary = $fileType -match "ELF"
            $marker = if ($isLinuxBinary) { "✓" } else { "?" }
        }
        
        Write-Host ("  {0} {1,-20} {2,10}" -f $marker, $_.Name, $sizeStr) -ForegroundColor $(if ($marker -eq "✓") { "Green" } else { "White" })
    }
}
else {
    Write-Host "  (no artifacts found)" -ForegroundColor DarkGray
}

Write-Host "`nNext steps:" -ForegroundColor Yellow
Write-Host "  1. Create USB: cd installer; .\NewStone.ps1 -UsbDrive G:"
Write-Host "  2. Deploy to Stone and test"
if ($UseDocker -and $existingImage -and -not $ForceRebuild) {
    Write-Host "  (Build container cached for next run)" -ForegroundColor DarkGray
}
Write-Host ""
