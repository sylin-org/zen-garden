<#
.SYNOPSIS
    Build Zen Garden for all platforms including Linux i386

.DESCRIPTION
    Convenience wrapper for build.ps1 -IncludeI386.
    Produces 3 packages: Linux amd64, Linux i386, Windows amd64.

.PARAMETER Tier
    Build tier: "core" or "full". Default: "core".

.PARAMETER SkipLinux
    Skip Linux amd64 build

.PARAMETER SkipWindows
    Skip Windows build

.PARAMETER DebugBuild
    Build debug binaries

.PARAMETER Release
    Build full-release binaries (full LTO)

.PARAMETER Fast
    Use fast-release profile (default, thin LTO)

.PARAMETER ForceRebuild
    Force rebuild of Docker containers

.PARAMETER Jobs
    Number of parallel cargo jobs

.EXAMPLE
    .\build-all-platforms.ps1
    Build core binaries for Linux amd64, Linux i386, and Windows

.EXAMPLE
    .\build-all-platforms.ps1 -Tier full -SkipWindows
    Build all binaries for Linux amd64 + i386 only
#>

[CmdletBinding()]
param(
    [ValidateSet('core', 'full')]
    [string]$Tier = "core",

    [switch]$SkipLinux,
    [switch]$SkipWindows,
    [switch]$DebugBuild,
    [switch]$Release,
    [switch]$Fast,
    [switch]$ForceRebuild,
    [int]$Jobs = 0
)

$scriptPath = Join-Path $PSScriptRoot "build.ps1"

& $scriptPath `
    -Tier $Tier `
    -IncludeI386 `
    -SkipLinux:$SkipLinux `
    -SkipWindows:$SkipWindows `
    -DebugBuild:$DebugBuild `
    -Release:$Release `
    -Fast:$Fast `
    -ForceRebuild:$ForceRebuild `
    -Jobs $Jobs

exit $LASTEXITCODE
