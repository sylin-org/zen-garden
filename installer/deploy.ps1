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
    Write-Host "`n🔨 Building packages..." -ForegroundColor Cyan
    $buildScript = Join-Path $PSScriptRoot "build.ps1"
    & $buildScript
    if ($LASTEXITCODE -ne 0) {
        Write-Host "✗ Build failed" -ForegroundColor Red
        exit 1
    }
    Write-Host ""
}

# Find packages
$distRoot = Resolve-Path "$PSScriptRoot/../dist"
$packagesDir = Join-Path $distRoot "packages"

$linuxPackage = $null
$windowsPackage = $null

if (Test-Path $packagesDir) {
    $linuxPackages = Get-ChildItem $packagesDir -Filter "zen-garden-*-linux-amd64.tar.gz" | Sort-Object LastWriteTime -Descending
    $windowsPackages = Get-ChildItem $packagesDir -Filter "zen-garden-*-windows-amd64.zip" | Sort-Object LastWriteTime -Descending
    if ($linuxPackages.Count -gt 0) { $linuxPackage = $linuxPackages[0].FullName }
    if ($windowsPackages.Count -gt 0) { $windowsPackage = $windowsPackages[0].FullName }
}

if (-not $linuxPackage -and -not $windowsPackage) {
    Write-Host "⚠️  No packages found in $packagesDir" -ForegroundColor Yellow
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

    Write-Status "🔍 Discovering stones on network (timeout: ${TimeoutSeconds}s)..."

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

                $endpoint = $response.stone_endpoint
                if ($Port -gt 0) {
                    $endpoint = $endpoint -replace ':\d+$', ":$Port"
                }

                $stones.Add([PSCustomObject]@{
                    Name = $response.stone_name
                    Endpoint = $endpoint
                    Address = $remoteEP.Address.ToString()
                }) | Out-Null
                Write-Status "   ✓ Found: $($response.stone_name) at $endpoint" -Type "Success"
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
        Write-Status "   ⚠️  $($Stone.Name) reports loopback, using UDP source address" -Type "Warning"
        $port = "7185"
        if ($endpoint -match ":(\d+)") { $port = $Matches[1] }
        if ($Stone.Address -and $Stone.Address -ne "127.0.0.1") {
            return "http://$($Stone.Address):$port"
        }
    }
    return $endpoint
}

function Get-StoneInfo {
    param([PSCustomObject]$Stone)

    $resolvedEndpoint = Resolve-StoneEndpoint -Stone $Stone

    try {
        $url = "$($resolvedEndpoint.TrimEnd('/'))/health"
        $health = Invoke-RestMethod -Uri $url -Method Get -TimeoutSec 3
        return [PSCustomObject]@{
            OS = if ($health.os) { $health.os } else { "linux" }
            ResolvedEndpoint = $resolvedEndpoint
        }
    }
    catch {
        return [PSCustomObject]@{
            OS = "linux"
            ResolvedEndpoint = $resolvedEndpoint
        }
    }
}

function Deploy-PackageToStone {
    param(
        [PSCustomObject]$Stone,
        [string]$PackagePath,
        [string]$Platform
    )

    Write-Status "`n📦 Deploying package to $($Stone.Name) ($Platform)..."

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
            Write-Status "   ✅ Package uploaded and staged" -Type "Success"

            Write-Status "   ⏳ Waiting for service to restart..."
            Start-Sleep -Seconds 5

            $healthUrl = "$($Stone.Endpoint.TrimEnd('/'))/health"
            $online = $false

            for ($i = 1; $i -le 15; $i++) {
                Start-Sleep -Seconds 2
                try {
                    $health = Invoke-RestMethod -Uri $healthUrl -Method Get -TimeoutSec 3
                    if ($health.status) {
                        Write-Status "   ✅ $($Stone.Name) is back online" -Type "Success"
                        $online = $true
                        break
                    }
                }
                catch { Write-Host "." -NoNewline }
            }

            if (-not $online) {
                Write-Status "   ⚠️  $($Stone.Name) did not respond after restart" -Type "Warning"
                return $false
            }

            Write-Status "   ✅ $($Stone.Name) updated" -Type "Success"
            return $true
        }
        else {
            Write-Status "   ✗ Unexpected response: $($response | ConvertTo-Json -Compress)" -Type "Error"
            return $false
        }
    }
    catch {
        Write-Status "   ✗ Failed to deploy to $($Stone.Name)" -Type "Error"
        Write-Status "      Error: $_" -Type "Error"
        return $false
    }
}

# Main execution
try {
    Write-Status "`n═══════════════════════════════════════════════════════════════"
    Write-Status "  Deploy Zen Garden Packages to All Stones"
    Write-Status "═══════════════════════════════════════════════════════════════`n"

    Write-Status "📦 Using packages:" -ForegroundColor Cyan
    if ($linuxPackage) {
        Write-Status "   Linux:   $(Split-Path -Leaf $linuxPackage)"
    } else {
        Write-Status "   Linux:   (not available)" -Type "Warning"
    }
    if ($windowsPackage) {
        Write-Status "   Windows: $(Split-Path -Leaf $windowsPackage)"
    } else {
        Write-Status "   Windows: (not available)" -Type "Warning"
    }
    Write-Host ""

    $stones = Find-AllStones -TimeoutSeconds $Timeout

    if ($stones.Count -eq 0) {
        Write-Status "`n⚠️  No stones discovered on the network" -Type "Warning"
        exit 1
    }

    # Detect platform and prepare configs
    Write-Status "`n🔍 Detecting platform for each stone..."
    $stoneConfigs = @()
    $skippedStones = @()

    foreach ($stone in $stones) {
        $info = Get-StoneInfo -Stone $stone
        $platform = if ($info.OS -match "windows") { "Windows" } else { "Linux" }
        $packagePath = if ($platform -eq "Windows") { $windowsPackage } else { $linuxPackage }

        if (-not $packagePath) {
            Write-Status "   $($stone.Name): $platform - SKIPPED (no package)" -Type "Warning"
            $skippedStones += $stone.Name
            continue
        }

        Write-Status "   $($stone.Name): $platform" -Type "Success"

        $stoneConfigs += [PSCustomObject]@{
            Stone = [PSCustomObject]@{
                Name = $stone.Name
                Endpoint = $info.ResolvedEndpoint
                Address = $stone.Address
            }
            Platform = $platform
            PackagePath = $packagePath
        }
    }

    if ($stoneConfigs.Count -eq 0) {
        Write-Status "`n⚠️  No stones can be deployed to" -Type "Warning"
        exit 1
    }

    if ($skippedStones.Count -gt 0) {
        Write-Status "`n⚠️  Skipped $($skippedStones.Count) stone(s): $($skippedStones -join ', ')" -Type "Warning"
    }

    # Deploy
    Write-Status "`n📡 Deploying packages to $($stoneConfigs.Count) stone(s)..."

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

                    Start-Sleep -Seconds 5

                    $healthUrl = "$($StoneEndpoint.TrimEnd('/'))/health"
                    $online = $false

                    for ($i = 1; $i -le 15; $i++) {
                        Start-Sleep -Seconds 2
                        try {
                            $health = Invoke-RestMethod -Uri $healthUrl -Method Get -TimeoutSec 3
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

            Write-Status "   ⏳ Started: $($config.Stone.Name)"
        }

        Write-Status "`n   Waiting for deployments..."
        $completed = 0
        while ($completed -lt $jobs.Count) {
            $done = @($jobs | Where-Object { $_.State -eq 'Completed' }).Count
            if ($done -gt $completed) {
                $completed = $done
                Write-Status "   Progress: $completed/$($jobs.Count)"
            }
            Start-Sleep -Milliseconds 500
        }

        $results = $jobs | Receive-Job
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
    Write-Status "`n═══════════════════════════════════════════════════════════════"
    Write-Status "  Deployment Summary"
    Write-Status "═══════════════════════════════════════════════════════════════`n"

    $totalStones = @($stones).Count
    $successful = @($results | Where-Object { $_.Success }).Count
    $failed = @($results | Where-Object { -not $_.Success }).Count

    Write-Status "   Total stones: $totalStones"
    Write-Status "   Successful: $successful" -Type "Success"
    if ($failed -gt 0) {
        Write-Status "   Failed: $failed" -Type "Error"
        Write-Status "`nFailed stones:"
        foreach ($result in ($results | Where-Object { -not $_.Success })) {
            Write-Status "   ✗ $($result.Name)" -Type "Error"
            if ($result.Error) { Write-Status "      $($result.Error)" -Type "Error" }
        }
    }

    Write-Status ""

    if ($failed -eq 0) {
        Write-Status "✅ All stones updated successfully!" -Type "Success"
        exit 0
    }
    else {
        Write-Status "⚠️  Some stones failed to update" -Type "Warning"
        exit 1
    }
}
catch {
    Write-Status "`n✗ Script failed: $_" -Type "Error"
    Write-Status $_.ScriptStackTrace -Type "Error"
    exit 1
}
