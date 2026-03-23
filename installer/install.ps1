# install.ps1 - One-liner Zen Garden installer for Windows
#
# Usage (run as Administrator):
#   irm https://raw.githubusercontent.com/sylin-org/zen-garden/dev/installer/install.ps1 | iex
#
# Or with flags:
#   & ([scriptblock]::Create((irm https://raw.githubusercontent.com/sylin-org/zen-garden/dev/installer/install.ps1)))
#
# For dry-run:
#   $env:ZG_INSTALL_FLAGS = "--dry-run"; irm .../install.ps1 | iex

param()

$ErrorActionPreference = "Stop"
$Repo = "sylin-org/zen-garden"
$Platform = "windows-x64"
$InstallFlags = if ($env:ZG_INSTALL_FLAGS) { $env:ZG_INSTALL_FLAGS } else { "" }

# -- Privilege check --------------------------------------------------

$isAdmin = ([Security.Principal.WindowsPrincipal] [Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole(
    [Security.Principal.WindowsBuiltInRole]::Administrator
)

if (-not $isAdmin) {
    Write-Host ""
    Write-Host "This installer requires Administrator privileges." -ForegroundColor Red
    Write-Host "Right-click your terminal and choose 'Run as administrator', then try again."
    Write-Host ""
    exit 1
}

# -- Fetch latest release --------------------------------------------

Write-Host ""
Write-Host "  Zen Garden Installer"
Write-Host ""

Write-Host "Fetching latest release from GitHub..."

try {
    $headers = @{
        "Accept"     = "application/vnd.github+json"
        "User-Agent" = "zen-garden-installer"
    }
    $release = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases/latest" -Headers $headers
}
catch {
    Write-Host "Could not reach GitHub API. For offline install, download manually:" -ForegroundColor Red
    Write-Host "  https://github.com/$Repo/releases/latest"
    exit 1
}

$version = $release.tag_name
Write-Host "  Latest version: $version"

# -- Find matching assets --------------------------------------------

$pkgAsset = $release.assets | Where-Object {
    $_.name -like "zen-garden-*-$Platform.zip"
} | Select-Object -First 1

if (-not $pkgAsset) {
    Write-Host "No package found for platform: $Platform" -ForegroundColor Red
    Write-Host "Available at: https://github.com/$Repo/releases/latest"
    exit 1
}

# -- Download ---------------------------------------------------------

$tmpDir = Join-Path $env:TEMP "zen-garden-install-$(Get-Random)"
New-Item -ItemType Directory -Path $tmpDir -Force | Out-Null

try {
    $pkgPath = Join-Path $tmpDir $pkgAsset.name
    Write-Host "Downloading $($pkgAsset.name)..."
    Invoke-WebRequest -Uri $pkgAsset.browser_download_url -OutFile $pkgPath -UseBasicParsing

    # Extract garden-moss.exe from the package
    Write-Host "Extracting garden-moss.exe..."
    Expand-Archive -Path $pkgPath -DestinationPath $tmpDir -Force

    $mossExe = Get-ChildItem -Path $tmpDir -Recurse -Filter "garden-moss.exe" | Select-Object -First 1
    if (-not $mossExe) {
        Write-Host "Could not find garden-moss.exe in package." -ForegroundColor Red
        exit 1
    }

    # Copy moss and package to a flat working directory
    $workDir = Join-Path $tmpDir "work"
    New-Item -ItemType Directory -Path $workDir -Force | Out-Null
    Copy-Item $mossExe.FullName (Join-Path $workDir "garden-moss.exe")
    Copy-Item $pkgPath $workDir

    # -- Run install --------------------------------------------------

    Write-Host ""
    Write-Host "Running garden-moss install..."
    Write-Host ""

    # Auto-accept prompts (non-interactive context) + forward user flags
    $installArgs = @("install", "--yes")
    if ($InstallFlags) {
        $installArgs += $InstallFlags.Split(" ")
    }

    & (Join-Path $workDir "garden-moss.exe") @installArgs

}
finally {
    # Cleanup
    Remove-Item -Path $tmpDir -Recurse -Force -ErrorAction SilentlyContinue
}
