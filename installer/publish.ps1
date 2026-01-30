#!/usr/bin/env pwsh
<#
.SYNOPSIS
    Build and publish garden binaries to all stones in one step.

.DESCRIPTION
    This is a convenience script that combines dist.ps1 (build) and push2all.ps1 (deploy).
    Use this for quick iteration during development - build once, deploy everywhere.

    Steps:
    1. Run dist.ps1 to build the binaries
    2. Run push2all.ps1 with -SkipBuild -Y to deploy to all discovered stones

.PARAMETER Method
    Deployment method: 'HTTP' (via API) or 'SSH' (direct file transfer)
    Default: HTTP

.PARAMETER PublishMode
    What to publish: 'Package' (full), 'MossRake' (legacy), 'MossOnly'
    Default: MossRake (moss and rake binaries only)

.PARAMETER SSHUser
    SSH username for SSH method (default: stone)

.PARAMETER SSHPassword
    SSH password for SSH method (default: stone)

.EXAMPLE
    .\publish.ps1
    Build and deploy moss+rake to all stones via HTTP API

.EXAMPLE
    .\publish.ps1 -Method SSH
    Build and deploy via SSH (useful when API is broken)

.EXAMPLE
    .\publish.ps1 -PublishMode MossOnly
    Build and deploy only moss (skip rake)
#>

param(
    [ValidateSet('HTTP', 'SSH')]
    [string]$Method = 'HTTP',
    [ValidateSet('Package', 'MossRake', 'MossOnly')]
    [string]$PublishMode = 'Package',
    [string]$SSHUser = 'stone',
    [string]$SSHPassword = 'stone'
)

$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path

Write-Host "`n========================================" -ForegroundColor Cyan
Write-Host "  Zen Garden Publish" -ForegroundColor Cyan
Write-Host "========================================`n" -ForegroundColor Cyan

# Step 1: Build
Write-Host "Step 1: Building binaries..." -ForegroundColor Yellow
Write-Host "----------------------------------------" -ForegroundColor DarkGray

$distScript = Join-Path $ScriptDir "dist.ps1"
if (-not (Test-Path $distScript)) {
    Write-Host "Error: dist.ps1 not found at $distScript" -ForegroundColor Red
    exit 1
}

& $distScript
if ($LASTEXITCODE -ne 0) {
    Write-Host "`nBuild failed. Aborting publish." -ForegroundColor Red
    exit 1
}

Write-Host "`nBuild completed successfully." -ForegroundColor Green

# Step 2: Deploy
Write-Host "`nStep 2: Deploying to all stones..." -ForegroundColor Yellow
Write-Host "----------------------------------------" -ForegroundColor DarkGray

$push2allScript = Join-Path $ScriptDir "push2all.ps1"
if (-not (Test-Path $push2allScript)) {
    Write-Host "Error: push2all.ps1 not found at $push2allScript" -ForegroundColor Red
    exit 1
}

# Build arguments
$pushArgs = @(
    "-SkipBuild",
    "-Y",
    "-Method", $Method,
    "-PublishMode", $PublishMode
)

if ($Method -eq 'SSH') {
    $pushArgs += @("-SSHUser", $SSHUser, "-SSHPassword", $SSHPassword)
}

& $push2allScript @pushArgs
if ($LASTEXITCODE -ne 0) {
    Write-Host "`nDeployment failed." -ForegroundColor Red
    exit 1
}

Write-Host "`n========================================" -ForegroundColor Green
Write-Host "  Publish Complete!" -ForegroundColor Green
Write-Host "========================================`n" -ForegroundColor Green
