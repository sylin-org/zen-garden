#!/usr/bin/env pwsh
# Test discovery protocol with DIRECT UDP (not broadcast)

$targetIP = "192.168.1.197"

$udpClient = New-Object System.Net.Sockets.UdpClient
$udpClient.EnableBroadcast = $true

# Prepare discovery request with correct format
$requestId = [guid]::NewGuid().ToString()
$requestData = @{
    discover = "moss"
    request_id = $requestId
    requester = "test-direct"
}

# NOTE: Field name is "type" not "announcement_type"
$announcement = @{
    type = "discovery_request"
    data = $requestData
} | ConvertTo-Json -Compress

Write-Host "Sending to $targetIP`:7184 :" -ForegroundColor Cyan
Write-Host $announcement
Write-Host ""

$requestBytes = [System.Text.Encoding]::UTF8.GetBytes($announcement)

# Send DIRECT to stone (not broadcast)
$targetEndpoint = New-Object System.Net.IPEndPoint([System.Net.IPAddress]::Parse($targetIP), 7184)
$sent = $udpClient.Send($requestBytes, $requestBytes.Length, $targetEndpoint)
Write-Host "Sent $sent bytes to $targetIP`:7184"

# Listen for responses
$udpClient.Client.ReceiveTimeout = 3000
$remoteEP = New-Object System.Net.IPEndPoint([System.Net.IPAddress]::Any, 0)

Write-Host "Listening for responses..." -ForegroundColor Cyan

$startTime = Get-Date
while (((Get-Date) - $startTime).TotalSeconds -lt 3) {
    try {
        $responseBytes = $udpClient.Receive([ref]$remoteEP)
        $responseJson = [System.Text.Encoding]::UTF8.GetString($responseBytes)
        Write-Host "`nReceived from $($remoteEP.Address):" -ForegroundColor Green
        Write-Host $responseJson
        
        $envelope = $responseJson | ConvertFrom-Json
        Write-Host "Type: $($envelope.type)" -ForegroundColor Yellow
        Write-Host "Data: $($envelope.data | ConvertTo-Json)" -ForegroundColor Yellow
    }
    catch [System.Net.Sockets.SocketException] {
        Write-Host "Timeout after 3s" -ForegroundColor Yellow
        break
    }
    catch {
        Write-Host "Error: $_" -ForegroundColor Red
    }
}

$udpClient.Close()
