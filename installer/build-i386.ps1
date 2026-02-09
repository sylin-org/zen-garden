<#
.SYNOPSIS
    Build Zen Garden i386 (32-bit) Linux distribution

.DESCRIPTION
    Convenience wrapper that builds Linux i386 binaries and creates
    a deployment package. For 32-bit stones (e.g., Atom Z5xx machines).

    Equivalent to: build-linux-i386.ps1 with auto-generated version.

.PARAMETER DebugBuild
    Build debug binaries

.PARAMETER Fast
    Use fast-release profile (default)

.PARAMETER ForceRebuild
    Force rebuild of Docker container

.PARAMETER Jobs
    Number of parallel cargo jobs

.EXAMPLE
    .\build-i386.ps1
    # Build core i386 binaries and package
#>

[CmdletBinding()]
param(
    [switch]$DebugBuild,
    [switch]$Fast,
    [switch]$ForceRebuild,
    [int]$Jobs = 0
)

# Generate version
$versionFile = Join-Path (Split-Path $PSScriptRoot -Parent) "version.json"
$versionData = Get-Content $versionFile | ConvertFrom-Json
$buildNumber = Get-Date -Format "yyyyMMddHHmm"
$version = "$($versionData.major).$($versionData.minor).$buildNumber"

$scriptPath = Join-Path $PSScriptRoot "build-linux-i386.ps1"

& $scriptPath `
    -Version $version `
    -Tier core `
    -DebugBuild:$DebugBuild `
    -Fast:$Fast `
    -ForceRebuild:$ForceRebuild `
    -Jobs $Jobs

exit $LASTEXITCODE
