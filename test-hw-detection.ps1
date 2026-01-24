#!/usr/bin/env pwsh
# Test hardware detection with actual stone dmidecode values

$ErrorActionPreference = "Stop"

Write-Host "Testing Hardware Detection..." -ForegroundColor Cyan
Write-Host ""

# Test data from actual stones
$testCases = @(
    @{
        Name = "stone-bronze-canyon (Celeron J4105)"
        Manufacturer = "Dell Inc."
        ProductName = "Wyse 5070 Thin Client"
        Baseboard = "02DXT3"
        ExpectedMatch = $true
    },
    @{
        Name = "stone-crystal-forest (Pentium J5005)"
        Manufacturer = "Dell Inc."
        ProductName = "Wyse 5070 Thin Client"
        Baseboard = "0PC10G"
        ExpectedMatch = $true
    },
    @{
        Name = "stone-coral-prairie (Pentium J5005)"
        Manufacturer = "Dell Inc."
        ProductName = "Wyse 5070 Thin Client"
        Baseboard = "02D0WN"
        ExpectedMatch = $true
    },
    @{
        Name = "Unknown HP device"
        Manufacturer = "HP Inc."
        ProductName = "t630 Thin Client"
        Baseboard = "UNKNOWN"
        ExpectedMatch = $false
    }
)

# Load and parse the manifest
$manifestPath = "manifests/hw/dell/wyse-5070.manifest.yaml"
$manifest = Get-Content $manifestPath -Raw

Write-Host "Loaded manifest: $manifestPath" -ForegroundColor Green
Write-Host "Manifest size: $($manifest.Length) bytes"
Write-Host ""

# Extract identity patterns from YAML
$manufacturer = if ($manifest -match 'system_manufacturer:\s*"([^"]+)"') { $matches[1] } else { "" }
$productPatterns = @()
if ($manifest -match '(?s)system_product_name_patterns:(.*?)system_version_patterns:') {
    $patternBlock = $matches[1]
    $productPatterns = [regex]::Matches($patternBlock, '"([^"]+)"') | ForEach-Object { $_.Groups[1].Value }
}

Write-Host "Manifest Identity:" -ForegroundColor Yellow
Write-Host "  Manufacturer: $manufacturer"
Write-Host "  Product Patterns: $($productPatterns -join ', ')"
Write-Host ""

# Test each case
$passed = 0
$failed = 0

foreach ($test in $testCases) {
    $matches = $false
    
    # Check manufacturer
    if ($test.Manufacturer -match $manufacturer) {
        # Check product name against patterns
        foreach ($pattern in $productPatterns) {
            if ($test.ProductName -match [regex]::Escape($pattern)) {
                $matches = $true
                break
            }
        }
    }
    
    $result = if ($matches -eq $test.ExpectedMatch) {
        $passed++
        "[PASS]"
    } else {
        $failed++
        "[FAIL]"
    }
    
    $color = if ($matches -eq $test.ExpectedMatch) { "Green" } else { "Red" }
    
    Write-Host "$result $($test.Name)" -ForegroundColor $color
    Write-Host "      Manufacturer: $($test.Manufacturer)" -ForegroundColor Gray
    Write-Host "      Product: $($test.ProductName)" -ForegroundColor Gray
    Write-Host "      Matched: $matches (Expected: $($test.ExpectedMatch))" -ForegroundColor Gray
    Write-Host ""
}

Write-Host "Results: $passed passed, $failed failed" -ForegroundColor $(if ($failed -eq 0) { "Green" } else { "Red" })

if ($failed -gt 0) {
    exit 1
}
