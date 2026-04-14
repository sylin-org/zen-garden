<#
.SYNOPSIS
    GUIDv7 minting + roster management for firefly device provisioning.

.DESCRIPTION
    Helper module used by NewFirefly.ps1 to implement the FIREFLY-0004
    provisioning ritual: mint a GUIDv7, stage it for upload to the
    device, and append a roster entry to the host-side inventory.

    GUIDv7 format per RFC 9562 §5.7:
        - first 48 bits = Unix timestamp (ms, big-endian)
        - next 4 bits   = version (0x7)
        - next 12 bits  = random
        - next 2 bits   = variant (0b10)
        - remaining 62  = random
#>

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

# Default location for the operator's firefly inventory.
$script:RosterDefaultPath = Join-Path $env:USERPROFILE ".zen-garden\firefly-roster.json"

function New-FireflyGuidV7 {
    <#
    .SYNOPSIS
        Mint a GUIDv7 (RFC 9562 §5.7).

    .DESCRIPTION
        Emits a v7 UUID string in the canonical dashed lowercase form.
        Upper bits carry the current Unix millisecond timestamp so
        GUIDs sort chronologically by mint time.
    #>
    [CmdletBinding()]
    param()

    $unixMs = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()

    # Allocate 16 random bytes, then overwrite the leading timestamp
    # bytes and set the version/variant bits in place.
    $bytes = New-Object byte[] 16
    $rng = [System.Security.Cryptography.RandomNumberGenerator]::Create()
    try { $rng.GetBytes($bytes) } finally { $rng.Dispose() }

    # 48-bit big-endian timestamp -> bytes 0..5
    $bytes[0] = [byte](($unixMs -shr 40) -band 0xFF)
    $bytes[1] = [byte](($unixMs -shr 32) -band 0xFF)
    $bytes[2] = [byte](($unixMs -shr 24) -band 0xFF)
    $bytes[3] = [byte](($unixMs -shr 16) -band 0xFF)
    $bytes[4] = [byte](($unixMs -shr 8)  -band 0xFF)
    $bytes[5] = [byte]( $unixMs          -band 0xFF)

    # Version: high nibble of byte 6 = 0x7
    $bytes[6] = [byte](($bytes[6] -band 0x0F) -bor 0x70)
    # Variant: top two bits of byte 8 = 0b10
    $bytes[8] = [byte](($bytes[8] -band 0x3F) -bor 0x80)

    '{0:x2}{1:x2}{2:x2}{3:x2}-{4:x2}{5:x2}-{6:x2}{7:x2}-{8:x2}{9:x2}-{10:x2}{11:x2}{12:x2}{13:x2}{14:x2}{15:x2}' -f `
        $bytes[0], $bytes[1], $bytes[2],  $bytes[3],  $bytes[4],  $bytes[5],  $bytes[6],  $bytes[7],
        $bytes[8], $bytes[9], $bytes[10], $bytes[11], $bytes[12], $bytes[13], $bytes[14], $bytes[15]
}

function Save-FireflyDeviceIdFile {
    <#
    .SYNOPSIS
        Write `device_id.txt` to a staging directory ready for upload
        to the device filesystem.

    .PARAMETER DeviceId
        The GUIDv7 string.

    .PARAMETER StagingDir
        Directory to write into. Created if absent.

    .OUTPUTS
        The full path of the staged file.
    #>
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string]$DeviceId,
        [Parameter(Mandatory)][string]$StagingDir
    )

    if (-not (Test-Path $StagingDir)) {
        New-Item -ItemType Directory -Path $StagingDir -Force | Out-Null
    }

    $path = Join-Path $StagingDir "device_id.txt"
    # Plain UTF-8 without BOM — the firmware reads this as a single-line
    # ASCII token; a BOM breaks it.
    [System.IO.File]::WriteAllText($path, $DeviceId, [System.Text.UTF8Encoding]::new($false))
    $path
}

function Read-FireflyRoster {
    <#
    .SYNOPSIS
        Load the firefly roster file (returns a hashtable). If the
        file doesn't exist, returns a fresh empty roster.
    #>
    [CmdletBinding()]
    param(
        [string]$Path = $script:RosterDefaultPath
    )

    if (-not (Test-Path $Path)) {
        return @{
            version   = 1
            fireflies = @()
        }
    }

    try {
        $raw = Get-Content -Path $Path -Raw -Encoding UTF8
        $parsed = $raw | ConvertFrom-Json -AsHashtable
        if (-not $parsed.ContainsKey('version'))   { $parsed.version = 1 }
        if (-not $parsed.ContainsKey('fireflies')) { $parsed.fireflies = @() }
        return $parsed
    } catch {
        Write-Warning "Firefly roster at $Path is corrupt ($_); starting fresh."
        return @{ version = 1; fireflies = @() }
    }
}

function Write-FireflyRoster {
    <#
    .SYNOPSIS
        Persist the roster hashtable to disk via write-then-rename so
        a crash mid-write can't corrupt the file.
    #>
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][hashtable]$Roster,
        [string]$Path = $script:RosterDefaultPath
    )

    $parent = Split-Path -Parent $Path
    if (-not (Test-Path $parent)) {
        New-Item -ItemType Directory -Path $parent -Force | Out-Null
    }

    $json = $Roster | ConvertTo-Json -Depth 8
    $tmp = "$Path.tmp"
    [System.IO.File]::WriteAllText($tmp, $json, [System.Text.UTF8Encoding]::new($false))
    if (Test-Path $Path) { Remove-Item -Path $Path -Force }
    Move-Item -Path $tmp -Destination $Path
}

function Add-FireflyRosterEntry {
    <#
    .SYNOPSIS
        Append a provisioning entry to the roster and persist.

    .DESCRIPTION
        The entry records the mint timestamp, operator identity, human
        label, variant, firmware version, and optional stone
        assignment. Re-running NewFirefly against the same physical
        device mints a fresh GUID — a new entry is appended; older
        entries for that device remain as history.
    #>
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string]$DeviceId,
        [Parameter(Mandatory)][string]$Variant,
        [string]$Label,
        [string]$FirmwareVersionAtProvisioning,
        [string]$StoneAssignedTo,
        [string]$RosterPath = $script:RosterDefaultPath
    )

    $roster = Read-FireflyRoster -Path $RosterPath

    $entry = @{
        device_id  = $DeviceId
        minted_at  = (Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ssZ")
        minted_by  = "$env:USERNAME@$env:COMPUTERNAME"
        variant    = $Variant
    }
    if ($Label)                         { $entry.label = $Label }
    if ($FirmwareVersionAtProvisioning) { $entry.firmware_version_at_provisioning = $FirmwareVersionAtProvisioning }
    if ($StoneAssignedTo)               { $entry.stone_assigned_to = $StoneAssignedTo }

    $roster.fireflies = @($roster.fireflies) + @($entry)
    Write-FireflyRoster -Roster $roster -Path $RosterPath
}

Export-ModuleMember -Function `
    'New-FireflyGuidV7', `
    'Save-FireflyDeviceIdFile', `
    'Read-FireflyRoster', `
    'Write-FireflyRoster', `
    'Add-FireflyRosterEntry'
