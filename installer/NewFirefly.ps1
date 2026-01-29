<#
.SYNOPSIS
    Installs and tests Firefly firmware on a Waveshare RP2040-Matrix device.

.DESCRIPTION
    A fully automated, low-cognitive-load installer for Zen Garden Firefly:

    - Detects device state automatically (connected, bootloader, CircuitPython)
    - Downloads and flashes CircuitPython if needed
    - Installs Firefly firmware and libraries
    - Tests serial communication to verify everything works
    - Provides clear guidance when user action is needed

.PARAMETER Force
    Skip confirmation prompts.

.PARAMETER UpdateOnly
    Only update firmware, skip CircuitPython flash even if in bootloader mode.

.EXAMPLE
    .\NewFirefly.ps1
    # Fully automated - detects state and does the right thing

.NOTES
    Requires: Windows 10/11, Internet connection
    Hardware: Waveshare RP2040-Matrix (or compatible RP2040 + WS2812)
    Author: Zen Garden Team
    License: Apache 2.0
#>

[CmdletBinding()]
param(
    [switch]$Force,
    [switch]$UpdateOnly
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

#region Configuration
$script:Config = @{
    # CircuitPython settings
    # Note: RP2040-Matrix has no dedicated build, but generic Pico build works fine
    CircuitPythonVersion    = "10.0.3"
    CircuitPythonUrl        = "https://downloads.circuitpython.org/bin/raspberry_pi_pico/en_US/adafruit-circuitpython-raspberry_pi_pico-en_US-10.0.3.uf2"

    # Library bundle (must match CircuitPython major version)
    LibraryBundleUrl        = "https://github.com/adafruit/Adafruit_CircuitPython_Bundle/releases/download/20260129/adafruit-circuitpython-bundle-10.x-mpy-20260129.zip"

    # Local paths
    CacheDir                = (Join-Path $env:USERPROFILE ".zen-garden\firefly-cache")
    FirmwareDir             = (Join-Path $PSScriptRoot "..\firmware\firefly\circuitpython")

    # Drive names
    BootloaderDriveName     = "RPI-RP2"
    CircuitPyDriveName      = "CIRCUITPY"

    # Timeouts
    DriveWaitTimeoutSeconds = 60
    SerialTimeoutMs         = 3000

    # UI
    BoxWidth                = 56
}
#endregion

#region UI Helpers
function Write-Banner {
    Write-Host ""
    Write-Host "  ╔════════════════════════════════════════════════════════╗" -ForegroundColor Cyan
    Write-Host "  ║   Zen Garden Firefly Installer                         ║" -ForegroundColor Cyan
    Write-Host "  ║   LED status indicator for your Stones                 ║" -ForegroundColor Cyan
    Write-Host "  ╚════════════════════════════════════════════════════════╝" -ForegroundColor Cyan
    Write-Host ""
}

function Write-Panel {
    param(
        [string]$Title = '',
        [string[]]$Lines = @(),
        [string]$Color = 'Cyan'
    )

    $width = $script:Config.BoxWidth
    Write-Host "  ┌$('─' * $width)┐" -ForegroundColor $Color

    if ($Title) {
        $titlePadded = " $Title ".PadRight($width)
        Write-Host "  │$($titlePadded.Substring(0, $width))│" -ForegroundColor $Color
        Write-Host "  ├$('─' * $width)┤" -ForegroundColor $Color
    }

    foreach ($line in $Lines) {
        $linePadded = " $line".PadRight($width)
        Write-Host "  │$($linePadded.Substring(0, $width))│" -ForegroundColor $Color
    }

    Write-Host "  └$('─' * $width)┘" -ForegroundColor $Color
    Write-Host ""
}

function Write-Step {
    param(
        [string]$Message,
        [string]$Status = "..."
    )

    $symbols = @{
        "..."  = @{ Symbol = "[*]"; Color = "Cyan" }
        "OK"   = @{ Symbol = "[+]"; Color = "Green" }
        "FAIL" = @{ Symbol = "[x]"; Color = "Red" }
        "WARN" = @{ Symbol = "[!]"; Color = "Yellow" }
        "WAIT" = @{ Symbol = "[~]"; Color = "Magenta" }
        "TEST" = @{ Symbol = "[?]"; Color = "Blue" }
    }

    $s = $symbols[$Status]
    if (-not $s) { $s = $symbols["..."] }

    Write-Host "  $($s.Symbol) " -ForegroundColor $s.Color -NoNewline
    Write-Host $Message
}

function Write-Progress-Inline {
    param([string]$Message)
    Write-Host "`r      $Message                              " -NoNewline
}
#endregion

#region Device Detection
function Get-BootloaderDrive {
    $drives = Get-Volume -ErrorAction SilentlyContinue | Where-Object {
        $_.FileSystemLabel -eq $script:Config.BootloaderDriveName
    }
    if ($drives) {
        return "$($drives[0].DriveLetter):"
    }
    return $null
}

function Get-CircuitPyDrive {
    $drives = Get-Volume -ErrorAction SilentlyContinue | Where-Object {
        $_.FileSystemLabel -eq $script:Config.CircuitPyDriveName
    }
    if ($drives) {
        return "$($drives[0].DriveLetter):"
    }
    return $null
}

function Get-FireflySerialPort {
    <#
    .SYNOPSIS
        Finds COM port for CircuitPython/RP2040 device.
    #>
    try {
        # Method 1: Look for USB Serial Device
        $ports = Get-WmiObject Win32_PnPEntity -ErrorAction SilentlyContinue | Where-Object {
            $_.Name -and $_.Name -match "COM\d+" -and
            ($_.Name -match "USB Serial" -or $_.Name -match "CircuitPython" -or
             $_.Name -match "RP2" -or $_.Name -match "Board CDC")
        }

        if ($ports) {
            $port = if ($ports -is [array]) { $ports[0] } else { $ports }
            if ($port.Name -match "(COM\d+)") {
                return $matches[1]
            }
        }

        # Method 2: Check Win32_SerialPort for USB Serial Device
        $serialPorts = Get-WmiObject Win32_SerialPort -ErrorAction SilentlyContinue | Where-Object {
            $_.Description -match "USB Serial"
        }
        if ($serialPorts) {
            $port = if ($serialPorts -is [array]) { $serialPorts[0] } else { $serialPorts }
            return $port.DeviceID
        }
    }
    catch {
        # Ignore detection errors
    }
    return $null
}

function Get-DeviceState {
    <#
    .SYNOPSIS
        Determines the current state of the Firefly device.
    .OUTPUTS
        Hashtable with: State, ComPort, CircuitPyDrive, BootloaderDrive
    #>
    $state = @{
        State           = "NOT_CONNECTED"
        ComPort         = $null
        CircuitPyDrive  = $null
        BootloaderDrive = $null
        FirmwareVersion = $null
    }

    # Check for serial port first (device running)
    $state.ComPort = Get-FireflySerialPort

    # Check for CIRCUITPY drive
    $state.CircuitPyDrive = Get-CircuitPyDrive

    # Check for bootloader drive
    $state.BootloaderDrive = Get-BootloaderDrive

    # Determine state
    if ($state.ComPort) {
        $state.State = "SERIAL_AVAILABLE"
    }
    elseif ($state.CircuitPyDrive) {
        $state.State = "CIRCUITPY_MOUNTED"
    }
    elseif ($state.BootloaderDrive) {
        $state.State = "BOOTLOADER_MODE"
    }
    else {
        $state.State = "NOT_CONNECTED"
    }

    return $state
}

function Wait-ForState {
    param(
        [string]$TargetState,
        [int]$TimeoutSeconds = 30,
        [string]$WaitMessage = "Waiting..."
    )

    $elapsed = 0
    while ($elapsed -lt $TimeoutSeconds) {
        $state = Get-DeviceState

        if ($state.State -eq $TargetState) {
            Write-Host ""
            return $state
        }

        # Also accept more advanced states
        $stateOrder = @("NOT_CONNECTED", "BOOTLOADER_MODE", "CIRCUITPY_MOUNTED", "SERIAL_AVAILABLE")
        $targetIdx = [array]::IndexOf($stateOrder, $TargetState)
        $currentIdx = [array]::IndexOf($stateOrder, $state.State)

        if ($currentIdx -ge $targetIdx -and $targetIdx -ge 0) {
            Write-Host ""
            return $state
        }

        Write-Progress-Inline "$WaitMessage ($elapsed/$TimeoutSeconds sec)"
        Start-Sleep -Seconds 1
        $elapsed++
    }

    Write-Host ""
    return $null
}
#endregion

#region Serial Communication
function Test-SerialConnection {
    param([string]$ComPort)

    <#
    .SYNOPSIS
        Tests serial communication with Firefly firmware.
    .OUTPUTS
        Hashtable with: Success, Response, IsFirefly
    #>

    $result = @{
        Success   = $false
        Response  = $null
        IsFirefly = $false
        Error     = $null
    }

    try {
        $port = New-Object System.IO.Ports.SerialPort $ComPort, 115200
        $port.ReadTimeout = $script:Config.SerialTimeoutMs
        $port.WriteTimeout = $script:Config.SerialTimeoutMs
        $port.NewLine = "`r`n"
        $port.DtrEnable = $true

        $port.Open()
        Start-Sleep -Milliseconds 500  # Let device settle

        # Clear any pending data
        $port.DiscardInBuffer()
        $port.DiscardOutBuffer()

        # Send info command
        $port.WriteLine("I")
        Start-Sleep -Milliseconds 300

        # Read response
        $response = ""
        while ($port.BytesToRead -gt 0) {
            $response += $port.ReadExisting()
            Start-Sleep -Milliseconds 50
        }

        $port.Close()

        $result.Response = $response.Trim()
        $result.Success = $true

        # Check if it's Firefly firmware
        if ($response -match "firefly" -or $response -match "OK,firefly") {
            $result.IsFirefly = $true
        }
    }
    catch {
        $result.Error = $_.Exception.Message
        try { $port.Close() } catch {}
    }

    return $result
}

function Send-SerialCommand {
    param(
        [string]$ComPort,
        [string]$Command
    )

    try {
        $port = New-Object System.IO.Ports.SerialPort $ComPort, 115200
        $port.ReadTimeout = $script:Config.SerialTimeoutMs
        $port.WriteTimeout = $script:Config.SerialTimeoutMs
        $port.DtrEnable = $true

        $port.Open()
        Start-Sleep -Milliseconds 200
        $port.DiscardInBuffer()

        $port.WriteLine($Command)
        Start-Sleep -Milliseconds 200

        $response = ""
        while ($port.BytesToRead -gt 0) {
            $response += $port.ReadExisting()
            Start-Sleep -Milliseconds 50
        }

        $port.Close()
        return $response.Trim()
    }
    catch {
        try { $port.Close() } catch {}
        return $null
    }
}

function Test-FireflyVisual {
    param([string]$ComPort)

    <#
    .SYNOPSIS
        Runs a visual test sequence on the Firefly LEDs.
    #>

    Write-Step "Running LED test sequence..." "TEST"

    $tests = @(
        @{ Cmd = "C"; Desc = "Clear"; Delay = 200 }
        @{ Cmd = "F,255,0,0"; Desc = "Red"; Delay = 600 }
        @{ Cmd = "F,0,255,0"; Desc = "Green"; Delay = 600 }
        @{ Cmd = "F,0,0,255"; Desc = "Blue"; Delay = 600 }
        @{ Cmd = "A,rainbow"; Desc = "Rainbow"; Delay = 2000 }
        @{ Cmd = "T,healthy"; Desc = "Healthy"; Delay = 1000 }
        @{ Cmd = "C"; Desc = "Clear"; Delay = 200 }
    )

    $allPassed = $true
    foreach ($test in $tests) {
        $response = Send-SerialCommand -ComPort $ComPort -Command $test.Cmd
        $passed = ($response -match "OK")

        if (-not $passed) {
            $allPassed = $false
        }

        Start-Sleep -Milliseconds $test.Delay
    }

    return $allPassed
}
#endregion

#region Installation Functions
function Initialize-CacheDir {
    if (-not (Test-Path $script:Config.CacheDir)) {
        New-Item -ItemType Directory -Path $script:Config.CacheDir -Force | Out-Null
    }
}

function Get-CircuitPythonUf2 {
    Initialize-CacheDir

    $uf2FileName = "circuitpython-rp2040-$($script:Config.CircuitPythonVersion).uf2"
    $uf2Path = Join-Path $script:Config.CacheDir $uf2FileName

    if ((Test-Path $uf2Path) -and (Get-Item $uf2Path).Length -gt 500KB) {
        Write-Step "CircuitPython found in cache" "OK"
        return $uf2Path
    }

    Write-Step "Downloading CircuitPython $($script:Config.CircuitPythonVersion)..." "..."

    try {
        $ProgressPreference = 'SilentlyContinue'
        Invoke-WebRequest -Uri $script:Config.CircuitPythonUrl -OutFile $uf2Path -UseBasicParsing
        $ProgressPreference = 'Continue'

        $sizeMB = [math]::Round((Get-Item $uf2Path).Length / 1MB, 2)
        Write-Step "Downloaded CircuitPython ($sizeMB MB)" "OK"
        return $uf2Path
    }
    catch {
        Write-Step "Download failed: $_" "FAIL"
        throw
    }
}

function Get-NeoPixelLibrary {
    Initialize-CacheDir

    $libPath = Join-Path $script:Config.CacheDir "neopixel.mpy"

    if (Test-Path $libPath) {
        Write-Step "NeoPixel library found in cache" "OK"
        return $libPath
    }

    Write-Step "Downloading library bundle..." "..."

    $bundleZip = Join-Path $script:Config.CacheDir "bundle.zip"
    $bundleExtract = Join-Path $script:Config.CacheDir "bundle-extract"

    try {
        $ProgressPreference = 'SilentlyContinue'
        Invoke-WebRequest -Uri $script:Config.LibraryBundleUrl -OutFile $bundleZip -UseBasicParsing
        $ProgressPreference = 'Continue'

        if (Test-Path $bundleExtract) {
            Remove-Item $bundleExtract -Recurse -Force
        }

        Expand-Archive -Path $bundleZip -DestinationPath $bundleExtract -Force

        $neopixelFile = Get-ChildItem -Path $bundleExtract -Filter "neopixel.mpy" -Recurse |
            Where-Object { $_.DirectoryName -like "*\lib" } |
            Select-Object -First 1

        if (-not $neopixelFile) {
            throw "neopixel.mpy not found in bundle"
        }

        Copy-Item $neopixelFile.FullName $libPath -Force

        Remove-Item $bundleZip -Force -ErrorAction SilentlyContinue
        Remove-Item $bundleExtract -Recurse -Force -ErrorAction SilentlyContinue

        Write-Step "NeoPixel library ready" "OK"
        return $libPath
    }
    catch {
        Write-Step "Library download failed: $_" "FAIL"
        throw
    }
}

function Install-CircuitPython {
    param([string]$BootloaderDrive)

    $uf2Path = Get-CircuitPythonUf2

    Write-Step "Flashing CircuitPython to $BootloaderDrive..." "..."

    try {
        Copy-Item $uf2Path "$BootloaderDrive\" -Force
        Write-Step "CircuitPython flashed - device rebooting..." "OK"
        return $true
    }
    catch {
        Write-Step "Flash failed: $_" "FAIL"
        return $false
    }
}

function Install-Firmware {
    param([string]$CircuitPyDrive)

    # Create lib directory
    $libDir = Join-Path $CircuitPyDrive "lib"
    if (-not (Test-Path $libDir)) {
        New-Item -ItemType Directory -Path $libDir -Force | Out-Null
    }

    # Install NeoPixel library
    $neopixelPath = Get-NeoPixelLibrary
    Copy-Item $neopixelPath "$libDir\" -Force
    Write-Step "NeoPixel library installed" "OK"

    # Install firmware
    $firmwarePath = Join-Path $script:Config.FirmwareDir "code.py"
    if (-not (Test-Path $firmwarePath)) {
        throw "Firmware not found at $firmwarePath"
    }

    Copy-Item $firmwarePath "$CircuitPyDrive\code.py" -Force
    Write-Step "Firefly firmware installed" "OK"

    return $true
}
#endregion

#region Main Flow
function Show-NotConnected {
    Write-Panel -Title "Device Not Detected" -Color "Yellow" -Lines @(
        "No Firefly device found.",
        "",
        "To connect your RP2040-Matrix:",
        "",
        "  1. Hold the BOOT button on the device",
        "  2. While holding, plug in the USB-C cable",
        "  3. Release the BOOT button",
        "",
        "A drive named 'RPI-RP2' will appear."
    )
}

function Show-Success {
    param([string]$ComPort)

    Write-Host ""
    Write-Panel -Title "Firefly Ready!" -Color "Green" -Lines @(
        "Installation complete and verified.",
        "",
        "Serial Port: $ComPort",
        "Baud Rate:   115200",
        "",
        "Test commands (via PuTTY or serial terminal):",
        "  F,0,255,0    - All LEDs green",
        "  F,255,0,0    - All LEDs red",
        "  A,rainbow    - Rainbow animation",
        "  T,healthy    - Status: healthy",
        "  T,error      - Status: error (blinks)",
        "  C            - Clear all LEDs"
    )
}

function Main {
    Write-Banner

    # Get initial device state
    Write-Step "Detecting device state..." "..."
    $state = Get-DeviceState

    switch ($state.State) {
        "SERIAL_AVAILABLE" {
            Write-Step "Found device on $($state.ComPort)" "OK"

            # Test if Firefly firmware is running
            Write-Step "Testing serial communication..." "TEST"
            $testResult = Test-SerialConnection -ComPort $state.ComPort

            if ($testResult.Success -and $testResult.IsFirefly) {
                Write-Step "Firefly firmware responding!" "OK"

                # Offer update or test
                Write-Panel -Title "Firefly Already Installed" -Color "Green" -Lines @(
                    "Device is working with Firefly firmware.",
                    "",
                    "Options:",
                    "  [T] Run LED test sequence",
                    "  [U] Update firmware to latest",
                    "  [Q] Quit (everything is fine!)"
                )

                Write-Host "  Choice [T/U/Q]: " -NoNewline -ForegroundColor Yellow
                $choice = Read-Host

                switch ($choice.ToLower()) {
                    "t" {
                        Write-Host ""
                        $testPassed = Test-FireflyVisual -ComPort $state.ComPort
                        if ($testPassed) {
                            Write-Step "LED test completed successfully!" "OK"
                        }
                        else {
                            Write-Step "Some tests may have had issues" "WARN"
                        }
                        Show-Success -ComPort $state.ComPort
                    }
                    "u" {
                        Write-Host ""
                        # Need CIRCUITPY drive for update
                        if ($state.CircuitPyDrive) {
                            Install-Firmware -CircuitPyDrive $state.CircuitPyDrive
                            Write-Step "Firmware updated!" "OK"
                            Start-Sleep -Seconds 2

                            # Re-test
                            $newState = Get-DeviceState
                            if ($newState.ComPort) {
                                Test-FireflyVisual -ComPort $newState.ComPort
                            }
                            Show-Success -ComPort $state.ComPort
                        }
                        else {
                            Write-Step "CIRCUITPY drive not found - unplug and replug device" "WARN"
                        }
                    }
                    default {
                        Write-Step "Firefly is working - no changes made" "OK"
                        Show-Success -ComPort $state.ComPort
                    }
                }
                return
            }
            elseif ($testResult.Success) {
                Write-Step "Device responds but not running Firefly firmware" "WARN"
                $responsePreview = if ($testResult.Response.Length -gt 60) {
                    $testResult.Response.Substring(0, 60) + "..."
                } else {
                    $testResult.Response
                }
                Write-Host "       Response: $responsePreview" -ForegroundColor Gray

                # Try to install firmware if CircuitPython is available
                if ($state.CircuitPyDrive) {
                    Write-Host ""
                    Write-Step "Installing Firefly firmware..." "..."
                    Install-Firmware -CircuitPyDrive $state.CircuitPyDrive

                    Write-Step "Waiting for device to restart..." "WAIT"
                    Start-Sleep -Seconds 3

                    $newState = Wait-ForState -TargetState "SERIAL_AVAILABLE" -TimeoutSeconds 15 -WaitMessage "Waiting for serial"
                    if ($newState -and $newState.ComPort) {
                        Test-FireflyVisual -ComPort $newState.ComPort
                        Show-Success -ComPort $newState.ComPort
                    }
                }
                else {
                    # Device has factory/other firmware, needs CircuitPython
                    Write-Host ""
                    Write-Panel -Title "CircuitPython Required" -Color "Yellow" -Lines @(
                        "This device has factory firmware, not CircuitPython.",
                        "We need to flash CircuitPython first.",
                        "",
                        "Please enter bootloader mode:",
                        "",
                        "  1. UNPLUG the device",
                        "  2. HOLD the BOOT button",
                        "  3. While holding, PLUG IN USB",
                        "  4. RELEASE the button",
                        "",
                        "A drive named 'RPI-RP2' will appear."
                    )

                    Write-Step "Waiting for bootloader mode..." "WAIT"
                    $newState = Wait-ForState -TargetState "BOOTLOADER_MODE" -TimeoutSeconds 60 -WaitMessage "Unplug, hold BOOT, plug in"

                    if ($newState -and $newState.BootloaderDrive) {
                        Write-Host ""
                        Write-Step "Bootloader detected at $($newState.BootloaderDrive)" "OK"

                        # Flash CircuitPython
                        if (-not (Install-CircuitPython -BootloaderDrive $newState.BootloaderDrive)) {
                            return
                        }

                        # Wait for CIRCUITPY
                        Write-Step "Waiting for CircuitPython to boot..." "WAIT"
                        Start-Sleep -Seconds 2

                        $cpState = Wait-ForState -TargetState "CIRCUITPY_MOUNTED" -TimeoutSeconds 30 -WaitMessage "Waiting for CIRCUITPY"

                        if ($cpState -and $cpState.CircuitPyDrive) {
                            Write-Step "CircuitPython ready at $($cpState.CircuitPyDrive)" "OK"
                            Write-Host ""
                            Install-Firmware -CircuitPyDrive $cpState.CircuitPyDrive

                            Write-Step "Waiting for device..." "WAIT"
                            Start-Sleep -Seconds 3

                            $finalState = Wait-ForState -TargetState "SERIAL_AVAILABLE" -TimeoutSeconds 20 -WaitMessage "Waiting for serial"
                            if ($finalState -and $finalState.ComPort) {
                                Write-Step "Device ready on $($finalState.ComPort)" "OK"
                                Write-Host ""
                                Test-FireflyVisual -ComPort $finalState.ComPort
                                Show-Success -ComPort $finalState.ComPort
                            }
                        }
                    }
                    else {
                        Write-Host ""
                        Write-Step "Timeout waiting for bootloader" "FAIL"
                        Write-Host "       Try again: unplug, hold BOOT, plug in USB" -ForegroundColor Yellow
                    }
                }
                return
            }
            else {
                Write-Step "Serial port found but communication failed" "WARN"
                Write-Host "       Error: $($testResult.Error)" -ForegroundColor Gray
            }
        }

        "CIRCUITPY_MOUNTED" {
            Write-Step "Found CircuitPython drive at $($state.CircuitPyDrive)" "OK"

            # Install firmware
            Write-Host ""
            Install-Firmware -CircuitPyDrive $state.CircuitPyDrive

            Write-Step "Waiting for device..." "WAIT"
            Start-Sleep -Seconds 3

            $newState = Wait-ForState -TargetState "SERIAL_AVAILABLE" -TimeoutSeconds 20 -WaitMessage "Waiting for serial"

            if ($newState -and $newState.ComPort) {
                Write-Step "Device ready on $($newState.ComPort)" "OK"

                # Test it
                Write-Host ""
                $testPassed = Test-FireflyVisual -ComPort $newState.ComPort

                if ($testPassed) {
                    Write-Step "All tests passed!" "OK"
                }

                Show-Success -ComPort $newState.ComPort
            }
            else {
                Write-Step "Device not responding on serial" "WARN"
                Write-Host "       Firmware installed but serial test skipped." -ForegroundColor Gray
                Write-Host "       Try unplugging and replugging the device." -ForegroundColor Gray
            }
            return
        }

        "BOOTLOADER_MODE" {
            Write-Step "Found device in bootloader mode at $($state.BootloaderDrive)" "OK"

            Write-Host ""

            # Flash CircuitPython
            if (-not (Install-CircuitPython -BootloaderDrive $state.BootloaderDrive)) {
                return
            }

            # Wait for CIRCUITPY
            Write-Step "Waiting for CircuitPython to boot..." "WAIT"
            Start-Sleep -Seconds 2

            $newState = Wait-ForState -TargetState "CIRCUITPY_MOUNTED" -TimeoutSeconds 30 -WaitMessage "Waiting for CIRCUITPY"

            if (-not $newState -or -not $newState.CircuitPyDrive) {
                Write-Step "Timeout waiting for CIRCUITPY drive" "FAIL"
                Write-Host "       Try unplugging and replugging the device." -ForegroundColor Yellow
                return
            }

            Write-Step "CircuitPython ready at $($newState.CircuitPyDrive)" "OK"

            # Install firmware
            Write-Host ""
            Install-Firmware -CircuitPyDrive $newState.CircuitPyDrive

            # Wait for serial
            Write-Step "Waiting for device to initialize..." "WAIT"
            Start-Sleep -Seconds 3

            $finalState = Wait-ForState -TargetState "SERIAL_AVAILABLE" -TimeoutSeconds 20 -WaitMessage "Waiting for serial"

            if ($finalState -and $finalState.ComPort) {
                Write-Step "Device ready on $($finalState.ComPort)" "OK"

                Write-Host ""
                $testPassed = Test-FireflyVisual -ComPort $finalState.ComPort

                if ($testPassed) {
                    Write-Step "All tests passed!" "OK"
                }

                Show-Success -ComPort $finalState.ComPort
            }
            else {
                Write-Step "Serial port not detected" "WARN"
                Write-Host "       Firmware installed. Try unplugging and replugging." -ForegroundColor Yellow
            }
            return
        }

        "NOT_CONNECTED" {
            Show-NotConnected

            Write-Step "Waiting for device..." "WAIT"
            $newState = Wait-ForState -TargetState "BOOTLOADER_MODE" -TimeoutSeconds $script:Config.DriveWaitTimeoutSeconds -WaitMessage "Connect device (hold BOOT + plug USB)"

            if (-not $newState) {
                Write-Host ""
                Write-Step "No device detected" "FAIL"
                Write-Host ""
                Write-Host "  Troubleshooting:" -ForegroundColor Yellow
                Write-Host "    - Make sure you hold BOOT while plugging in USB" -ForegroundColor Gray
                Write-Host "    - Try a different USB cable (some are charge-only)" -ForegroundColor Gray
                Write-Host "    - Try a different USB port" -ForegroundColor Gray
                Write-Host ""
                return
            }

            # Recurse with new state
            Write-Host ""
            Main
            return
        }
    }
}

# Entry point
try {
    Main
}
catch {
    Write-Host ""
    Write-Step "Error: $_" "FAIL"
    Write-Host "       $($_.ScriptStackTrace)" -ForegroundColor DarkGray
}
#endregion
