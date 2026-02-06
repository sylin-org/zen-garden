<#
.SYNOPSIS
    Installs and tests Firefly firmware on supported devices.

.DESCRIPTION
    Device-agnostic installer for Zen Garden Firefly visual indicators.

    Supported devices:
    - Waveshare RP2040-Matrix: 5x5 RGB LED matrix (CircuitPython)
    - ESP8266 NodeMCU + OLED: 128x64 SSD1306 display (MicroPython)

    Auto-detects connected hardware and applies appropriate firmware.

.PARAMETER Force
    Skip confirmation prompts.

.EXAMPLE
    .\NewFirefly.ps1

.NOTES
    Requires: Windows 10/11, Python (for ESP8266), Internet connection
    Author: Zen Garden Team
#>

[CmdletBinding()]
param(
    [switch]$Force  # Skip confirmation prompts
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

#region Configuration
# Centralized config to avoid magic strings and make firmware/runtime updates easy.
$script:Config = @{
    CacheDir      = (Join-Path $env:USERPROFILE ".zen-garden\firefly-cache")
    FirmwareDir   = (Join-Path $PSScriptRoot "..\firmware\firefly")
    SerialTimeout = 3000
    BoxWidth      = 56

    RP2040 = @{
        CircuitPythonUrl  = "https://downloads.circuitpython.org/bin/raspberry_pi_pico/en_US/adafruit-circuitpython-raspberry_pi_pico-en_US-10.0.3.uf2"
        LibraryBundleUrl  = "https://github.com/adafruit/Adafruit_CircuitPython_Bundle/releases/download/20260129/adafruit-circuitpython-bundle-10.x-mpy-20260129.zip"
        FirmwareFile      = "circuitpython\code.py"
    }

    ESP8266 = @{
        MicroPythonUrl = "https://micropython.org/resources/firmware/ESP8266_GENERIC-20251209-v1.27.0.bin"
        I2C_SCL        = 12  # D6
        I2C_SDA        = 14  # D5
        # Resources to upload: @{Local="path"; Remote="filename"}
        # Using .mpy bytecode for smaller footprint (compiled with mpy-cross)
        Resources      = @(
            @{Local="micropython\boot.py"; Remote="boot.py"}
            @{Local="micropython\firefly_oled.mpy"; Remote="firefly_oled.mpy"}
            @{Local="micropython\main.py"; Remote="main.py"}
            @{Local="etc\esp8266\profont_10.mpy"; Remote="profont_10.mpy"}
        )
    }
}
#endregion

#region UI Helpers
function Write-Banner {
    Write-Host ""
    Write-Host "  ========================================================" -ForegroundColor Cyan
    Write-Host "     Zen Garden Firefly Installer                         " -ForegroundColor Cyan
    Write-Host "     Visual status indicators for your Stones             " -ForegroundColor Cyan
    Write-Host "  ========================================================" -ForegroundColor Cyan
    Write-Host ""
}

function Write-Panel {
    param([string]$Title = '', [string[]]$Lines = @(), [string]$Color = 'Cyan')
    $w = $script:Config.BoxWidth
    Write-Host "  +$('-' * $w)+" -ForegroundColor $Color
    if ($Title) {
        $t = " $Title ".PadRight($w)
        Write-Host "  |$($t.Substring(0, $w))|" -ForegroundColor $Color
        Write-Host "  +$('-' * $w)+" -ForegroundColor $Color
    }
    foreach ($line in $Lines) {
        $l = " $line".PadRight($w)
        Write-Host "  |$($l.Substring(0, $w))|" -ForegroundColor $Color
    }
    Write-Host "  +$('-' * $w)+" -ForegroundColor $Color
    Write-Host ""
}

function Write-Step {
    param([string]$Message, [string]$Status = "...")
    $sym = @{
        "..."  = @{ S = "[*]"; C = "Cyan" }
        "OK"   = @{ S = "[+]"; C = "Green" }
        "FAIL" = @{ S = "[x]"; C = "Red" }
        "WARN" = @{ S = "[!]"; C = "Yellow" }
        "WAIT" = @{ S = "[~]"; C = "Magenta" }
        "TEST" = @{ S = "[?]"; C = "Blue" }
    }
    $s = $sym[$Status]
    if (-not $s) { $s = $sym["..."] }
    Write-Host "  $($s.S) " -ForegroundColor $s.C -NoNewline
    Write-Host $Message
}

function Confirm-Action {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Message,
        [bool]$DefaultYes = $true
    )

    if ($Force) {
        Write-Step "$Message (auto-yes: -Force)" "OK"
        return $true
    }

    $suffix = if ($DefaultYes) { "[Y/n]" } else { "[y/N]" }
    $answer = Read-Host "  $Message $suffix"

    if ([string]::IsNullOrWhiteSpace($answer)) {
        return $DefaultYes
    }

    return @("y", "yes") -contains $answer.Trim().ToLowerInvariant()
}

function Initialize-Cache {
    if (-not (Test-Path $script:Config.CacheDir)) {
        New-Item -ItemType Directory -Path $script:Config.CacheDir -Force | Out-Null
    }
}
#endregion

#region Device Detection
# Detect via COM ports + drive labels (bootloader/CircuitPython).
function Get-ConnectedDevices {
    $devices = @()

    # COM ports from PnP devices (VID/PID/name heuristics).
    $ports = Get-WmiObject Win32_PnPEntity -ErrorAction SilentlyContinue |
        Where-Object { $_.Name -and $_.Name -match 'COM\d+' }

    foreach ($p in $ports) {
        $com = if ($p.Name -match '(COM\d+)') { $matches[1] } else { $null }
        $type = $null
        $vendorId = $null
        $productId = $null

        if ($p.DeviceID -match 'VID_([0-9A-F]{4})') { $vendorId = $matches[1] }
        if ($p.DeviceID -match 'PID_([0-9A-F]{4})') { $productId = $matches[1] }

        # VID/PID takes priority; fall back to friendly-name heuristics.
        if ($vendorId -eq "2E8A" -or $vendorId -eq "239A") { $type = "RP2040" }
        elseif ($vendorId -eq "1A86" -and $productId -eq "7523") { $type = "ESP8266" }
        elseif ($p.Name -match 'RP2|CircuitPython|Board CDC') { $type = "RP2040" }
        elseif ($p.Name -match 'CH340|CH34') { $type = "ESP8266" }

        if ($type -and $com) {
            $devices += @{ Type = $type; ComPort = $com; Name = $p.Name; VID = $vendorId; PID = $productId }
        }
    }

    # RP2040 bootloader exposes a mass-storage drive named RPI-RP2.
    $boot = Get-Volume -ErrorAction SilentlyContinue | Where-Object { $_.FileSystemLabel -eq "RPI-RP2" }
    if ($boot) {
        $devices += @{ Type = "RP2040"; BootloaderDrive = "$($boot.DriveLetter):"; Name = "RP2040 Bootloader" }
    }

    # CircuitPython drive for firmware + libraries.
    $cpy = Get-Volume -ErrorAction SilentlyContinue | Where-Object { $_.FileSystemLabel -eq "CIRCUITPY" }
    if ($cpy) {
        $existing = $devices | Where-Object { $_.Type -eq "RP2040" } | Select-Object -First 1
        if ($existing) { $existing.CircuitPyDrive = "$($cpy.DriveLetter):" }
        else { $devices += @{ Type = "RP2040"; CircuitPyDrive = "$($cpy.DriveLetter):"; Name = "CircuitPython Device" } }
    }

    return $devices
}
#endregion

#region RP2040 Handler
# CircuitPython runtime + NeoPixel library + Firefly firmware (code.py).
function Get-RP2040Runtime {
    Initialize-Cache
    $path = Join-Path $script:Config.CacheDir "circuitpython-rp2040.uf2"
    if ((Test-Path $path) -and (Get-Item $path).Length -gt 500KB) {
        Write-Step "CircuitPython found in cache" "OK"
        return $path
    }
    Write-Step "Downloading CircuitPython..." "..."
    $ProgressPreference = 'SilentlyContinue'
    # Cache downloads so repeat runs are fast/offline-friendly.
    Invoke-WebRequest -Uri $script:Config.RP2040.CircuitPythonUrl -OutFile $path -UseBasicParsing
    Write-Step "CircuitPython downloaded" "OK"
    return $path
}

function Get-RP2040Library {
    Initialize-Cache
    $path = Join-Path $script:Config.CacheDir "neopixel.mpy"
    if (Test-Path $path) {
        Write-Step "NeoPixel library found in cache" "OK"
        return $path
    }
    Write-Step "Downloading library bundle..." "..."
    $zip = Join-Path $script:Config.CacheDir "bundle.zip"
    $extract = Join-Path $script:Config.CacheDir "bundle-extract"
    $ProgressPreference = 'SilentlyContinue'
    Invoke-WebRequest -Uri $script:Config.RP2040.LibraryBundleUrl -OutFile $zip -UseBasicParsing
    if (Test-Path $extract) { Remove-Item $extract -Recurse -Force }
    Expand-Archive -Path $zip -DestinationPath $extract -Force
    $neopixel = Get-ChildItem -Path $extract -Filter "neopixel.mpy" -Recurse |
        Where-Object { $_.DirectoryName -like "*\lib" } | Select-Object -First 1
    Copy-Item $neopixel.FullName $path -Force
    Remove-Item $zip -Force -ErrorAction SilentlyContinue
    Remove-Item $extract -Recurse -Force -ErrorAction SilentlyContinue
    Write-Step "NeoPixel library ready" "OK"
    return $path
}

function Install-RP2040Runtime {
    param([string]$Drive)
    $uf2 = Get-RP2040Runtime
    Write-Step "Flashing CircuitPython to $Drive..." "..."
    # UF2 flashing is just a copy to the bootloader volume.
    Copy-Item $uf2 "$Drive\" -Force
    Write-Step "CircuitPython flashed" "OK"
}

function Install-RP2040Firmware {
    param([string]$Drive)
    $lib = Get-RP2040Library
    $libDir = Join-Path $Drive "lib"
    if (-not (Test-Path $libDir)) { New-Item -ItemType Directory -Path $libDir -Force | Out-Null }
    Copy-Item $lib "$libDir\" -Force
    Write-Step "NeoPixel library installed" "OK"

    $fw = Join-Path $script:Config.FirmwareDir $script:Config.RP2040.FirmwareFile
    Copy-Item $fw "$Drive\code.py" -Force
    Write-Step "Firefly firmware installed" "OK"
}

function Wait-RP2040Volume {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Label,
        [int]$TimeoutSec = 30
    )

    for ($i = 0; $i -lt $TimeoutSec; $i++) {
        $vol = Get-Volume -ErrorAction SilentlyContinue | Where-Object { $_.FileSystemLabel -eq $Label } | Select-Object -First 1
        if ($vol -and $vol.DriveLetter) {
            return "$($vol.DriveLetter):"
        }
        Start-Sleep -Seconds 1
    }

    return $null
}

function Refresh-RP2040ComPort {
    $rp = @(Get-ConnectedDevices | Where-Object { $_["Type"] -eq "RP2040" -and $_["ComPort"] }) | Select-Object -First 1
    if ($rp) { return $rp["ComPort"] }
    return $null
}

function Install-RP2040FromCurrentState {
    param([hashtable]$Device)

    $cpDrive = $Device["CircuitPyDrive"]

    if (-not $cpDrive) {
        $bootDrive = $Device["BootloaderDrive"]

        if (-not $bootDrive) {
            Write-Step "RP2040 bootloader drive (RPI-RP2) not detected" "WARN"
            Write-Host "       To install Firefly, put the RP2040 in BOOT mode (hold BOOT while reconnecting USB)." -ForegroundColor Gray

            if ($Force) {
                Write-Step "Waiting for BOOT mode drive..." "WAIT"
            } else {
                $next = Read-Host "       Press Enter when ready, or type 'skip' to cancel"
                if ($next -match '^\s*skip\s*$') {
                    Write-Step "Installation skipped by user" "WARN"
                    return $false
                }
            }

            $bootDrive = Wait-RP2040Volume -Label "RPI-RP2" -TimeoutSec 45
            if (-not $bootDrive) {
                Write-Step "Timed out waiting for RPI-RP2 drive" "FAIL"
                return $false
            }
            $Device["BootloaderDrive"] = $bootDrive
        }

        Write-Step "Bootloader mode at $bootDrive" "OK"
        Install-RP2040Runtime -Drive $bootDrive
        Write-Step "Waiting for CircuitPython..." "WAIT"
        $cpDrive = Wait-RP2040Volume -Label "CIRCUITPY" -TimeoutSec 45
        if (-not $cpDrive) {
            Write-Step "Timed out waiting for CIRCUITPY drive" "FAIL"
            return $false
        }
        $Device["CircuitPyDrive"] = $cpDrive
    }

    Write-Step "CircuitPython at $cpDrive" "OK"
    Install-RP2040Firmware -Drive $cpDrive
    Start-Sleep -Seconds 3

    $freshPort = Refresh-RP2040ComPort
    if ($freshPort) {
        $Device["ComPort"] = $freshPort
    }

    return $true
}

function Test-RP2040Connection {
    param([string]$Port)
    $serial = $null
    try {
        $serial = New-Object System.IO.Ports.SerialPort $Port, 115200
        $serial.ReadTimeout = $script:Config.SerialTimeout
        $serial.DtrEnable = $true
        $serial.Open()
        Start-Sleep -Milliseconds 500
        $serial.DiscardInBuffer()
        # "I" is a lightweight identity probe expected to contain "firefly".
        $serial.WriteLine("I")
        Start-Sleep -Milliseconds 300
        $response = ""
        while ($serial.BytesToRead -gt 0) {
            $response += $serial.ReadExisting()
            Start-Sleep -Milliseconds 50
        }
        $serial.Close()
        return @{ Success = $true; IsFirefly = ($response -match "firefly"); Response = $response }
    } catch {
        if ($serial) { try { $serial.Close() } catch {} }
        return @{ Success = $false; Error = $_.Exception.Message }
    }
}

function Invoke-RP2040VisualTest {
    param([string]$Port)
    Write-Step "Running LED test sequence..." "TEST"
    # Firefly command protocol: C=clear, F=r,g,b, A=animation, T=theme.
    $cmds = @("C", "F,255,0,0", "F,0,255,0", "F,0,0,255", "A,rainbow", "T,healthy", "C")
    $delays = @(200, 600, 600, 600, 2000, 1000, 200)
    for ($i = 0; $i -lt $cmds.Count; $i++) {
        $serial = $null
        try {
            $serial = New-Object System.IO.Ports.SerialPort $Port, 115200
            $serial.DtrEnable = $true
            $serial.Open()
            $serial.WriteLine($cmds[$i])
            Start-Sleep -Milliseconds $delays[$i]
            $serial.Close()
        } catch {
            if ($serial) { try { $serial.Close() } catch {} }
        }
    }
    Write-Step "LED test complete" "OK"
}

function Invoke-RP2040Handler {
    param($Device)
    Write-Step "Handling RP2040-Matrix..." "..."

    if ($Device["BootloaderDrive"]) {
        Write-Step "Bootloader mode at $($Device['BootloaderDrive'])" "OK"
        Install-RP2040Runtime -Drive $Device["BootloaderDrive"]
        Write-Step "Waiting for CircuitPython..." "WAIT"
        $drv = Wait-RP2040Volume -Label "CIRCUITPY" -TimeoutSec 30
        if ($drv) { $Device["CircuitPyDrive"] = $drv }
    }

    if ($Device["CircuitPyDrive"]) {
        Write-Step "CircuitPython at $($Device['CircuitPyDrive'])" "OK"
        Install-RP2040Firmware -Drive $Device["CircuitPyDrive"]
        Start-Sleep -Seconds 3
    }

    if ($Device["ComPort"]) {
        $test = Test-RP2040Connection -Port $Device["ComPort"]
        if ($test.Success -and $test.IsFirefly) {
            Write-Step "Firefly responding!" "OK"
            Invoke-RP2040VisualTest -Port $Device["ComPort"]
            Write-Panel -Title "RP2040-Matrix Firefly Ready!" -Color "Green" -Lines @(
                "Installation complete."
                ""
                "Device: Waveshare RP2040-Matrix"
                "Port: $($Device['ComPort'])"
                ""
                "Commands: F,r,g,b / A,rainbow / T,healthy / C"
            )
        } elseif ($test.Success) {
            Write-Step "Device responds but not Firefly firmware" "WARN"
            $shouldInstall = Confirm-Action -Message "Install Firefly firmware on this RP2040 now?"
            if ($shouldInstall) {
                $installed = Install-RP2040FromCurrentState -Device $Device
                if ($installed -and $Device["ComPort"]) {
                    $post = Test-RP2040Connection -Port $Device["ComPort"]
                    if ($post.Success -and $post.IsFirefly) {
                        Write-Step "Firefly responding after install!" "OK"
                        Invoke-RP2040VisualTest -Port $Device["ComPort"]
                        Write-Panel -Title "RP2040-Matrix Firefly Ready!" -Color "Green" -Lines @(
                            "Installation complete."
                            ""
                            "Device: Waveshare RP2040-Matrix"
                            "Port: $($Device['ComPort'])"
                            ""
                            "Commands: F,r,g,b / A,rainbow / T,healthy / C"
                        )
                    } else {
                        Write-Step "Installed, but Firefly did not respond on serial yet" "WARN"
                    }
                } elseif ($installed) {
                    Write-Step "Firmware installed. Reconnect and re-run to verify serial commands." "WARN"
                }
            }
        } else {
            Write-Step "Communication failed" "WARN"
        }
    }
}
#endregion

#region ESP8266 Handler
# MicroPython runtime + Firefly OLED resources (boot.py/main.py + .mpy libs).
function Get-ESP8266Runtime {
    Initialize-Cache
    $path = Join-Path $script:Config.CacheDir "micropython-esp8266.bin"
    if ((Test-Path $path) -and (Get-Item $path).Length -gt 500KB) {
        Write-Step "MicroPython found in cache" "OK"
        return $path
    }
    Write-Step "Downloading MicroPython..." "..."
    $ProgressPreference = 'SilentlyContinue'
    Invoke-WebRequest -Uri $script:Config.ESP8266.MicroPythonUrl -OutFile $path -UseBasicParsing
    Write-Step "MicroPython downloaded" "OK"
    return $path
}

function Test-ESP8266Esptool {
    try {
        $r = & python -m esptool version 2>&1
        return $r -match "esptool"
    } catch { return $false }
}

function Install-ESP8266Esptool {
    Write-Step "Installing esptool..." "..."
    & pip install esptool --user --quiet 2>&1 | Out-Null
    Write-Step "esptool installed" "OK"
}

function Install-ESP8266Runtime {
    param([string]$Port)
    if (-not (Test-ESP8266Esptool)) { Install-ESP8266Esptool }
    $bin = Get-ESP8266Runtime
    Write-Step "Erasing flash..." "..."
    & python -m esptool --port $Port erase_flash 2>&1 | Out-Null
    Write-Step "Flashing MicroPython..." "..."
    & python -m esptool --port $Port --baud 460800 write_flash --flash_size=detect 0 $bin 2>&1 | Out-Null
    Write-Step "MicroPython flashed" "OK"
}

function Send-ESP8266File {
    param([string]$Port, [string]$LocalPath, [string]$RemoteName)

    if (-not (Test-Path $LocalPath)) { return $false }

    $serial = $null
    # Send in chunks to avoid MicroPython REPL buffer overflow.
    $chunkSize = 512

    try {
        $serial = New-Object System.IO.Ports.SerialPort $Port, 115200
        $serial.ReadTimeout = 10000
        $serial.WriteTimeout = 5000
        $serial.Open()

        # Interrupt any running code
        $serial.Write([char]3)
        $serial.Write([char]3)
        Start-Sleep -Milliseconds 300
        $serial.DiscardInBuffer()

        # Open file for writing in binary mode
        $serial.Write("f=open('$RemoteName','wb')`r`n")
        Start-Sleep -Milliseconds 200
        $serial.DiscardInBuffer()

        # Read file as raw bytes (works for both text and binary .mpy files).
        $bytes = [System.IO.File]::ReadAllBytes($LocalPath)
        $b64 = [Convert]::ToBase64String($bytes)

        $serial.Write("import ubinascii`r`n")
        Start-Sleep -Milliseconds 100

        # Write chunks and drain buffer to avoid REPL backpressure.
        for ($i = 0; $i -lt $b64.Length; $i += $chunkSize) {
            $chunk = $b64.Substring($i, [Math]::Min($chunkSize, $b64.Length - $i))
            $serial.Write("f.write(ubinascii.a2b_base64('$chunk'))`r`n")
            Start-Sleep -Milliseconds 150
            # Drain buffer to prevent overflow
            while ($serial.BytesToRead -gt 0) { $null = $serial.ReadByte() }
        }

        # Close file and verify
        $serial.Write("f.close()`r`n")
        Start-Sleep -Milliseconds 200
        $serial.Write("import os; print('SIZE:', os.stat('$RemoteName')[6])`r`n")
        Start-Sleep -Milliseconds 300

        $resp = ''
        while ($serial.BytesToRead -gt 0) {
            $resp += [char]$serial.ReadByte()
        }

        $serial.Close()
        return ($resp -match 'SIZE:')
    } catch {
        if ($serial) { try { $serial.Close() } catch {} }
        return $false
    }
}

function Install-ESP8266Resources {
    param([string]$Port)

    $resources = $script:Config.ESP8266.Resources
    if (-not $resources) { return }

    foreach ($res in $resources) {
        $localPath = Join-Path $script:Config.FirmwareDir $res.Local
        $remoteName = $res.Remote

        if (Test-Path $localPath) {
            Write-Step "Uploading $remoteName..." "..."
            $ok = Send-ESP8266File -Port $Port -LocalPath $localPath -RemoteName $remoteName
            if ($ok) {
                Write-Step "$remoteName uploaded" "OK"
            } else {
                Write-Step "$remoteName failed" "WARN"
            }
        } else {
            Write-Step "$remoteName not found" "WARN"
        }
    }
}

function Test-ESP8266Connection {
    param([string]$Port)
    $serial = $null
    try {
        $serial = New-Object System.IO.Ports.SerialPort $Port, 115200
        $serial.ReadTimeout = $script:Config.SerialTimeout
        $serial.DtrEnable = $false
        $serial.Open()
        Start-Sleep -Milliseconds 500
        $serial.Write([char]3)
        Start-Sleep -Milliseconds 300
        $serial.DiscardInBuffer()
        $serial.Write("`r`n")
        Start-Sleep -Milliseconds 300
        $response = ""
        while ($serial.BytesToRead -gt 0) {
            $response += $serial.ReadExisting()
            Start-Sleep -Milliseconds 50
        }
        $serial.Close()
        return @{
            Success = $true
            IsMicroPython = ($response -match ">>>")
            IsFirefly = ($response -match "firefly")
            Response = $response
        }
    } catch {
        if ($serial) { try { $serial.Close() } catch {} }
        return @{ Success = $false; Error = $_.Exception.Message }
    }
}

function Invoke-ESP8266VisualTest {
    param([string]$Port)
    Write-Step "Testing OLED display..." "TEST"
    $serial = $null
    try {
        $serial = New-Object System.IO.Ports.SerialPort $Port, 115200
        $serial.ReadTimeout = 3000
        $serial.Open()
        $serial.DtrEnable = $true
        Start-Sleep -Milliseconds 100
        $serial.DtrEnable = $false
        Start-Sleep -Milliseconds 2000
        $serial.Write([char]3)
        Start-Sleep -Milliseconds 500
        $serial.DiscardInBuffer()

        # Enter paste mode (Ctrl+E) to send a multi-line script.
        $serial.Write([char]5)
        Start-Sleep -Milliseconds 200

        # Multi-line script with custom font
        $script = "from machine import Pin, SoftI2C`n"
        $script += "import ssd1306`n"
        $script += "import profont_10 as font`n"
        $script += "i2c = SoftI2C(scl=Pin(12), sda=Pin(14), freq=400000)`n"
        $script += "oled = ssd1306.SSD1306_I2C(128, 64, i2c)`n"
        $script += "oled.fill(0)`n"
        $script += "font.draw(oled, 'ZEN GARDEN', 14, 3)`n"
        $script += "font.draw(oled, 'FIREFLY', 38, 28)`n"
        $script += "font.draw(oled, 'Ready!', 42, 50)`n"
        $script += "oled.show()`n"

        $serial.Write($script)
        Start-Sleep -Milliseconds 100
        $serial.Write([char]4)  # Ctrl+D executes paste
        Start-Sleep -Milliseconds 1500

        $serial.Close()
        Write-Step "OLED test complete" "OK"
    } catch {
        if ($serial) { try { $serial.Close() } catch {} }
        Write-Step "OLED test failed" "WARN"
    }
}

function Reset-ESP8266 {
    param([string]$Port)
    $serial = $null
    try {
        $serial = New-Object System.IO.Ports.SerialPort $Port, 115200
        $serial.ReadTimeout = 5000
        $serial.Open()
        # Ensure REPL prompt
        $serial.Write([char]3)
        Start-Sleep -Milliseconds 200
        $serial.DiscardInBuffer()
        # Hard reset (more reliable than Ctrl+D after paste mode)
        $serial.Write("import machine`r`n")
        Start-Sleep -Milliseconds 200
        $serial.Write("machine.reset()`r`n")
        Start-Sleep -Milliseconds 200
        $serial.Close()
    } catch {
        if ($serial) { try { $serial.Close() } catch {} }
    }
}

function Invoke-ESP8266Handler {
    param($Device)
    Write-Step "Handling ESP8266-OLED..." "..."

    $port = $Device["ComPort"]
    if (-not $port) {
        Write-Step "No COM port found" "FAIL"
        return
    }

    # Always do a clean install: erase flash + re-flash MicroPython + upload resources.
    # This ensures no stale files and consistent behavior.
    Write-Step "Erasing and flashing MicroPython..." "..."
    Install-ESP8266Runtime -Port $port
    Write-Step "Waiting for reboot..." "WAIT"
    Start-Sleep -Seconds 3

    Install-ESP8266Resources -Port $port
    Invoke-ESP8266VisualTest -Port $port

    # Final reset to let main.py auto-start (visual test leaves device in REPL).
    Write-Step "Starting main.py..." "..."
    Reset-ESP8266 -Port $port
    Start-Sleep -Seconds 5

    Write-Panel -Title "ESP8266-OLED Firefly Ready!" -Color "Green" -Lines @(
        "Installation complete."
        ""
        "Device: NodeMCU ESP8266 + OLED"
        "Port: $port"
        "Display: SSD1306 128x64 I2C"
        ""
        "Firefly firmware is running!"
    )
}
#endregion

#region Main
# Single-device flow: detect, pick first supported device, install/test.
function Invoke-DeviceHandler {
    param($Device)

    switch ($Device["Type"]) {
        "RP2040"  { Invoke-RP2040Handler -Device $Device }
        "ESP8266" { Invoke-ESP8266Handler -Device $Device }
        default   { Write-Step "Unknown device type: $($Device['Type'])" "FAIL" }
    }
}

function Main {
    Write-Banner

    Write-Step "Scanning for devices..." "..."
    $devices = @(Get-ConnectedDevices)

    if ($devices.Count -eq 0 -or $null -eq $devices[0]) {
        Write-Step "No devices found" "WARN"
        Write-Panel -Title "No Device Detected" -Color "Yellow" -Lines @(
            "Supported devices:"
            ""
            "1. RP2040-Matrix: Hold BOOT + plug USB"
            "2. ESP8266-OLED: Just plug in USB"
        )
        return
    }

    # Filter to only valid devices with a Type
    $validDevices = @($devices | Where-Object { $_["Type"] })

    if ($validDevices.Count -eq 0) {
        Write-Step "No supported devices found" "WARN"
        return
    }

    Write-Step "Found $($validDevices.Count) device(s):" "OK"
    foreach ($d in $validDevices) {
        $info = if ($d["ComPort"]) { $d["ComPort"] } else { $d["BootloaderDrive"] }
        Write-Host "       - $($d['Type']): $info" -ForegroundColor Gray
    }
    Write-Host ""

    $device = $validDevices[0]
    Invoke-DeviceHandler -Device $device
}

try {
    Main
} catch {
    Write-Host ""
    Write-Step ("Error: " + $_.Exception.Message) "FAIL"
}
#endregion
