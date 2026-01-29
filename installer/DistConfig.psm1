<#
.SYNOPSIS
    Distribution configuration helper module

.DESCRIPTION
    Provides functions to read and work with dist.json configuration
#>

function Get-DistConfig {
    [CmdletBinding()]
    param(
        [string]$ConfigPath = (Join-Path $PSScriptRoot "dist.json")
    )
    
    if (-not (Test-Path $ConfigPath)) {
        throw "Distribution config not found: $ConfigPath"
    }
    
    $config = Get-Content $ConfigPath -Raw | ConvertFrom-Json
    
    # Resolve paths relative to config file location
    $configDir = Split-Path $ConfigPath -Parent
    $config.workspace.root = Resolve-ConfigPath $config.workspace.root $configDir
    $config.workspace.dist = Resolve-ConfigPath $config.workspace.dist $configDir
    $config.packages.outputDir = Resolve-ConfigPath $config.packages.outputDir $configDir
    $config.staging.linux = Resolve-ConfigPath $config.staging.linux $configDir
    $config.staging.windows = Resolve-ConfigPath $config.staging.windows $configDir
    
    return $config
}

function Resolve-ConfigPath {
    param(
        [string]$Path,
        [string]$BasePath
    )
    
    # Handle environment variables
    $resolved = $Path -replace '\$\{(\w+)\}', {
        param($match)
        $varName = $match.Groups[1].Value
        [Environment]::GetEnvironmentVariable($varName)
    }
    
    # Resolve relative paths
    if (-not [System.IO.Path]::IsPathRooted($resolved)) {
        $resolved = Join-Path $BasePath $resolved
    }
    
    return $resolved
}

function Get-PlatformBinaries {
    <#
    .SYNOPSIS
        Get all binaries for a platform (for packaging)
    .DESCRIPTION
        Returns all binaries configured for the platform, regardless of tier.
        Used during packaging to include all available binaries.
    #>
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [PSCustomObject]$Config,

        [Parameter(Mandatory)]
        [ValidateSet('linux', 'windows')]
        [string]$Platform
    )

    $binaries = @()

    # Add main binaries
    foreach ($key in $Config.binaries.PSObject.Properties.Name) {
        $binary = $Config.binaries.$key
        if ($binary.platforms -contains $Platform) {
            $binaries += [PSCustomObject]@{
                Name = $key
                Source = $binary.source
                Destination = $binary.destination
                Required = $binary.required
                Tier = if ($binary.tier) { $binary.tier } else { "core" }
                Type = 'binary'
            }
        }
    }

    # Add adapters (legacy support - adapters section)
    if ($Config.adapters) {
        foreach ($key in $Config.adapters.PSObject.Properties.Name) {
            $adapter = $Config.adapters.$key
            if ($adapter.platforms -contains $Platform) {
                $binaries += [PSCustomObject]@{
                    Name = $key
                    Source = $adapter.source
                    Destination = $adapter.destination
                    Required = $adapter.required
                    Tier = if ($adapter.tier) { $adapter.tier } else { "full" }
                    Type = 'adapter'
                }
            }
        }
    }

    return $binaries
}

function Get-TierBinaries {
    <#
    .SYNOPSIS
        Get binaries to BUILD for a specific tier
    .DESCRIPTION
        Returns only binaries that should be compiled for the given tier.
        - "core" tier: only moss and rake
        - "full" tier: all binaries
    #>
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [PSCustomObject]$Config,

        [Parameter(Mandatory)]
        [ValidateSet('linux', 'windows')]
        [string]$Platform,

        [Parameter(Mandatory)]
        [ValidateSet('core', 'full')]
        [string]$Tier
    )

    # Get tier definition from config
    $tierBinaries = if ($Config.tiers -and $Config.tiers.$Tier) {
        $Config.tiers.$Tier
    } else {
        # Fallback: core = moss/rake, full = everything
        if ($Tier -eq "core") { @("moss", "rake") } else { $Config.binaries.PSObject.Properties.Name }
    }

    $binaries = @()

    foreach ($key in $Config.binaries.PSObject.Properties.Name) {
        # Only include if in tier definition
        if ($tierBinaries -contains $key) {
            $binary = $Config.binaries.$key
            if ($binary.platforms -contains $Platform) {
                $binaries += [PSCustomObject]@{
                    Name = $key
                    Source = $binary.source
                    Destination = $binary.destination
                    Required = $binary.required
                    Tier = if ($binary.tier) { $binary.tier } else { "core" }
                    Type = 'binary'
                }
            }
        }
    }

    return $binaries
}

function Get-CargoBuildTargets {
    <#
    .SYNOPSIS
        Get cargo package names to build for a tier
    .DESCRIPTION
        Returns the list of cargo package names (e.g., "garden-moss") to pass to cargo build -p
    #>
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [PSCustomObject]$Config,

        [Parameter(Mandatory)]
        [ValidateSet('core', 'full')]
        [string]$Tier
    )

    $tierBinaries = if ($Config.tiers -and $Config.tiers.$Tier) {
        $Config.tiers.$Tier
    } else {
        if ($Tier -eq "core") { @("moss", "rake") } else { $Config.binaries.PSObject.Properties.Name }
    }

    $targets = @()
    foreach ($key in $tierBinaries) {
        if ($Config.binaries.$key) {
            $targets += $Config.binaries.$key.source
        }
    }

    return $targets
}

function Get-PlatformAssets {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [PSCustomObject]$Config,
        
        [Parameter(Mandatory)]
        [ValidateSet('linux', 'windows')]
        [string]$Platform
    )
    
    $assets = @()
    
    foreach ($key in $Config.assets.PSObject.Properties.Name) {
        $asset = $Config.assets.$key
        if ($asset.platforms -contains $Platform) {
            $assets += [PSCustomObject]@{
                Name = $key
                Source = $asset.source
                Destination = $asset.destination
                Recursive = $asset.recursive
                Files = $asset.files
                LineEndings = $asset.lineEndings
            }
        }
    }
    
    return $assets
}

function New-StagingDirectory {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [string]$Path,
        
        [switch]$Clean
    )
    
    if ($Clean -and (Test-Path $Path)) {
        Remove-Item $Path -Recurse -Force
    }
    
    New-Item -ItemType Directory -Path $Path -Force | Out-Null
    return $Path
}

function Copy-BinaryToStaging {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [string]$SourceDir,
        
        [Parameter(Mandatory)]
        [string]$StagingRoot,
        
        [Parameter(Mandatory)]
        [PSCustomObject]$Binary,
        
        [Parameter(Mandatory)]
        [ValidateSet('linux', 'windows')]
        [string]$Platform
    )
    
    # Determine source filename with platform-specific extension
    $extension = if ($Platform -eq 'windows') { '.exe' } else { '' }
    $sourceFilename = $Binary.Source + $extension
    $sourcePath = Join-Path $SourceDir $sourceFilename
    
    if (-not (Test-Path $sourcePath)) {
        if ($Binary.Required) {
            throw "Required binary not found: $sourcePath"
        } else {
            Write-Warning "Optional binary not found: $sourcePath"
            return $false
        }
    }
    
    # Destination is a directory - append the source filename
    $destDir = Join-Path $StagingRoot $Binary.Destination
    New-Item -ItemType Directory -Path $destDir -Force | Out-Null
    $destPath = Join-Path $destDir $sourceFilename
    
    Copy-Item $sourcePath $destPath -Force
    
    $relativePath = ($Binary.Destination + $sourceFilename) -replace '\\', '/'
    Write-Host "  + $relativePath" -ForegroundColor DarkGray
    
    return $true
}

function Copy-AssetToStaging {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [string]$WorkspaceRoot,
        
        [Parameter(Mandatory)]
        [string]$StagingRoot,
        
        [Parameter(Mandatory)]
        [PSCustomObject]$Asset
    )
    
    $sourcePath = Join-Path $WorkspaceRoot $Asset.Source
    $destPath = Join-Path $StagingRoot $Asset.Destination
    
    if (-not (Test-Path $sourcePath)) {
        Write-Warning "Asset not found: $sourcePath"
        return
    }
    
    if ($Asset.Files) {
        # Copy specific files
        New-Item -ItemType Directory -Path $destPath -Force | Out-Null
        foreach ($file in $Asset.Files) {
            $srcFile = Join-Path $sourcePath $file
            $dstFile = Join-Path $destPath $file
            
            if (Test-Path $srcFile) {
                if ($Asset.LineEndings -eq "lf") {
                    # Convert to Unix line endings
                    $content = Get-Content $srcFile -Raw
                    $content = $content -replace "`r`n", "`n"
                    [System.IO.File]::WriteAllText($dstFile, $content, [System.Text.UTF8Encoding]::new($false))
                    Write-Host "  + $($Asset.Destination)/$file (LF)" -ForegroundColor DarkGray
                } else {
                    Copy-Item $srcFile $dstFile -Force
                    Write-Host "  + $($Asset.Destination)/$file" -ForegroundColor DarkGray
                }
            }
        }
    } elseif ($Asset.Recursive) {
        # Copy entire directory
        Copy-Item $sourcePath $destPath -Recurse -Force
        $fileCount = (Get-ChildItem $destPath -Recurse -File).Count
        Write-Host "  + $($Asset.Destination)/ ($fileCount files)" -ForegroundColor DarkGray
    } else {
        # Copy single directory (non-recursive)
        Copy-Item $sourcePath $destPath -Force
        Write-Host "  + $($Asset.Destination)/" -ForegroundColor DarkGray
    }
}

Export-ModuleMember -Function @(
    'Get-DistConfig',
    'Get-PlatformBinaries',
    'Get-TierBinaries',
    'Get-CargoBuildTargets',
    'Get-PlatformAssets',
    'New-StagingDirectory',
    'Copy-BinaryToStaging',
    'Copy-AssetToStaging'
)
