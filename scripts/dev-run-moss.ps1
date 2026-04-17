<#
.SYNOPSIS
  Run garden-moss.exe locally for a fixed duration, capturing stdout/stderr.
  Used for investigation — not a long-running dev loop.

.PARAMETER Seconds
  How long to let moss run before killing it. Default 20.
#>

param(
    [int]$Seconds = 20
)

$ErrorActionPreference = "Stop"
$Repo = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$Moss = Join-Path $Repo "dist\windows-x64\garden-moss.exe"
$DevDir = Join-Path $Repo "dev"
$OutLog = Join-Path $DevDir "moss.out"
$ErrLog = Join-Path $DevDir "moss.err"

$env:GARDEN_COMPANIONS_DIR = Join-Path $DevDir "companions"
$env:GARDEN_DATA_DIR = Join-Path $DevDir "data"
$env:GARDEN_SHARED_DATA_DIR = Join-Path $DevDir "shared-data"
$env:PORT = "7185"
if (-not $env:RUST_LOG) {
    $env:RUST_LOG = "info,garden_firefly=debug,garden_companion_sdk::usb_devices=debug,garden_moss::infra::companions=debug"
}

New-Item -ItemType Directory -Force -Path (Join-Path $DevDir "data") | Out-Null
Remove-Item $OutLog, $ErrLog -ErrorAction SilentlyContinue

Write-Host "Starting moss for $Seconds s..."
$proc = Start-Process -FilePath $Moss -RedirectStandardOutput $OutLog -RedirectStandardError $ErrLog -PassThru -NoNewWindow
Start-Sleep -Seconds $Seconds
if (-not $proc.HasExited) {
    Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
}

Write-Host "`n--- STDOUT (tail 60) ---"
if (Test-Path $OutLog) { Get-Content $OutLog -Tail 60 } else { Write-Host "(empty)" }
Write-Host "`n--- STDERR (tail 60) ---"
if (Test-Path $ErrLog) { Get-Content $ErrLog -Tail 60 } else { Write-Host "(empty)" }
