#!/usr/bin/env pwsh
# exercise.ps1 — Black-box exerciser for the Ollama Orchestrator proxy.
#
# Discovers models dynamically via /v1/models and /v1/stones, then runs
# three phases:
#
#   1. Warm+Test  — load each model, immediately exercise its payloads while hot
#   2. Chaos      — random mixed-model parallel bursts (cold starts, evictions)
#   3. Summary    — per-model stats and final stone state
#
# Usage:
#   ./exercise.ps1                        # defaults
#   ./exercise.ps1 -Rounds 10 -Burst 4   # heavier chaos phase
#   ./exercise.ps1 -WarmupOnly            # just phase 1, no chaos

param(
    [string]$Proxy = "http://localhost:21434",
    [int]$Rounds = 5,
    [int]$Burst = 3,
    [int]$DelayMs = 500,
    [switch]$WarmupOnly
)

$ErrorActionPreference = "Continue"

# ── Colors ──────────────────────────────────────────────────────
$c = @{
    R = "`e[0m"; S = "`e[38;5;108m"; C = "`e[38;5;180m"
    G = "`e[38;5;186m"; M = "`e[38;5;246m"; E = "`e[38;5;167m"
    D = "`e[38;5;243m"
}

# ── Banner ──────────────────────────────────────────────────────
Write-Host ""
Write-Host "$($c.S)  Ollama Orchestrator — Exercise Script$($c.R)"
Write-Host "$($c.M)  ─────────────────────────────────────$($c.R)"
Write-Host "$($c.M)  Proxy:  $Proxy$($c.R)"
Write-Host "$($c.M)  Rounds: $Rounds × $Burst parallel payloads$($c.R)"
Write-Host ""

# ── Discovery via /v1/ extension API ───────────────────────────
try {
    $stonesResp = Invoke-RestMethod -Uri "$Proxy/v1/stones" -TimeoutSec 5
    $modelsResp = Invoke-RestMethod -Uri "$Proxy/v1/models" -TimeoutSec 5
} catch {
    Write-Host "$($c.E)  ✗ Cannot reach orchestrator at $Proxy — is it running?$($c.R)"
    Write-Host "$($c.E)    $_$($c.R)"
    exit 1
}

$stones = $stonesResp.stones
$models = $modelsResp.models

Write-Host "$($c.S)  ✓$($c.R) Discovered $($stones.Count) stone(s), $($models.Count) model(s)"
Write-Host ""

# ── Show stones ─────────────────────────────────────────────────
Write-Host "$($c.G)── Stones ──$($c.R)"
foreach ($s in $stones) {
    $gpu = if ($s.gpu) { "$($s.gpu.name) — $($s.gpu.vram_total_mb) MB" } else { "no GPU" }
    $health = if ($s.health -eq "healthy") { "$($c.S)healthy$($c.R)" } else { "$($c.E)$($s.health)$($c.R)" }
    Write-Host "  $($c.C)$($s.name)$($c.R) $($c.D)($($s.tier))$($c.R) $health"
    Write-Host "    $($c.M)$gpu$($c.R)"
    Write-Host "    $($c.M)available: $($s.models.available -join ', ')$($c.R)"
    if ($s.models.loaded) {
        Write-Host "    $($c.M)loaded:    $($s.models.loaded -join ', ')$($c.R)"
    }
}
Write-Host ""

# ── Build payloads from discovered models ───────────────────────
$payloads = @()

foreach ($m in $models) {
    $name = $m.name
    $caps = $m.capabilities

    if ($caps -contains "completion" -or $caps -contains "chat") {
        $payloads += @{
            Label    = "$name generate"
            Endpoint = "/api/generate"
            Body     = @{ model = $name; prompt = "Explain gravity in one sentence."; stream = $false }
            Warmup   = $false
        }
        $payloads += @{
            Label    = "$name chat"
            Endpoint = "/api/chat"
            Body     = @{ model = $name; messages = @(@{ role = "user"; content = "What is 2+2?" }); stream = $false }
            Warmup   = $false
        }
    }

    if ($caps -contains "embedding") {
        $payloads += @{
            Label    = "$name embed"
            Endpoint = "/api/embed"
            Body     = @{ model = $name; input = "The quick brown fox jumps over the lazy dog." }
            Warmup   = $false
        }
        $payloads += @{
            Label    = "$name embed-batch"
            Endpoint = "/api/embed"
            Body     = @{ model = $name; input = @("First.", "Second.", "Third.") }
            Warmup   = $false
        }
    }

    if ($caps -contains "vision") {
        # Vision models support chat with image URLs — exercise text-only path
        $payloads += @{
            Label    = "$name vision-text"
            Endpoint = "/api/chat"
            Body     = @{ model = $name; messages = @(@{ role = "user"; content = "Describe a circle." }); stream = $false }
            Warmup   = $false
        }
    }
}

# ── Filter out models blocked on every stone ───────────────────
# If ALL placements report fitness_score == 0 the orchestrator will refuse to
# route there anyway (ModelBlocked).  Skip them so we don't waste warmup time.
$blocked = @()
foreach ($m in $models) {
    $placements = $m.available_on
    if ($placements -and $placements.Count -gt 0) {
        $allBlocked = ($placements | Where-Object { $_.fitness_score -ne $null -and $_.fitness_score -eq 0 }).Count -eq $placements.Count
        if ($allBlocked) { $blocked += $m.name }
    }
}
if ($blocked.Count -gt 0) {
    Write-Host "$($c.G)── Blocked Models (skipped) ──$($c.R)"
    foreach ($b in $blocked) {
        Write-Host "  $($c.E)⊘$($c.R) $($c.C)$b$($c.R) $($c.M)— blocked on all stones$($c.R)"
    }
    Write-Host ""
    $payloads = @($payloads | Where-Object { $blocked -notcontains ($_.Body).model })
}

if ($payloads.Count -eq 0) {
    Write-Host "$($c.E)  No exercisable models found (need completion, chat, embedding, or vision capabilities)$($c.R)"
    exit 1
}

Write-Host "$($c.G)── Payloads ──$($c.R)"
Write-Host "  $($c.M)Built $($payloads.Count) payloads from $($models.Count) models$($c.R)"
Write-Host ""

# ── Group payloads by model ─────────────────────────────────────
$modelPayloads = @{}
foreach ($p in $payloads) {
    $modelName = ($p.Body).model
    if (-not $modelPayloads[$modelName]) { $modelPayloads[$modelName] = @() }
    $modelPayloads[$modelName] += $p
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
        if ($msg.Length -gt 100) { $msg = $msg.Substring(0, 100) }
        [PSCustomObject]@{ Label = $Label; OK = $false; Ms = $sw.ElapsedMilliseconds; Preview = ""; Error = $msg }
    }
}

# ── Stats ───────────────────────────────────────────────────────
$stats = @{ Total = 0; OK = 0; Err = 0 }
$modelStats = @{}

function Record-Result($r) {
    $script:stats.Total++
    $parts = $r.Label -split ' '
    $model = if ($parts[0] -eq "warmup:") { $parts[1] } else { $parts[0] }
    if (-not $script:modelStats[$model]) {
        $script:modelStats[$model] = @{ OK = 0; Err = 0; TotalMs = 0; Errors = @() }
    }
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
        $script:modelStats[$model].Errors += $r.Error
        Write-Host "  $($c.E)✗$($c.R) $($c.C)$($r.Label)$($c.R) — $($c.E)$($r.Error)$($c.R)"
    }
}

# ═══════════════════════════════════════════════════════════════
# Phase 1: Warm + Test (per-model, sequential, smallest first)
# Load each model then immediately exercise all its payloads
# while it's still hot in VRAM.
# ═══════════════════════════════════════════════════════════════
Write-Host "$($c.G)══ Phase 1: Warm + Test ══$($c.R)"
Write-Host "$($c.M)  $($modelPayloads.Count) models — smallest → largest, warm each then exercise while hot$($c.R)"
Write-Host ""

# Sort models by VRAM requirement (smallest first), falling back to size on disk
$modelSizeMap = @{}
foreach ($m in $models) {
    $size = if ($m.vram_bytes) { $m.vram_bytes } elseif ($m.size_disk) { $m.size_disk } else { [long]::MaxValue }
    $modelSizeMap[$m.name] = $size
}
$sortedModelNames = @($modelPayloads.Keys | Sort-Object { $modelSizeMap[$_] })

$modelIndex = 0
foreach ($modelName in $sortedModelNames) {
    $modelIndex++
    $mPayloads = $modelPayloads[$modelName]
    $sizeLabel = ""
    $sizeBytes = $modelSizeMap[$modelName]
    if ($sizeBytes -and $sizeBytes -ne [long]::MaxValue) {
        $sizeMB = [math]::Round($sizeBytes / 1MB)
        $sizeLabel = " $($c.D)(${sizeMB} MB)$($c.R)"
    }
    Write-Host "$($c.G)── [$modelIndex/$($modelPayloads.Count)] $modelName$sizeLabel ──$($c.R)"

    # Warmup: pick the lightest payload (embedding > chat > generate)
    $warmupPayload = $mPayloads | Sort-Object { switch (($_.Endpoint)) { "/api/embed" { 0 } "/api/chat" { 1 } default { 2 } } } | Select-Object -First 1
    $uri = "$Proxy$($warmupPayload.Endpoint)"
    $json = $warmupPayload.Body | ConvertTo-Json -Depth 4 -Compress
    $r = Invoke-OllamaRequest -Uri $uri -Json $json -Label "warmup: $modelName" -TimeoutSec 180
    Record-Result $r

    if (-not $r.OK) {
        Write-Host "  $($c.D)  skipping remaining payloads for $modelName$($c.R)"
        Write-Host ""
        continue
    }

    # Exercise: fire remaining payloads while model is still loaded
    foreach ($p in $mPayloads) {
        # Skip if this is the same as the warmup payload
        if ($p.Endpoint -eq $warmupPayload.Endpoint -and $p.Label -eq $warmupPayload.Label) { continue }
        $uri = "$Proxy$($p.Endpoint)"
        $json = $p.Body | ConvertTo-Json -Depth 4 -Compress
        $r = Invoke-OllamaRequest -Uri $uri -Json $json -Label $p.Label
        Record-Result $r
    }
    Write-Host ""
}

if ($WarmupOnly) {
    Write-Host "$($c.S)  Warm+Test complete — skipping chaos phase.$($c.R)"
    Write-Host ""
} else {

# ═══════════════════════════════════════════════════════════════
# Phase 2: Chaos (mixed-model parallel bursts)
# Models evict each other, cold starts happen, routing is stressed.
# ═══════════════════════════════════════════════════════════════
Write-Host "$($c.G)══ Phase 2: Chaos ($Rounds rounds × $Burst parallel) ══$($c.R)"
Write-Host "$($c.M)  Random mixed-model bursts — expect evictions and cold starts$($c.R)"
Write-Host ""

for ($round = 1; $round -le $Rounds; $round++) {
    Write-Host "$($c.G)── Chaos $round/$Rounds ──$($c.R)"

    $pick = $payloads | Get-Random -Count ([Math]::Min($Burst, $payloads.Count))

    $jobs = @()
    foreach ($p in $pick) {
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
                if ($msg.Length -gt 100) { $msg = $msg.Substring(0, 100) }
                [PSCustomObject]@{ Label = $label; OK = $false; Ms = $sw.ElapsedMilliseconds; Preview = ""; Error = $msg }
            }
        } -ArgumentList $uri, $json, $label
    }

    $results = $jobs | Wait-Job -Timeout 130 | Receive-Job
    $jobs | Remove-Job -Force -ErrorAction SilentlyContinue

    foreach ($r in $results) {
        Record-Result $r
    }

    if ($round -lt $Rounds) {
        Start-Sleep -Milliseconds $DelayMs
    }
}

} # end if (-not $WarmupOnly)

# ── Summary ─────────────────────────────────────────────────────
Write-Host ""
Write-Host "$($c.S)  ─── Summary ───$($c.R)"
Write-Host "$($c.M)  Total: $($stats.Total)  OK: $($c.S)$($stats.OK)$($c.M)  Errors: $($c.E)$($stats.Err)$($c.R)"
Write-Host ""

foreach ($m in $modelStats.Keys | Sort-Object) {
    $ms = $modelStats[$m]
    $avg = if ($ms.OK -gt 0) { [math]::Round($ms.TotalMs / $ms.OK) } else { 0 }
    $errInfo = if ($ms.Err -gt 0) { " $($c.E)($($ms.Err) errors)$($c.R)" } else { "" }
    Write-Host "$($c.M)  $($c.C)$m$($c.M): $($ms.OK) ok / $($ms.Err) err — avg ${avg}ms$errInfo$($c.R)"
}

# ── Final stone state ───────────────────────────────────────────
try {
    $finalStones = (Invoke-RestMethod -Uri "$Proxy/v1/stones" -TimeoutSec 5).stones
    Write-Host ""
    Write-Host "$($c.S)  ─── Stone State ───$($c.R)"
    foreach ($s in $finalStones) {
        $loaded = if ($s.models.loaded) { $s.models.loaded -join ", " } else { "none" }
        $vram = if ($s.gpu) { "VRAM $($s.gpu.vram_used_mb)/$($s.gpu.vram_total_mb) MB" } else { "no GPU" }
        Write-Host "  $($c.C)$($s.name)$($c.R) $($c.M)queue=$($s.queue_depth) loaded=[$loaded] $vram$($c.R)"
    }
} catch {}

Write-Host ""
