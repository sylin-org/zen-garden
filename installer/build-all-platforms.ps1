<#
.SYNOPSIS
    Build Zen Garden for all fleet platforms (Linux x64, Windows x64, Android arm64-musl)

.DESCRIPTION
    Convenience wrapper for build.ps1 -IncludeAndroid.
    Produces packages for every platform a Stone runs on: Linux x64, Windows x64,
    and Android (aarch64-musl, the native phone Stone). Linux x86 is available via
    build.ps1 -IncludeX86 directly (no x86 Stone exists in the fleet, so it is not built here).

.PARAMETER Tier
    Build tier: "core" or "full". Default: "full".

.PARAMETER SkipLinux
    Skip Linux x64 build

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
    Build all binaries for Linux x64, Linux x86, and Windows x64

.EXAMPLE
    .\build-all-platforms.ps1 -Tier core -SkipWindows
    Build core binaries only for Linux x64 + x86
#>

[CmdletBinding()]
param(
    [ValidateSet('core', 'full')]
    [string]$Tier = "full",

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
    -SkipLinux:$SkipLinux `
    -SkipWindows:$SkipWindows `
    -IncludeAndroid `
    -DebugBuild:$DebugBuild `
    -Release:$Release `
    -Fast:$Fast `
    -ForceRebuild:$ForceRebuild `
    -Jobs $Jobs

exit $LASTEXITCODE
