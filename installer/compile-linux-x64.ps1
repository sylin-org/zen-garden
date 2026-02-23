<#
.SYNOPSIS
    Compile Zen Garden Linux binaries using Docker

.DESCRIPTION
    Intelligently compiles moss and garden-rake binaries for Linux:
    - On Windows: Uses Docker container for Linux cross-compilation
    - On Linux: Can build natively or use Docker for consistency
    - Detects existing build container and reuses it (perennial)
    - Only rebuilds container when Dockerfile changes or forced

.PARAMETER Targets
    List of cargo package names to build (e.g., "garden-moss", "garden-rake")
    If not specified, builds all binaries.

.PARAMETER DebugBuild
    Compile debug binaries instead of optimized release (default: release)

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
    .\compile-linux-x64.ps1
    # Build all binaries using Docker (default)

.EXAMPLE
    .\compile-linux-x64.ps1 -Targets "garden-moss","garden-rake"
    # Build only moss and rake (core tier)

.EXAMPLE
    .\compile-linux-x64.ps1 -Fast
    # Build with fast-release profile (~40% faster, slightly larger binaries)

.EXAMPLE
    .\compile-linux-x64.ps1 -DebugBuild
    # Compile debug binaries (faster compile, larger size)

.EXAMPLE
    .\compile-linux-x64.ps1 -ForceRebuild
    # Rebuild Docker image and compile debug binaries

.EXAMPLE
    .\compile-linux-x64.ps1 -Native -Release
    # On Linux: build natively without Docker

.EXAMPLE
    .\compile-linux-x64.ps1 -CheckUpdates
    # Check for outdated crates before building
#>

[CmdletBinding()]
param(
    [string]$Version,
    [string[]]$Targets,
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
$LINUX_DIR = Join-Path $DIST_DIR "linux-x64"
$IMAGE_NAME = "zen-builder-linux-x64:latest"

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
Write-Host "║   Zen Garden Linux x64 Build                       ║" -ForegroundColor Cyan
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
        
        if ($ForceRebuild) {
            # Remove existing container so it gets recreated from the new image
            Write-Host "  Removing old container..." -ForegroundColor DarkGray
            docker rm -f zen-builder-linux-x64 2>$null | Out-Null
        }
        
        Push-Location $WORKSPACE_ROOT
        try {
            docker build -f Dockerfile.linux-x64 -t $IMAGE_NAME . --quiet
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

    # Determine which binaries to build (needed before Lantern frontend check)
    $defaultTargets = @("garden-moss", "garden-lantern", "garden-rake", "garden-cricket", "garden-firefly")
    $buildTargets = if ($Targets -and $Targets.Count -gt 0) { $Targets } else { $defaultTargets }

    # Build Lantern frontend (on host, before Docker cargo build)
    if ($buildTargets -contains "garden-lantern") {
        $frontendDir = Join-Path $WORKSPACE_ROOT "src/lantern/frontend"
        if (Test-Path (Join-Path $frontendDir "package.json")) {
            Write-Host "Building Lantern frontend SPA..." -ForegroundColor Yellow

            $hasBun = Get-Command bun -ErrorAction SilentlyContinue
            $hasNpm = Get-Command npm -ErrorAction SilentlyContinue

            Push-Location $frontendDir
            try {
                if ($hasBun) {
                    Write-Host "  Using bun..." -ForegroundColor DarkGray
                    bun install --frozen-lockfile 2>$null
                    if ($LASTEXITCODE -ne 0) { bun install }
                    & ./node_modules/.bin/vite build
                } elseif ($hasNpm) {
                    Write-Host "  Using npm..." -ForegroundColor DarkGray
                    npm ci 2>$null
                    if ($LASTEXITCODE -ne 0) { npm install }
                    npx vite build
                } else {
                    Write-Host "  ⚠ Neither bun nor npm found — skipping frontend build" -ForegroundColor Yellow
                    Write-Host "    Lantern will embed whatever is in frontend/dist/" -ForegroundColor DarkGray
                }

                if ($LASTEXITCODE -eq 0 -and (Test-Path (Join-Path $frontendDir "dist/index.html"))) {
                    Write-Host "  ✓ Lantern frontend built`n" -ForegroundColor Green
                } elseif ($LASTEXITCODE -ne 0) {
                    Write-Host "  ⚠ Frontend build failed (exit code $LASTEXITCODE) — continuing with cargo build`n" -ForegroundColor Yellow
                }
            } finally {
                Pop-Location
            }
        }
    }

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

    # Koi repo is a sibling directory — mount it so path dependency resolves
    # Cargo.toml: koi-embedded = { path = "../koi/crates/koi-embedded" }
    # Inside container: /build/../koi → /koi
    $koiHostPath = (Resolve-Path (Join-Path $WORKSPACE_ROOT "../koi")).Path
    if ($RunningOnWindows) {
        $koiDriveLetter = $koiHostPath.Substring(0, 1).ToLower()
        $koiUnixPath = "/$koiDriveLetter" + $koiHostPath.Substring(2).Replace('\', '/')
    }
    else {
        $koiUnixPath = $koiHostPath
    }
    
    Push-Location $WORKSPACE_ROOT
    try {
        foreach ($target in $buildTargets) {
            Write-Host "  → Building $target..."
        }

        # Generate build number if not already set by parent script
        if (-not $env:CARGO_BUILD_NUMBER) {
            $env:CARGO_BUILD_NUMBER = (Get-Date).ToString("yyyyMMdd.HHmm")
            Write-Host "  Build Number: $env:CARGO_BUILD_NUMBER" -ForegroundColor Cyan
        }
        
        # Determine which binaries to build
        $defaultTargets = @("garden-moss", "garden-lantern", "garden-rake", "garden-cricket", "garden-firefly")
        $buildTargets = if ($Targets -and $Targets.Count -gt 0) { $Targets } else { $defaultTargets }

        # Build binaries in one container run for efficiency
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
        foreach ($target in $buildTargets) {
            $buildArgs += @("--bin", $target)
        }
        
        $containerName = "zen-builder-linux-x64"
        
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
                -v "${koiUnixPath}:/koi" `
                -v "zen-cargo-cache-linux-x64:/root/.cargo" `
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
        
        # Version update detection: build.rs declares cargo:rerun-if-env-changed=CARGO_BUILD_NUMBER
        # so Cargo automatically re-runs build scripts and recompiles affected crates when the
        # build number changes. No manual cache cleaning needed — incremental compilation works.

        # Execute build with isolated target directory
        # Mold linker: clang+mold are installed in the Docker image (Dockerfile.linux-x64).
        # Passed as env vars here (not .cargo/config.toml) to avoid affecting x86 cross-compilation.
        docker exec -e CARGO_BUILD_NUMBER=$env:CARGO_BUILD_NUMBER -e CARGO_TARGET_DIR=/build/target-linux-x64 -e CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=clang -e "CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS=-C link-arg=-fuse-ld=mold" $containerName $buildArgs
        
        if ($LASTEXITCODE -ne 0) { throw "Build failed" }
        
        # Copy binaries from Docker container to dist/linux-x64/
        # Use docker cp because volume mount may not reflect changes immediately on Windows
        Write-Host "  → Copying binaries from container..." -ForegroundColor DarkGray

        $copyFailed = $false

        foreach ($target in $buildTargets) {
            docker cp "${containerName}:/build/target-linux-x64/${buildProfile}/${target}" "$LINUX_DIR\$target" 2>$null
            if ($LASTEXITCODE -ne 0) {
                Write-Host "    ✗ Failed to copy $target" -ForegroundColor Red
                $copyFailed = $true
            }
        }

        if ($copyFailed) { throw "Failed to copy one or more binaries from container" }
        
        Write-Host "  ✓ Linux x64 binaries built`n" -ForegroundColor Green
        
    }
    finally {
        Pop-Location
    }
    
}
else {
    # Build Lantern frontend (on host, before native cargo build)
    # Determine which binaries to build (need this early for frontend check)
    $defaultTargets = @("garden-moss", "garden-lantern", "garden-rake", "garden-cricket", "garden-firefly")
    $buildTargets = if ($Targets -and $Targets.Count -gt 0) { $Targets } else { $defaultTargets }

    if ($buildTargets -contains "garden-lantern") {
        $frontendDir = Join-Path $WORKSPACE_ROOT "src/lantern/frontend"
        if (Test-Path (Join-Path $frontendDir "package.json")) {
            Write-Host "Building Lantern frontend SPA..." -ForegroundColor Yellow

            $hasBun = Get-Command bun -ErrorAction SilentlyContinue
            $hasNpm = Get-Command npm -ErrorAction SilentlyContinue

            Push-Location $frontendDir
            try {
                if ($hasBun) {
                    Write-Host "  Using bun..." -ForegroundColor DarkGray
                    bun install --frozen-lockfile 2>$null
                    if ($LASTEXITCODE -ne 0) { bun install }
                    & ./node_modules/.bin/vite build
                } elseif ($hasNpm) {
                    Write-Host "  Using npm..." -ForegroundColor DarkGray
                    npm ci 2>$null
                    if ($LASTEXITCODE -ne 0) { npm install }
                    npx vite build
                } else {
                    Write-Host "  ⚠ Neither bun nor npm found — skipping frontend build" -ForegroundColor Yellow
                    Write-Host "    Lantern will embed whatever is in frontend/dist/" -ForegroundColor DarkGray
                }

                if ($LASTEXITCODE -eq 0 -and (Test-Path (Join-Path $frontendDir "dist/index.html"))) {
                    Write-Host "  ✓ Lantern frontend built`n" -ForegroundColor Green
                } elseif ($LASTEXITCODE -ne 0) {
                    Write-Host "  ⚠ Frontend build failed (exit code $LASTEXITCODE) — continuing with cargo build`n" -ForegroundColor Yellow
                }
            } finally {
                Pop-Location
            }
        }
    }

    # Native Linux build
    Write-Host "Building binaries natively..." -ForegroundColor Cyan

    # Determine which binaries to build
    $defaultTargets = @("garden-moss", "garden-lantern", "garden-rake", "garden-cricket", "garden-firefly")
    $buildTargets = if ($Targets -and $Targets.Count -gt 0) { $Targets } else { $defaultTargets }

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
        # Version update detection: build.rs declares cargo:rerun-if-env-changed=CARGO_BUILD_NUMBER
        # so Cargo automatically re-runs build scripts and recompiles affected crates when the
        # build number changes. No manual cache cleaning needed — incremental compilation works.

        foreach ($target in $buildTargets) {
            Write-Host "  → Building $target..."
        }

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
        foreach ($target in $buildTargets) {
            $buildArgs += @("--bin", $target)
        }

        $env:CARGO_TARGET_DIR = Join-Path $WORKSPACE_ROOT "target-linux-x64"
        cargo @buildArgs

        if ($LASTEXITCODE -ne 0) { throw "Build failed" }

        # Copy binaries from target-linux-x64 to dist/linux-x64/
        $srcDir = Join-Path (Join-Path $WORKSPACE_ROOT "target-linux-x64") $buildProfile
        foreach ($target in $buildTargets) {
            $srcPath = Join-Path $srcDir $target
            if (Test-Path $srcPath) {
                Copy-Item $srcPath "$LINUX_DIR/$target" -Force
            }
        }

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
Write-Host "  1. Create USB: cd installer; .\NewStone-linux-x64.ps1 -UsbDrive G:"
Write-Host "  2. Deploy to Stone and test"
if ($UseDocker -and $existingImage -and -not $ForceRebuild) {
    Write-Host "  (Build container cached for next run)" -ForegroundColor DarkGray
}
Write-Host ""
