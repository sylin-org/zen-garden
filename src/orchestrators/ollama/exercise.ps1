#!/usr/bin/env pwsh
# exercise.ps1 — Stress-test the Ollama Orchestrator proxy routing.
#
# Fires parallel requests with different models and payload types to
# exercise VRAM-aware routing across stones. Watch the dashboard at
# http://localhost:7190 while this runs.
#
# The first request for each model cold-loads it into GPU VRAM, which
# can take 30-60s for large models. The script warms up with embedding
# models first (fast), then LLMs.

param(
    [string]$Proxy = "http://localhost:21434",
    [string]$Dashboard = "http://localhost:7190",
    [int]$Rounds = 5,
    [int]$DelayMs = 500
)

$ErrorActionPreference = "Continue"

# ── Colors ──────────────────────────────────────────────────────
$c = @{ R = "`e[0m"; S = "`e[38;5;108m"; C = "`e[38;5;180m"; G = "`e[38;5;186m"; M = "`e[38;5;246m"; E = "`e[38;5;167m" }

# ── Payloads ────────────────────────────────────────────────────
# Warmup payloads — small models, run sequentially to cold-load VRAM
$warmups = @(
    @{ Label = "warmup: minilm embed";  Endpoint = "/api/embed";     Body = @{ model = "all-minilm:latest";       input = "warmup" } }
    @{ Label = "warmup: nomic embed";   Endpoint = "/api/embed";     Body = @{ model = "nomic-embed-text:latest"; input = "warmup" } }
    @{ Label = "warmup: llama3.2";      Endpoint = "/api/generate";  Body = @{ model = "llama3.2:latest";         prompt = "Hi"; stream = $false } }
)

# Exercise payloads — mixed models and sizes for parallel bursts
$payloads = @(
    @{ Label = "llama3.2 generate";   Endpoint = "/api/generate";  Body = @{ model = "llama3.2:latest";       prompt = "Explain quantum entanglement in two sentences."; stream = $false } }
    @{ Label = "llama3.2 short";      Endpoint = "/api/generate";  Body = @{ model = "llama3.2:latest";       prompt = "Say hello."; stream = $false } }
    @{ Label = "llama3.2 chat";       Endpoint = "/api/chat";      Body = @{ model = "llama3.2:latest";       messages = @(@{ role = "user"; content = "What is the capital of France?" }); stream = $false } }
    @{ Label = "qwen2.5vl generate";  Endpoint = "/api/generate";  Body = @{ model = "qwen2.5vl:latest";     prompt = "Describe a sunset in one sentence."; stream = $false } }
    @{ Label = "qwen2.5vl chat";      Endpoint = "/api/chat";      Body = @{ model = "qwen2.5vl:latest";     messages = @(@{ role = "user"; content = "List 3 prime numbers." }); stream = $false } }
    @{ Label = "minilm embed";        Endpoint = "/api/embed";     Body = @{ model = "all-minilm:latest";    input = "The quick brown fox jumps over the lazy dog." } }
    @{ Label = "nomic embed";         Endpoint = "/api/embed";     Body = @{ model = "nomic-embed-text:latest"; input = "Zen Garden orchestrates Ollama instances across stones." } }
    @{ Label = "nomic embed batch";   Endpoint = "/api/embed";     Body = @{ model = "nomic-embed-text:latest"; input = @("First sentence.", "Second sentence.", "Third sentence.") } }
)

# ── Banner ──────────────────────────────────────────────────────
Write-Host ""
Write-Host "$($c.S)  Ollama Orchestrator — Exercise Script$($c.R)"
Write-Host "$($c.M)  ─────────────────────────────────────$($c.R)"
Write-Host "$($c.M)  Proxy:     $Proxy$($c.R)"
Write-Host "$($c.M)  Dashboard: $Dashboard$($c.R)"
Write-Host "$($c.M)  Rounds:    $Rounds × 2-3 parallel payloads$($c.R)"
Write-Host "$($c.M)  Delay:     ${DelayMs}ms between rounds$($c.R)"
Write-Host ""

# ── Pre-flight ──────────────────────────────────────────────────
try {
    $status = Invoke-RestMethod -Uri "$Dashboard/api/status" -TimeoutSec 5
    $stoneCount = ($status.stones | Measure-Object).Count
    $modelCount = ($status.models | Measure-Object).Count
    Write-Host "$($c.S)  ✓$($c.R) Connected — $stoneCount stone(s), $modelCount model(s)"
    foreach ($s in $status.stones) {
        Write-Host "$($c.M)    · $($s.stone_name) ($($s.endpoint)) — $($s.vram_budget_mb) MB VRAM$($c.R)"
    }
    Write-Host ""
} catch {
    Write-Host "$($c.E)  ✗ Cannot reach orchestrator at $Dashboard — is it running?$($c.R)"
    exit 1
}

# ── Helper: fire a single request ──────────────────────────────
function Invoke-OllamaRequest {
    param($Uri, $Json, $Label, $TimeoutSec = 120)
    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    try {
        $resp = Invoke-WebRequest -Uri $Uri -Method POST -Body $Json `
            -ContentType "application/json" -TimeoutSec $TimeoutSec -UseBasicParsing
        $sw.Stop()
        $bodyObj = $resp.Content | ConvertFrom-Json -ErrorAction SilentlyContinue

        $preview = ""
        if ($bodyObj.response) {
            $preview = $bodyObj.response.Substring(0, [Math]::Min(60, $bodyObj.response.Length)).Trim()
        } elseif ($bodyObj.message -and $bodyObj.message.content) {
            $preview = $bodyObj.message.content.Substring(0, [Math]::Min(60, $bodyObj.message.content.Length)).Trim()
        } elseif ($bodyObj.embeddings) {
            $count = ($bodyObj.embeddings | Measure-Object).Count
            $preview = "[embedding: ${count} vector(s)]"
        }
        [PSCustomObject]@{ Label = $Label; OK = $true; Ms = $sw.ElapsedMilliseconds; Preview = $preview; Error = $null }
    } catch {
        $sw.Stop()
        $msg = $_.Exception.Message
        if ($msg.Length -gt 80) { $msg = $msg.Substring(0, 80) }
        [PSCustomObject]@{ Label = $Label; OK = $false; Ms = $sw.ElapsedMilliseconds; Preview = ""; Error = $msg }
    }
}

# ── Stats ───────────────────────────────────────────────────────
$stats = @{ Total = 0; OK = 0; Err = 0 }
$modelStats = @{}

function Record-Result($r) {
    $script:stats.Total++
    $model = ($r.Label -split ' ')[0]
    if ($model.EndsWith(':')) { $model = $model.TrimEnd(':') }
    if (-not $script:modelStats[$model]) { $script:modelStats[$model] = @{ OK = 0; Err = 0; TotalMs = 0 } }
    if ($r.OK) {
        $script:stats.OK++
        $script:modelStats[$model].OK++
        $script:modelStats[$model].TotalMs += $r.Ms
        $secs = [math]::Round($r.Ms / 1000, 1)
        $preview = if ($r.Preview) { " — $($r.Preview)" } else { "" }
        Write-Host "  $($c.S)✓$($c.R) $($c.C)$($r.Label)$($c.R) $($c.M)(${secs}s)$($c.R)$preview"
    } else {
        $script:stats.Err++
        $script:modelStats[$model].Err++
        Write-Host "  $($c.E)✗$($c.R) $($c.C)$($r.Label)$($c.R) — $($c.E)$($r.Error)$($c.R)"
    }
}

# ── Phase 1: Warmup (sequential, generous timeout) ─────────────
Write-Host "$($c.G)── Warmup (cold-loading models into VRAM) ──$($c.R)"
Write-Host "$($c.M)  This may take 30-60s per model on first load...$($c.R)"

foreach ($w in $warmups) {
    $uri = "$Proxy$($w.Endpoint)"
    $json = $w.Body | ConvertTo-Json -Depth 4 -Compress
    $r = Invoke-OllamaRequest -Uri $uri -Json $json -Label $w.Label -TimeoutSec 120
    Record-Result $r
}

Write-Host ""

# ── Phase 2: Exercise (parallel bursts) ─────────────────────────
for ($round = 1; $round -le $Rounds; $round++) {
    Write-Host "$($c.G)── Round $round/$Rounds ──$($c.R)"

    # Pick 2-3 random payloads for parallel burst
    $burst = $payloads | Get-Random -Count ([Math]::Min(3, $payloads.Count))

    $jobs = @()
    foreach ($p in $burst) {
        $uri = "$Proxy$($p.Endpoint)"
        $json = $p.Body | ConvertTo-Json -Depth 4 -Compress
        $label = $p.Label

        $jobs += Start-Job -ScriptBlock {
            param($uri, $json, $label)
            $sw = [System.Diagnostics.Stopwatch]::StartNew()
            try {
                $resp = Invoke-WebRequest -Uri $uri -Method POST -Body $json `
                    -ContentType "application/json" -TimeoutSec 120 -UseBasicParsing
                $sw.Stop()
                $bodyObj = $resp.Content | ConvertFrom-Json -ErrorAction SilentlyContinue

                $preview = ""
                if ($bodyObj.response) {
                    $preview = $bodyObj.response.Substring(0, [Math]::Min(60, $bodyObj.response.Length)).Trim()
                } elseif ($bodyObj.message -and $bodyObj.message.content) {
                    $preview = $bodyObj.message.content.Substring(0, [Math]::Min(60, $bodyObj.message.content.Length)).Trim()
                } elseif ($bodyObj.embeddings) {
                    $count = ($bodyObj.embeddings | Measure-Object).Count
                    $preview = "[embedding: ${count} vector(s)]"
                }
                [PSCustomObject]@{ Label = $label; OK = $true; Ms = $sw.ElapsedMilliseconds; Preview = $preview; Error = $null }
            } catch {
                $sw.Stop()
                $msg = $_.Exception.Message
                if ($msg.Length -gt 80) { $msg = $msg.Substring(0, 80) }
                [PSCustomObject]@{ Label = $label; OK = $false; Ms = $sw.ElapsedMilliseconds; Preview = ""; Error = $msg }
            }
        } -ArgumentList $uri, $json, $label
    }

    # Wait for all jobs in this burst (2 min ceiling)
    $results = $jobs | Wait-Job -Timeout 130 | Receive-Job
    $jobs | Remove-Job -Force -ErrorAction SilentlyContinue

    foreach ($r in $results) {
        Record-Result $r
    }

    if ($round -lt $Rounds) {
        Start-Sleep -Milliseconds $DelayMs
    }
}

# ── Summary ─────────────────────────────────────────────────────
Write-Host ""
Write-Host "$($c.S)  ─── Summary ───$($c.R)"
Write-Host "$($c.M)  Total: $($stats.Total)  OK: $($c.S)$($stats.OK)$($c.M)  Errors: $($c.E)$($stats.Err)$($c.R)"
Write-Host ""

foreach ($m in $modelStats.Keys | Sort-Object) {
    $ms = $modelStats[$m]
    $avg = if ($ms.OK -gt 0) { [math]::Round($ms.TotalMs / $ms.OK) } else { 0 }
    Write-Host "$($c.M)  $($c.C)$m$($c.M): $($ms.OK) ok / $($ms.Err) err — avg ${avg}ms$($c.R)"
}

# Show final stone queue state
try {
    $final = Invoke-RestMethod -Uri "$Dashboard/api/status" -TimeoutSec 5
    Write-Host ""
    Write-Host "$($c.S)  ─── Stone State ───$($c.R)"
    foreach ($s in $final.stones) {
        $loaded = ($s.models_loaded | ForEach-Object { if ($_.name) { $_.name } else { $_ } }) -join ", "
        if (-not $loaded) { $loaded = "none" }
        Write-Host "$($c.M)  $($s.stone_name): queue=$($s.queue_depth), loaded=[$loaded]$($c.R)"
    }

    Write-Host ""
    Write-Host "$($c.S)  ─── Metrics ───$($c.R)"
    $met = $final.metrics
    Write-Host "$($c.M)  requests: $($met.requests_total)  tokens_out: $($met.tokens_out)  errors: $($met.errors)$($c.R)"
    if ($met.top_models) {
        $met.top_models | ForEach-Object { Write-Host "$($c.M)    $($_[0]): $($_[1]) requests$($c.R)" }
    }
} catch {}

Write-Host ""
