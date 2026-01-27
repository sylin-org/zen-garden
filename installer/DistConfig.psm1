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
                Type = 'binary'
            }
        }
    }
    
    # Add adapters
    foreach ($key in $Config.adapters.PSObject.Properties.Name) {
        $adapter = $Config.adapters.$key
        if ($adapter.platforms -contains $Platform) {
            $binaries += [PSCustomObject]@{
                Name = $key
                Source = $adapter.source
                Destination = $adapter.destination
                Required = $adapter.required
                Type = 'adapter'
            }
        }
    }
    
    return $binaries
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

function Write-DependenciesFile {
    <#
    .SYNOPSIS
        Write dependencies.json file to staging directory
    
    .DESCRIPTION
        Extracts platform-specific dependencies from config and writes to package
        for processing by moss-update-helper.sh during installation
    #>
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [PSCustomObject]$Config,
        
        [Parameter(Mandatory)]
        [string]$StagingRoot,
        
        [Parameter(Mandatory)]
        [ValidateSet('linux', 'windows')]
        [string]$Platform
    )
    
    # Check if dependencies key exists
    if (-not $Config.dependencies) {
        return $false
    }
    
    # Get platform-specific dependencies
    $platformDeps = $Config.dependencies.$Platform
    if (-not $platformDeps) {
        return $false
    }
    
    # Build output object with just this platform's dependencies
    $output = @{
        $Platform = @{}
    }
    
    foreach ($adapter in $platformDeps.PSObject.Properties.Name) {
        $deps = $platformDeps.$adapter
        $output[$Platform][$adapter] = @{
            apt = @($deps.apt)
            reason = $deps.reason
        }
    }
    
    $destPath = Join-Path $StagingRoot "dependencies.json"
    $output | ConvertTo-Json -Depth 5 | Set-Content $destPath -Encoding UTF8
    Write-Host "  + dependencies.json" -ForegroundColor DarkGray
    
    return $true
}

Export-ModuleMember -Function @(
    'Get-DistConfig',
    'Get-PlatformBinaries',
    'Get-PlatformAssets',
    'New-StagingDirectory',
    'Copy-BinaryToStaging',
    'Copy-AssetToStaging',
    'Write-DependenciesFile'
)
