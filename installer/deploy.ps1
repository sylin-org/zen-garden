#!/usr/bin/env pwsh
<#
.SYNOPSIS
    Deploy Zen Garden packages to all discovered stones on the network.

.DESCRIPTION
    This script:
    1. Discovers all stones via UDP broadcast/multicast (port 7184)
    2. Deploys packages using HTTP API (/api/v1/stone/deploy)
    3. Waits for each stone to restart and come back online
    4. Reports success/failure for each stone

.PARAMETER Timeout
    Discovery timeout in seconds (default: 5)

.PARAMETER Sequential
    Deploy to stones sequentially instead of in parallel.
    Use for debugging to see detailed output per stone.

.PARAMETER Port
    Override the port number (default: 0 = use discovered port)

.PARAMETER Build
    Build binaries before deploying

.EXAMPLE
    .\deploy.ps1
    Deploy to all discovered stones using existing packages

.EXAMPLE
    .\deploy.ps1 -Build
    Build packages first, then deploy to all stones

.EXAMPLE
    .\deploy.ps1 -Sequential
    Deploy one stone at a time (for debugging)
#>

param(
    [int]$Timeout = 5,
    [switch]$Sequential,
    [int]$Port = 0,
    [switch]$Build
)

$ErrorActionPreference = "Stop"

# Build if requested
if ($Build) {
    Write-Host "`n Building packages..." -ForegroundColor Cyan
    $buildScript = Join-Path $PSScriptRoot "build.ps1"
    & $buildScript
    if ($LASTEXITCODE -ne 0) {
        Write-Host "X Build failed" -ForegroundColor Red
        exit 1
    }
    Write-Host ""
}

# Find packages
$distRoot = Resolve-Path "$PSScriptRoot/../dist"
$packagesDir = Join-Path $distRoot "packages"

$linuxX64Package = $null
$linuxX86Package = $null
$windowsX64Package = $null
$linuxArm64Package = $null

if (Test-Path $packagesDir) {
    $linuxX64Packages = Get-ChildItem $packagesDir -Filter "zen-garden-*-linux-x64.tar.gz" | Sort-Object LastWriteTime -Descending
    $linuxX86Packages = Get-ChildItem $packagesDir -Filter "zen-garden-*-linux-x86.tar.gz" | Sort-Object LastWriteTime -Descending
    $windowsX64Packages = Get-ChildItem $packagesDir -Filter "zen-garden-*-windows-x64.zip" | Sort-Object LastWriteTime -Descending
    # aarch64: prefer the fully-static musl package (runs on Android AND glibc ARM64 stones);
    # fall back to the glibc arm64 package if that's all that's built.
    $linuxArm64Packages = Get-ChildItem $packagesDir -Filter "zen-garden-*-linux-arm64-musl.tar.gz" | Sort-Object LastWriteTime -Descending
    if ($linuxArm64Packages.Count -eq 0) {
        $linuxArm64Packages = Get-ChildItem $packagesDir -Filter "zen-garden-*-linux-arm64.tar.gz" | Sort-Object LastWriteTime -Descending
    }
    if ($linuxX64Packages.Count -gt 0) { $linuxX64Package = $linuxX64Packages[0].FullName }
    if ($linuxX86Packages.Count -gt 0) { $linuxX86Package = $linuxX86Packages[0].FullName }
    if ($windowsX64Packages.Count -gt 0) { $windowsX64Package = $windowsX64Packages[0].FullName }
    if ($linuxArm64Packages.Count -gt 0) { $linuxArm64Package = $linuxArm64Packages[0].FullName }
}

if (-not $linuxX64Package -and -not $linuxX86Package -and -not $windowsX64Package -and -not $linuxArm64Package) {
    Write-Host "!  No packages found in $packagesDir" -ForegroundColor Yellow
    Write-Host "   Run with -Build flag or run build.ps1 first." -ForegroundColor Yellow
    exit 1
}

function Write-Status {
    param([string]$Message, [string]$Type = "Info")
    $color = switch ($Type) {
        "Success" { "Green" }
        "Error" { "Red" }
        "Warning" { "Yellow" }
        default { "White" }
    }
    Write-Host $Message -ForegroundColor $color
}

function Get-LanBindAddress {
    $candidates = @()
    $adapters = Get-NetIPAddress -AddressFamily IPv4 -ErrorAction SilentlyContinue

    foreach ($adapter in $adapters) {
        $ip = $adapter.IPAddress
        $octets = $ip.Split('.')
        if ($octets.Count -ne 4) { continue }

        $first = [int]$octets[0]
        $second = [int]$octets[1]
        $third = [int]$octets[2]

        # Skip loopback and link-local
        if ($first -eq 127 -or $first -eq 169) { continue }

        # Skip Docker Desktop ranges (172.17.x.x, 172.24-31.x.x)
        if ($first -eq 172 -and ($second -eq 17 -or $second -ge 24)) { continue }

        # Skip virtual adapter ranges in 192.168.x.x:
        # - 192.168.224.x: Hyper-V Default Switch (NAT)
        # - 192.168.240.x: WSL NAT
        # - 192.168.48.x, 192.168.64.x: Docker Desktop
        if ($first -eq 192 -and $second -eq 168) {
            if ($third -ge 48 -and $third -le 64) { continue }   # Docker Desktop
            if ($third -ge 224) { continue }                      # Hyper-V, WSL (224-255)
        }

        # Prioritize typical LAN ranges (lower third octet = more likely physical LAN)
        $priority = switch ($first) {
            192 {
                if ($second -eq 168) {
                    # 192.168.0-15 = priority 1 (typical home/office)
                    # 192.168.16-47 = priority 2
                    # 192.168.65-223 = priority 3
                    if ($third -le 15) { 1 }
                    elseif ($third -le 47) { 2 }
                    else { 3 }
                } else { 5 }
            }
            10 { 2 }
            172 { if ($second -ge 16 -and $second -le 23) { 3 } else { 5 } }
            default { 5 }
        }

        $candidates += [PSCustomObject]@{ Priority = $priority; IP = $ip }
    }

    $sorted = @($candidates | Sort-Object Priority)
    if ($sorted.Count -gt 0) { return $sorted[0].IP }
    return $null
}

function Find-AllStones {
    param([int]$TimeoutSeconds)

    Write-Status " Discovering stones on network (timeout: ${TimeoutSeconds}s)..."

    $lanIP = Get-LanBindAddress
    if ($lanIP) {
        Write-Status "   Binding to LAN interface: $lanIP"
        $localEndpoint = New-Object System.Net.IPEndPoint([System.Net.IPAddress]::Parse($lanIP), 7184)
        $udpClient = New-Object System.Net.Sockets.UdpClient $localEndpoint
    } else {
        Write-Status "   No LAN interface found, using default binding" -Type "Warning"
        $udpClient = New-Object System.Net.Sockets.UdpClient 7184
    }

    $udpClient.EnableBroadcast = $true

    $requestId = [guid]::NewGuid().ToString()
    $announcement = @{
        type = "discovery_request"
        data = @{
            discover = "moss"
            request_id = $requestId
            requester = "deploy"
        }
    } | ConvertTo-Json -Compress

    $requestBytes = [System.Text.Encoding]::UTF8.GetBytes($announcement)

    $multicastEndpoint = New-Object System.Net.IPEndPoint([System.Net.IPAddress]::Parse("239.255.42.99"), 7184)
    $broadcastEndpoint = New-Object System.Net.IPEndPoint([System.Net.IPAddress]::Broadcast, 7184)

    $sent1 = $udpClient.Send($requestBytes, $requestBytes.Length, $multicastEndpoint)
    $sent2 = $udpClient.Send($requestBytes, $requestBytes.Length, $broadcastEndpoint)

    Write-Status "   Sent discovery: multicast $sent1 bytes + broadcast $sent2 bytes"

    [System.Collections.ArrayList]$stones = @()
    $seenStones = @{}
    $remoteEP = New-Object System.Net.IPEndPoint([System.Net.IPAddress]::Any, 0)
    $udpClient.Client.ReceiveTimeout = 1000

    $startTime = Get-Date
    while (((Get-Date) - $startTime).TotalSeconds -lt $TimeoutSeconds) {
        try {
            $responseBytes = $udpClient.Receive([ref]$remoteEP)
            $responseJson = [System.Text.Encoding]::UTF8.GetString($responseBytes)
            $envelope = $responseJson | ConvertFrom-Json

            if ($envelope.type -eq "discovery_response") {
                $response = $envelope.data
                if ($seenStones.ContainsKey($response.stone_name)) { continue }
                $seenStones[$response.stone_name] = $true

                # PeerAddress is { ip, port, tls_port? } - build endpoint URL
                $addr = $response.address
                $stoneIP = if ($addr.ip) { $addr.ip } else { $remoteEP.Address.ToString() }
                $stonePort = if ($Port -gt 0) { $Port } elseif ($addr.port) { $addr.port } else { 7185 }
                $endpoint = "http://${stoneIP}:${stonePort}"

                $stones.Add([PSCustomObject]@{
                    Name = $response.stone_name
                    Endpoint = $endpoint
                    Address = $remoteEP.Address.ToString()
                }) | Out-Null
                Write-Status "   OK Found: $($response.stone_name) at $endpoint" -Type "Success"
            }
        }
        catch [System.Net.Sockets.SocketException] { continue }
        catch { Write-Status "   Warning: Failed to parse response: $_" -Type "Warning" }
    }

    $udpClient.Close()
    Write-Status "   Discovery complete: Found $($stones.Count) stone(s)" -Type "Success"
    return $stones
}

function Resolve-StoneEndpoint {
    param([PSCustomObject]$Stone)

    $endpoint = $Stone.Endpoint
    if ($endpoint -match "127\.0\.0\.1|localhost") {
        Write-Status "   !  $($Stone.Name) reports loopback, using UDP source address" -Type "Warning"
        $port = "7185"
        if ($endpoint -match ":(\d+)") { $port = $Matches[1] }
        if ($Stone.Address -and $Stone.Address -ne "127.0.0.1") {
            return "http://$($Stone.Address):$port"
        }
    }
    return $endpoint
}

function Get-StoneInfo {
    param([PSCustomObject]$Stone, [int]$TimeoutSec = 10)

    $resolvedEndpoint = Resolve-StoneEndpoint -Stone $Stone

    try {
        $url = "$($resolvedEndpoint.TrimEnd('/'))/health"
        $health = Invoke-RestMethod -Uri $url -Method Get -TimeoutSec $TimeoutSec
        return [PSCustomObject]@{
            OS = if ($health.os) { $health.os } else { "linux" }
            Architecture = if ($health.architecture) { $health.architecture } else { "unknown" }
            ResolvedEndpoint = $resolvedEndpoint
            Reachable = $true
        }
    }
    catch {
        return [PSCustomObject]@{
            OS = "unknown"
            Architecture = "unknown"
            ResolvedEndpoint = $resolvedEndpoint
            Reachable = $false
        }
    }
}

function Deploy-PackageToStone {
    param(
        [PSCustomObject]$Stone,
        [string]$PackagePath,
        [string]$Platform
    )

    Write-Status "`n Deploying package to $($Stone.Name) ($Platform)..."

    $url = "$($Stone.Endpoint.TrimEnd('/'))/api/v1/stone/deploy"
    $packageName = Split-Path -Leaf $PackagePath

    Write-Status "   Computing checksum..."
    $hash = (Get-FileHash $PackagePath -Algorithm SHA256).Hash.ToLower()

    try {
        Write-Status "   Uploading $packageName..."
        $packageBytes = [System.IO.File]::ReadAllBytes($PackagePath)
        $sizeMb = [math]::Round($packageBytes.Length / 1MB, 2)
        Write-Status "   Package size: $sizeMb MB"

        $headers = @{ "X-Package-SHA256" = $hash }
        $response = Invoke-RestMethod -Uri $url -Method Post -Body $packageBytes -ContentType "application/octet-stream" -Headers $headers -TimeoutSec 120

        if ($response.status -eq "accepted") {
            Write-Status "   [OK] Package uploaded and staged" -Type "Success"

            Write-Status "    Waiting for service to restart..."
            Start-Sleep -Seconds 8

            $healthUrl = "$($Stone.Endpoint.TrimEnd('/'))/health"
            $online = $false

            for ($i = 1; $i -le 20; $i++) {
                Start-Sleep -Seconds 3
                try {
                    $health = Invoke-RestMethod -Uri $healthUrl -Method Get -TimeoutSec 5
                    if ($health.status) {
                        Write-Status "   [OK] $($Stone.Name) is back online" -Type "Success"
                        $online = $true
                        break
                    }
                }
                catch { Write-Host "." -NoNewline }
            }

            if (-not $online) {
                Write-Status "   !  $($Stone.Name) did not respond after restart" -Type "Warning"
                return $false
            }

            Write-Status "   [OK] $($Stone.Name) updated" -Type "Success"
            return $true
        }
        else {
            Write-Status "   X Unexpected response: $($response | ConvertTo-Json -Compress)" -Type "Error"
            return $false
        }
    }
    catch {
        Write-Status "   X Failed to deploy to $($Stone.Name)" -Type "Error"
        Write-Status "      Error: $_" -Type "Error"
        return $false
    }
}

# Main execution
try {
    Write-Status "`n==============================================================="
    Write-Status "  Deploy Zen Garden Packages to All Stones"
    Write-Status "===============================================================`n"

    Write-Status " Using packages:" -ForegroundColor Cyan
    if ($linuxX64Package) {
        Write-Status "   Linux x64:   $(Split-Path -Leaf $linuxX64Package)"
    } else {
        Write-Status "   Linux x64:   (not available)" -Type "Warning"
    }
    if ($linuxX86Package) {
        Write-Status "   Linux x86:   $(Split-Path -Leaf $linuxX86Package)"
    }
    if ($windowsX64Package) {
        Write-Status "   Windows x64: $(Split-Path -Leaf $windowsX64Package)"
    } else {
        Write-Status "   Windows x64: (not available)" -Type "Warning"
    }
    Write-Host ""

    $stones = Find-AllStones -TimeoutSeconds $Timeout

    if ($stones.Count -eq 0) {
        Write-Status "`n!  No stones discovered on the network" -Type "Warning"
        exit 1
    }

    # Detect platform and prepare configs (parallel probing)
    Write-Status "`n Detecting platform for each stone..."
    $stoneConfigs = @()
    $skippedStones = @()

    # Probe all stones in parallel with 10s timeout
    $probeJobs = @()
    foreach ($stone in $stones) {
        $resolvedEndpoint = Resolve-StoneEndpoint -Stone $stone
        $probeJobs += Start-Job -ScriptBlock {
            param($StoneName, $Endpoint, $Address, $TimeoutSec)
            try {
                $url = "$($Endpoint.TrimEnd('/'))/health"
                $health = Invoke-RestMethod -Uri $url -Method Get -TimeoutSec $TimeoutSec
                return @{
                    Name = $StoneName
                    Endpoint = $Endpoint
                    Address = $Address
                    OS = if ($health.os) { $health.os } else { "linux" }
                    Architecture = if ($health.architecture) { $health.architecture } else { "unknown" }
                    Reachable = $true
                }
            }
            catch {
                return @{
                    Name = $StoneName
                    Endpoint = $Endpoint
                    Address = $Address
                    OS = "unknown"
                    Architecture = "unknown"
                    Reachable = $false
                }
            }
        } -ArgumentList $stone.Name, $resolvedEndpoint, $stone.Address, 10
    }

    # Stream results as each probe completes (capped at 15s total)
    $probeHandled = @{}
    $probeStart = Get-Date
    while ($probeHandled.Count -lt $probeJobs.Count -and ((Get-Date) - $probeStart).TotalSeconds -lt 15) {
        foreach ($job in $probeJobs) {
            if ($probeHandled.ContainsKey($job.Id)) { continue }
            if ($job.State -ne 'Completed') { continue }
            $probeHandled[$job.Id] = $true

            $info = Receive-Job $job
            $arch = $info.Architecture

            if (-not $info.Reachable -or $arch -eq "unknown") {
                Write-Status "   $($info.Name): SKIPPED (unreachable)" -Type "Warning"
                $skippedStones += $info.Name
                continue
            }

            $platformLabel = if ($info.OS -match "windows") { "Windows x64" }
                             elseif ($arch -eq "aarch64" -or $arch -eq "arm64") { "Linux ARM64" }
                             elseif ($arch -eq "x86" -or $arch -eq "i686" -or $arch -eq "i386") { "Linux x86" }
                             else { "Linux x64" }

            $packagePath = switch ($platformLabel) {
                "Windows x64" { $windowsX64Package }
                "Linux ARM64" { $linuxArm64Package }
                "Linux x86"   { $linuxX86Package }
                default       { $linuxX64Package }
            }

            if (-not $packagePath) {
                Write-Status "   $($info.Name): $platformLabel - SKIPPED (no package)" -Type "Warning"
                $skippedStones += $info.Name
                continue
            }

            Write-Status "   $($info.Name): $platformLabel" -Type "Success"

            $stoneConfigs += [PSCustomObject]@{
                Stone = [PSCustomObject]@{
                    Name = $info.Name
                    Endpoint = $info.Endpoint
                    Address = $info.Address
                }
                Platform = $platformLabel
                PackagePath = $packagePath
            }
        }
        if ($probeHandled.Count -lt $probeJobs.Count) { Start-Sleep -Milliseconds 250 }
    }

    # Handle any remaining timed-out probes
    foreach ($job in $probeJobs) {
        if ($probeHandled.ContainsKey($job.Id)) { continue }
        # Try to get the stone name from completed-but-unprocessed jobs
        if ($job.State -eq 'Completed') {
            $info = Receive-Job $job
            Write-Status "   $($info.Name): SKIPPED (unreachable)" -Type "Warning"
            $skippedStones += $info.Name
        } else {
            # Job still running after 15s - find matching stone name from launch order
            $idx = [array]::IndexOf($probeJobs, $job)
            $stoneName = if ($idx -ge 0 -and $idx -lt $stones.Count) { $stones[$idx].Name } else { "unknown" }
            Write-Status "   ${stoneName}: SKIPPED (timed out)" -Type "Warning"
            $skippedStones += $stoneName
        }
    }
    $probeJobs | Remove-Job -Force

    if ($stoneConfigs.Count -eq 0) {
        Write-Status "`n!  No stones can be deployed to" -Type "Warning"
        exit 1
    }

    if ($skippedStones.Count -gt 0) {
        Write-Status "`n!  Skipped $($skippedStones.Count) stone(s): $($skippedStones -join ', ')" -Type "Warning"
    }

    # Deploy
    Write-Status "`n Deploying packages to $($stoneConfigs.Count) stone(s)..."

    $results = @()

    if (-not $Sequential) {
        # Parallel deployment
        Write-Status "   Mode: Parallel deployment`n"

        $jobs = @()
        foreach ($config in $stoneConfigs) {
            $packageBytes = [System.IO.File]::ReadAllBytes($config.PackagePath)
            $packageHash = (Get-FileHash $config.PackagePath -Algorithm SHA256).Hash.ToLower()

            $jobs += Start-Job -ScriptBlock {
                param($StoneName, $StoneEndpoint, $PackageBytes, $PackageHash, $Platform)

                $url = "$($StoneEndpoint.TrimEnd('/'))/api/v1/stone/deploy"

                try {
                    $headers = @{ "X-Package-SHA256" = $PackageHash }
                    $response = Invoke-RestMethod -Uri $url -Method Post -Body $PackageBytes -ContentType "application/octet-stream" -Headers $headers -TimeoutSec 120

                    if ($response.status -ne "accepted") {
                        return @{ Success = $false; Name = $StoneName; Platform = $Platform; Error = "Unexpected: $($response.status)" }
                    }

                    # Wait for service to restart - 8s initial grace, then poll for up to 60s
                    Start-Sleep -Seconds 8

                    $healthUrl = "$($StoneEndpoint.TrimEnd('/'))/health"
                    $online = $false

                    for ($i = 1; $i -le 20; $i++) {
                        Start-Sleep -Seconds 3
                        try {
                            $health = Invoke-RestMethod -Uri $healthUrl -Method Get -TimeoutSec 5
                            if ($health.status) { $online = $true; break }
                        }
                        catch {}
                    }

                    if (-not $online) {
                        return @{ Success = $false; Name = $StoneName; Platform = $Platform; Error = "Did not come back online" }
                    }

                    return @{ Success = $true; Name = $StoneName; Platform = $Platform }
                }
                catch {
                    return @{ Success = $false; Name = $StoneName; Platform = $Platform; Error = $_.Exception.Message }
                }
            } -ArgumentList $config.Stone.Name, $config.Stone.Endpoint, $packageBytes, $packageHash, $config.Platform

            Write-Status "    Started: $($config.Stone.Name)"
        }

        # Stream results as each deployment completes
        Write-Status ""
        $deployHandled = @{}
        while ($deployHandled.Count -lt $jobs.Count) {
            foreach ($job in $jobs) {
                if ($deployHandled.ContainsKey($job.Id)) { continue }
                if ($job.State -ne 'Completed') { continue }
                $deployHandled[$job.Id] = $true

                $result = Receive-Job $job
                $results += $result
                if ($result.Success) {
                    Write-Status "   [OK] $($result.Name) ($($result.Platform)) - updated" -Type "Success"
                } else {
                    Write-Status "   X $($result.Name) ($($result.Platform)) - $($result.Error)" -Type "Error"
                }
            }
            if ($deployHandled.Count -lt $jobs.Count) { Start-Sleep -Milliseconds 500 }
        }
        $jobs | Remove-Job
    }
    else {
        # Sequential deployment
        Write-Status "   Mode: Sequential deployment`n"

        foreach ($config in $stoneConfigs) {
            $success = Deploy-PackageToStone -Stone $config.Stone -PackagePath $config.PackagePath -Platform $config.Platform
            $results += @{
                Success = $success
                Name = $config.Stone.Name
                Platform = $config.Platform
            }
        }
    }

    # Summary
    Write-Status "`n==============================================================="
    Write-Status "  Deployment Summary"
    Write-Status "===============================================================`n"

    $totalStones = @($stones).Count
    $successful = @($results | Where-Object { $_.Success }).Count
    $failed = @($results | Where-Object { -not $_.Success }).Count

    Write-Status "   Total stones: $totalStones"
    Write-Status "   Successful: $successful" -Type "Success"
    if ($failed -gt 0) {
        Write-Status "   Failed: $failed" -Type "Error"
        Write-Status "`nFailed stones:"
        foreach ($result in ($results | Where-Object { -not $_.Success })) {
            Write-Status "   X $($result.Name)" -Type "Error"
            if ($result.Error) { Write-Status "      $($result.Error)" -Type "Error" }
        }
    }

    Write-Status ""

    if ($failed -eq 0) {
        Write-Status "[OK] All stones updated successfully!" -Type "Success"
        exit 0
    }
    else {
        Write-Status "!  Some stones failed to update" -Type "Warning"
        exit 1
    }
}
catch {
    Write-Status "`nX Script failed: $_" -Type "Error"
    Write-Status $_.ScriptStackTrace -Type "Error"
    exit 1
}
