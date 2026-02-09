<#
.SYNOPSIS
    Adds legacy BIOS boot support to a NewStone USB drive.

.DESCRIPTION
    NewStone.ps1 creates USB drives that boot via UEFI (EFI\BOOT\BOOTX64.EFI).
    This script patches an existing NewStone USB to also boot on legacy BIOS
    machines and 32-bit UEFI firmware by:

    1. Writing SYSLINUX MBR boot code (440 bytes) to the disk's MBR
    2. Installing SYSLINUX VBR on the FAT32 partition
    3. Creating a syslinux.cfg that chains to the existing isolinux/ configs
    4. Adding BOOTIA32.EFI for 32-bit UEFI machines

    The result is a universal boot USB: UEFI (64-bit and 32-bit) + legacy BIOS.
    Existing UEFI boot is not affected.

.PARAMETER UsbDrive
    The drive letter of the USB drive (e.g., "G:" or "G").
    If not provided, auto-detects.

.PARAMETER Force
    Skip confirmation prompts.

.EXAMPLE
    .\Repair-LegacyBoot.ps1
    # Auto-detects USB drive

.EXAMPLE
    .\Repair-LegacyBoot.ps1 -UsbDrive "G:"

.NOTES
    Requires: Windows 10/11, Administrator privileges
    Run AFTER NewStone.ps1 has created the USB.
    Dependencies: installer/dependencies/syslinux/ and installer/dependencies/bootia32.efi
#>

[CmdletBinding(SupportsShouldProcess = $true, ConfirmImpact = 'High')]
param(
    [Parameter(Mandatory = $false)]
    [ValidatePattern('^[A-Za-z]:?$')]
    [string]$UsbDrive,

    [switch]$Force
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

#region Helpers

function Write-Step {
    param(
        [string]$Message,
        [string]$Status = "..."
    )
    $symbol = switch ($Status) {
        "OK"   { "[+]" }
        "FAIL" { "[x]" }
        "WARN" { "[!]" }
        "SKIP" { "[-]" }
        default { "[*]" }
    }
    $color = switch ($Status) {
        "OK"   { "Green" }
        "FAIL" { "Red" }
        "WARN" { "Yellow" }
        "SKIP" { "DarkGray" }
        default { "Cyan" }
    }
    Write-Host "  $symbol " -ForegroundColor $color -NoNewline
    Write-Host $Message
}

function Test-Administrator {
    $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
    $principal = New-Object Security.Principal.WindowsPrincipal($identity)
    return $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}

#endregion

#region Main

function Main {
    Write-Host ""
    Write-Host "  Repair-LegacyBoot" -ForegroundColor Cyan
    Write-Host "  Adds legacy BIOS + 32-bit UEFI boot to a NewStone USB" -ForegroundColor Gray
    Write-Host ""

    # Check admin
    if (-not (Test-Administrator)) {
        Write-Step "This script requires Administrator privileges" "FAIL"
        Write-Host "       Right-click PowerShell and select 'Run as Administrator'" -ForegroundColor Gray
        exit 1
    }
    Write-Step "Running as Administrator" "OK"

    # Resolve dependencies
    $depsDir = Join-Path $PSScriptRoot "dependencies"
    $syslinuxDir = Join-Path $depsDir "syslinux"
    $mbrBin = Join-Path $syslinuxDir "mbr.bin"
    $syslinuxExe = Join-Path $syslinuxDir "syslinux64.exe"
    $bootia32Efi = Join-Path $depsDir "bootia32.efi"

    $missingDeps = @()
    if (-not (Test-Path $mbrBin)) { $missingDeps += "syslinux/mbr.bin" }
    if (-not (Test-Path $syslinuxExe)) { $missingDeps += "syslinux/syslinux64.exe" }
    if (-not (Test-Path $bootia32Efi)) { $missingDeps += "bootia32.efi" }

    if ($missingDeps.Count -gt 0) {
        Write-Step "Missing dependencies in $depsDir :" "FAIL"
        foreach ($dep in $missingDeps) {
            Write-Host "         $dep" -ForegroundColor Yellow
        }
        exit 1
    }
    Write-Step "Dependencies found" "OK"

    # Verify mbr.bin is correct size
    $mbrSize = (Get-Item $mbrBin).Length
    if ($mbrSize -ne 440) {
        Write-Step "mbr.bin is $mbrSize bytes (expected 440)" "FAIL"
        exit 1
    }

    # Find USB drive
    if ($UsbDrive) {
        $driveLetter = ($UsbDrive -replace ':$', '').ToUpper()
    }
    else {
        Write-Step "Detecting USB drives..." "..."
        $usbDisks = Get-Disk | Where-Object { $_.BusType -eq 'USB' -and $_.OperationalStatus -eq 'Online' }
        if (-not $usbDisks) {
            Write-Step "No USB drives detected" "FAIL"
            exit 1
        }

        $found = $null
        foreach ($disk in $usbDisks) {
            $partitions = Get-Partition -DiskNumber $disk.DiskNumber -ErrorAction SilentlyContinue |
                Where-Object { $_.DriveLetter }
            foreach ($p in $partitions) {
                $vol = Get-Volume -DriveLetter $p.DriveLetter -ErrorAction SilentlyContinue
                if ($vol -and $vol.FileSystem -eq 'FAT32') {
                    # Check if it looks like a NewStone USB
                    $testPath = "$($p.DriveLetter):\isolinux"
                    if (Test-Path $testPath) {
                        $found = @{ DriveLetter = $p.DriveLetter; DiskNumber = $disk.DiskNumber; FriendlyName = $disk.FriendlyName }
                        break
                    }
                }
            }
            if ($found) { break }
        }

        if (-not $found) {
            Write-Step "No NewStone USB found (looking for FAT32 with isolinux/)" "FAIL"
            exit 1
        }

        $driveLetter = $found.DriveLetter
        Write-Step "Found NewStone USB: $($driveLetter): ($($found.FriendlyName))" "OK"
    }

    $driveRoot = "$($driveLetter):"
    $diskNumber = $null

    # Get disk number for the drive
    $partition = Get-Partition -DriveLetter $driveLetter -ErrorAction Stop
    $diskNumber = $partition.DiskNumber

    # Validate this is a NewStone USB
    $isolinuxDir = Join-Path $driveRoot "isolinux"
    $preseedCfg = Join-Path $driveRoot "preseed.cfg"
    $efiBootDir = Join-Path $driveRoot "EFI\BOOT"

    if (-not (Test-Path $isolinuxDir)) {
        Write-Step "Not a NewStone USB: isolinux/ directory missing" "FAIL"
        exit 1
    }
    if (-not (Test-Path $preseedCfg)) {
        Write-Step "Not a NewStone USB: preseed.cfg missing" "FAIL"
        exit 1
    }
    Write-Step "Validated NewStone USB on $driveRoot (Disk $diskNumber)" "OK"

    # Safety: refuse to touch system disk
    $systemDisk = (Get-Partition | Where-Object { $_.DriveLetter -eq $env:SystemDrive[0] } |
        Select-Object -First 1).DiskNumber
    if ($diskNumber -eq $systemDisk) {
        Write-Step "SAFETY: Refusing to modify system disk" "FAIL"
        exit 1
    }

    # Confirm
    if (-not $Force) {
        Write-Host ""
        Write-Host "  This will add legacy BIOS boot support to $driveRoot (Disk $diskNumber)" -ForegroundColor Yellow
        Write-Host "  Changes:" -ForegroundColor White
        Write-Host "    - Write 440 bytes of MBR boot code (partition table preserved)" -ForegroundColor Gray
        Write-Host "    - Install SYSLINUX VBR + runtime on FAT32 partition" -ForegroundColor Gray
        Write-Host "    - Create syslinux.cfg bridging to existing isolinux/ configs" -ForegroundColor Gray
        Write-Host "    - Add BOOTIA32.EFI for 32-bit UEFI (if missing)" -ForegroundColor Gray
        Write-Host ""
        Write-Host "  Existing UEFI boot (BOOTX64.EFI) is NOT affected." -ForegroundColor Green
        Write-Host ""
        Write-Host "  Proceed? [Y/N]: " -NoNewline -ForegroundColor Yellow
        $confirm = Read-Host
        if ($confirm -notin @('y', 'Y', 'yes', 'Yes')) {
            Write-Host "  Cancelled." -ForegroundColor Yellow
            return
        }
    }

    # Clear read-only if set
    $disk = Get-Disk -Number $diskNumber
    if ($disk.IsReadOnly) {
        Write-Step "Clearing read-only on disk $diskNumber..." "..."
        Set-Disk -Number $diskNumber -IsReadOnly $false
        Write-Step "Read-only cleared" "OK"
    }

    # =========================================================================
    # Step 1: Write MBR boot code
    # =========================================================================
    Write-Step "Writing MBR boot code to disk $diskNumber..." "..."

    $mbrBytes = [System.IO.File]::ReadAllBytes($mbrBin)
    $diskPath = "\\.\PhysicalDrive$diskNumber"

    try {
        # Raw disk I/O on Windows requires sector-aligned reads/writes (512-byte multiples).
        # Strategy: read the full 512-byte MBR sector, splice in our 440 bytes of boot code
        # (preserving disk signature, partition table, and boot signature), write back the
        # full sector.
        $stream = [System.IO.FileStream]::new(
            $diskPath,
            [System.IO.FileMode]::Open,
            [System.IO.FileAccess]::ReadWrite,
            [System.IO.FileShare]::ReadWrite
        )
        try {
            # Read existing 512-byte MBR sector
            $sector = New-Object byte[] 512
            $bytesRead = $stream.Read($sector, 0, 512)
            if ($bytesRead -ne 512) {
                throw "Could not read full MBR sector (got $bytesRead bytes)"
            }

            # Verify boot signature exists (0x55, 0xAA at offset 510-511)
            if ($sector[510] -ne 0x55 -or $sector[511] -ne 0xAA) {
                Write-Step "WARNING: No MBR boot signature found - disk may not be MBR formatted" "WARN"
            }

            # Splice: overwrite first 440 bytes (boot code) while preserving
            # bytes 440-445 (disk signature), 446-509 (partition table), 510-511 (0x55AA)
            [Array]::Copy($mbrBytes, 0, $sector, 0, 440)

            # Write back the full 512-byte sector
            $stream.Seek(0, [System.IO.SeekOrigin]::Begin) | Out-Null
            $stream.Write($sector, 0, 512)
            $stream.Flush()
        }
        finally {
            $stream.Close()
        }
        Write-Step "MBR boot code written (440 bytes spliced, partition table preserved)" "OK"
    }
    catch {
        Write-Step "Failed to write MBR: $_" "FAIL"
        throw
    }

    # =========================================================================
    # Step 2: Install SYSLINUX VBR
    # =========================================================================
    Write-Step "Installing SYSLINUX on $driveRoot..." "..."

    try {
        # --force: required for fresh install (no existing syslinux on the partition)
        # --install: install VBR boot code + ldlinux.sys + ldlinux.c32
        # Use cmd /c because PowerShell's native exe argument passing mangles syslinux's flags
        $cmdLine = "`"$syslinuxExe`" --force --install $driveRoot"
        Write-Host "       Running: $cmdLine" -ForegroundColor Gray
        $result = cmd /c "$cmdLine 2>&1"
        $exitCode = $LASTEXITCODE
        if ($result) {
            Write-Host "       syslinux output: $result" -ForegroundColor Gray
        }
        if ($exitCode -ne 0) {
            throw "syslinux64.exe exited with code $exitCode : $result"
        }

        # Verify ldlinux.sys was created (syslinux marks it hidden+system)
        $ldlinux = Join-Path $driveRoot "ldlinux.sys"
        if (-not (Test-Path $ldlinux)) {
            throw "ldlinux.sys not found after syslinux install"
        }
        $ldSize = (Get-Item $ldlinux -Force).Length
        Write-Step "SYSLINUX installed (ldlinux.sys: $ldSize bytes)" "OK"
    }
    catch {
        Write-Step "Failed to install SYSLINUX: $_" "FAIL"
        throw
    }

    # =========================================================================
    # Step 3: Write syslinux.cfg
    # =========================================================================
    Write-Step "Writing syslinux.cfg..." "..."

    # Chain to the existing isolinux config.
    # CONFIG directive loads isolinux.cfg with isolinux/ as the working directory,
    # so all relative paths (kernels, initrds, .c32 modules) resolve correctly.
    $syslinuxCfg = @"
# Bridge to existing Debian isolinux configuration.
# Added by Repair-LegacyBoot.ps1 for legacy BIOS boot support.
DEFAULT install
LABEL install
  CONFIG /isolinux/isolinux.cfg /isolinux
"@

    $syslinuxCfgPath = Join-Path $driveRoot "syslinux.cfg"
    # Write without BOM for syslinux compatibility
    [System.IO.File]::WriteAllText($syslinuxCfgPath, $syslinuxCfg, [System.Text.UTF8Encoding]::new($false))
    Write-Step "syslinux.cfg written (chains to isolinux/)" "OK"

    # =========================================================================
    # Step 4: Add BOOTIA32.EFI for 32-bit UEFI
    # =========================================================================
    $efiDest = Join-Path $efiBootDir "bootia32.efi"
    if (Test-Path $efiDest) {
        Write-Step "BOOTIA32.EFI already present" "SKIP"
    }
    elseif (Test-Path $efiBootDir) {
        Write-Step "Adding BOOTIA32.EFI for 32-bit UEFI..." "..."
        Copy-Item $bootia32Efi $efiDest -Force
        Write-Step "BOOTIA32.EFI added to EFI\BOOT\" "OK"
    }
    else {
        Write-Step "EFI\BOOT\ directory missing, skipping BOOTIA32.EFI" "WARN"
    }

    # =========================================================================
    # Step 5: Re-enable read-only
    # =========================================================================
    Write-Step "Setting USB to read-only..." "..."
    try {
        Set-Disk -Number $diskNumber -IsReadOnly $true
        Write-Step "USB set to read-only" "OK"
    }
    catch {
        Write-Step "Could not set read-only: $_" "WARN"
    }

    # Done
    Write-Host ""
    Write-Host "  Legacy boot support added." -ForegroundColor Green
    Write-Host ""
    Write-Host "  This USB now boots via:" -ForegroundColor White
    Write-Host "    UEFI 64-bit  : EFI\BOOT\BOOTX64.EFI  (unchanged)" -ForegroundColor Gray
    Write-Host "    UEFI 32-bit  : EFI\BOOT\BOOTIA32.EFI  (added)" -ForegroundColor Gray
    Write-Host "    Legacy BIOS  : MBR -> SYSLINUX -> isolinux/  (added)" -ForegroundColor Gray
    Write-Host ""
}

#endregion

try {
    Main
}
catch {
    Write-Host ""
    Write-Step "Error: $_" "FAIL"
    Write-Host "       $($_.ScriptStackTrace)" -ForegroundColor DarkGray
    exit 1
}
