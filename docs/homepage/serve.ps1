# Serve homepage options locally
# Usage: ./serve.ps1 [port]
# Then open http://localhost:8080 (or your chosen port)

param(
    [int]$Port = 8080
)

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path

Write-Host ""
Write-Host "   Zen Garden Homepage Preview" -ForegroundColor Green
Write-Host "  ===============================" -ForegroundColor DarkGray
Write-Host ""
Write-Host "  Starting server on port $Port..." -ForegroundColor Cyan
Write-Host ""
Write-Host "  Pages available:" -ForegroundColor White
Write-Host "    http://localhost:$Port/                 -> Option E: The Quiet Place (index)" -ForegroundColor Gray
Write-Host "    http://localhost:$Port/option-a.html    -> Option A: Quiet Garden" -ForegroundColor Gray
Write-Host "    http://localhost:$Port/option-b.html    -> Option B: Permission Slip" -ForegroundColor Gray
Write-Host "    http://localhost:$Port/option-c.html    -> Option C: Metaphor Journey" -ForegroundColor Gray
Write-Host "    http://localhost:$Port/option-d.html    -> Option D: Hybrid A+B" -ForegroundColor Gray
Write-Host ""
Write-Host "  Press Ctrl+C to stop the server" -ForegroundColor DarkGray
Write-Host ""

# Try Python first (more common)
$pythonCmd = Get-Command python -ErrorAction SilentlyContinue
$python3Cmd = Get-Command python3 -ErrorAction SilentlyContinue

if ($pythonCmd -or $python3Cmd) {
    $pyExe = if ($python3Cmd) { "python3" } else { "python" }
    
    # Check Python version
    $pyVersion = & $pyExe --version 2>&1
    Write-Host "  Using $pyVersion" -ForegroundColor DarkGray
    Write-Host ""
    
    Push-Location $ScriptDir
    try {
        & $pyExe -m http.server $Port
    }
    finally {
        Pop-Location
    }
}
else {
    # Fallback to .NET HttpListener
    Write-Host "  Python not found, using .NET HttpListener..." -ForegroundColor Yellow
    Write-Host ""
    
    $listener = New-Object System.Net.HttpListener
    $listener.Prefixes.Add("http://localhost:$Port/")
    $listener.Start()
    
    try {
        while ($listener.IsListening) {
            $context = $listener.GetContext()
            $request = $context.Request
            $response = $context.Response
            
            $localPath = $request.Url.LocalPath
            if ($localPath -eq "/") { $localPath = "/index.html" }
            
            $filePath = Join-Path $ScriptDir $localPath.TrimStart("/")
            
            if (Test-Path $filePath) {
                $content = [System.IO.File]::ReadAllBytes($filePath)
                $response.ContentType = "text/html"
                $response.ContentLength64 = $content.Length
                $response.OutputStream.Write($content, 0, $content.Length)
                Write-Host "  GET $localPath -> 200" -ForegroundColor Green
            }
            else {
                $response.StatusCode = 404
                $notFound = [System.Text.Encoding]::UTF8.GetBytes("Not Found")
                $response.OutputStream.Write($notFound, 0, $notFound.Length)
                Write-Host "  GET $localPath -> 404" -ForegroundColor Red
            }
            
            $response.Close()
        }
    }
    finally {
        $listener.Stop()
    }
}
