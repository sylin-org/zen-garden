<#
.SYNOPSIS
    Build full Zen Garden distribution (all binaries including Companions)

.DESCRIPTION
    Convenience wrapper for build.ps1 -Tier full.
    Builds all binaries: moss, rake, lantern, cricket, firefly.

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
    .\build-full.ps1
    Build all binaries for both platforms

.EXAMPLE
    .\build-full.ps1 -SkipWindows
    Build all binaries for Linux only
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

# Forward all parameters to build.ps1 with -Tier full
$scriptPath = Join-Path $PSScriptRoot "build.ps1"

& $scriptPath `
    -Tier full `
    -SkipLinux:$SkipLinux `
    -SkipWindows:$SkipWindows `
    -DebugBuild:$DebugBuild `
    -Release:$Release `
    -Fast:$Fast `
    -ForceRebuild:$ForceRebuild `
    -Jobs $Jobs

exit $LASTEXITCODE
