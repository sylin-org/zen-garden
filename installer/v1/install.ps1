<#
.SYNOPSIS
    Zen Garden v1 installer (Windows).

.DESCRIPTION
    Downloads the newest release bundle for windows-x86_64, verifies the
    sha256 checksum, and installs moss.exe + rake.exe into
    %USERPROFILE%\.zen-garden\bin (override with -InstallDir).

.EXAMPLE
    .\install.ps1
    .\install.ps1 -InstallDir D:\tools\zg
#>
[CmdletBinding()]
param(
    [string]$InstallDir = "$env:USERPROFILE\.zen-garden\bin",
    [string]$Repo = "sylin-org/zen-garden"
)

$ErrorActionPreference = "Stop"
$bundle = "zen-garden-windows-x86_64.zip"
$base = "https://github.com/$Repo/releases/latest/download"
$tmp = Join-Path ([System.IO.Path]::GetTempPath()) ("zg-install-" + [guid]::NewGuid())
New-Item -ItemType Directory -Force -Path $tmp | Out-Null
New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null

try {
    Write-Host "fetching $bundle..."
    Invoke-WebRequest "$base/$bundle" -OutFile "$tmp/$bundle"
    Invoke-WebRequest "$base/checksums.txt" -OutFile "$tmp/checksums.txt"

    Write-Host "verifying..."
    $line = (Get-Content "$tmp/checksums.txt") | Where-Object { $_ -like "*$bundle" } | Select-Object -First 1
    $want = ($line -split '\s+')[0]
    $got = (Get-FileHash "$tmp/$bundle" -Algorithm SHA256).Hash.ToLower()
    if (-not $want -or $got -ne $want.ToLower()) {
        throw "checksum MISMATCH: want '$want', got '$got' - refusing"
    }

    Expand-Archive "$tmp/$bundle" -DestinationPath $tmp
    foreach ($bin in @("moss.exe", "rake.exe")) {
        Move-Item (Join-Path $tmp $bin) (Join-Path $InstallDir $bin) -Force
    }

    Write-Host ""
    Write-Host "installed: $InstallDir\moss.exe, $InstallDir\rake.exe"
    if (($env:PATH -split ";") -notcontains $InstallDir) {
        Write-Host "note: $InstallDir is not on your PATH - add it to use these directly"
    }
    Write-Host "next: run 'rake observe' near a running moss, or start one: `$env:MOSS_RUNTIME='docker'; moss"
} finally {
    Remove-Item $tmp -Recurse -Force -ErrorAction SilentlyContinue
}
