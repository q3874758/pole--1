param(
    [string]$SourceBinaryPath = ".\pole-client.exe",
    [string]$InstallRoot = "",
    [string]$ConfigPath = ""
)

$ErrorActionPreference = "Stop"

function Resolve-InputPath {
    param(
        [Parameter(Mandatory = $true)]
        [string]$PathValue,
        [Parameter(Mandatory = $true)]
        [string]$BaseDir
    )

    if ([System.IO.Path]::IsPathRooted($PathValue)) {
        return [System.IO.Path]::GetFullPath($PathValue)
    }

    return [System.IO.Path]::GetFullPath((Join-Path $BaseDir $PathValue))
}

function Resolve-ConfigDataDir {
    param(
        [Parameter(Mandatory = $true)]
        [string]$ResolvedConfigPath
    )

    if (-not (Test-Path $ResolvedConfigPath)) {
        return $null
    }

    $configDir = Split-Path $ResolvedConfigPath -Parent
    $configJson = Get-Content $ResolvedConfigPath -Raw | ConvertFrom-Json
    $dataDirSetting = $configJson.runtime.data_dir
    if ([string]::IsNullOrWhiteSpace($dataDirSetting)) {
        return $null
    }

    if ([System.IO.Path]::IsPathRooted($dataDirSetting)) {
        return [System.IO.Path]::GetFullPath($dataDirSetting)
    }

    return [System.IO.Path]::GetFullPath((Join-Path $configDir $dataDirSetting))
}

function Stop-ExistingDaemon {
    param(
        [Parameter(Mandatory = $true)]
        [string]$ResolvedConfigPath
    )

    $dataDir = Resolve-ConfigDataDir -ResolvedConfigPath $ResolvedConfigPath
    if ([string]::IsNullOrWhiteSpace($dataDir)) {
        return
    }

    $pidFile = Join-Path $dataDir "daemon.pid"
    if (-not (Test-Path $pidFile)) {
        return
    }

    $existingPid = Get-Content $pidFile | Select-Object -First 1
    if ($existingPid) {
        $existingProcess = Get-Process -Id $existingPid -ErrorAction SilentlyContinue
        if ($existingProcess) {
            Stop-Process -Id $existingPid -Force
            Write-Output "stopped_existing_daemon_pid=$existingPid"
        }
    }

    Remove-Item $pidFile -Force -ErrorAction SilentlyContinue
}

function Copy-OptionalArtifact {
    param(
        [Parameter(Mandatory = $true)]
        [string]$SourcePath,
        [Parameter(Mandatory = $true)]
        [string]$DestinationPath
    )

    if (Test-Path $SourcePath) {
        Copy-Item $SourcePath $DestinationPath -Force
    }
}

$scriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$resolvedSourceBinary = Resolve-InputPath -PathValue $SourceBinaryPath -BaseDir $scriptRoot

if (-not (Test-Path $resolvedSourceBinary)) {
    throw "pole-client binary not found at $resolvedSourceBinary"
}

if ([string]::IsNullOrWhiteSpace($InstallRoot)) {
    if ([string]::IsNullOrWhiteSpace($env:LOCALAPPDATA)) {
        throw "LOCALAPPDATA is not set; cannot determine install root"
    }
    $InstallRoot = Join-Path $env:LOCALAPPDATA "PoLE\player-app"
}

if ([string]::IsNullOrWhiteSpace($ConfigPath)) {
    if ([string]::IsNullOrWhiteSpace($env:LOCALAPPDATA)) {
        throw "LOCALAPPDATA is not set; cannot determine player config path"
    }
    $ConfigPath = Join-Path $env:LOCALAPPDATA "PoLE\player\node.json"
}

$resolvedInstallRoot = [System.IO.Path]::GetFullPath($InstallRoot)
$resolvedConfigPath = [System.IO.Path]::GetFullPath($ConfigPath)
$installedBinary = Join-Path $resolvedInstallRoot "pole-client.exe"
$stagedBinary = Join-Path $resolvedInstallRoot "pole-client.exe.new"

New-Item -ItemType Directory -Path $resolvedInstallRoot -Force | Out-Null
Stop-ExistingDaemon -ResolvedConfigPath $resolvedConfigPath

Copy-Item $resolvedSourceBinary $stagedBinary -Force
if (Test-Path $installedBinary) {
    Remove-Item $installedBinary -Force
}
Move-Item $stagedBinary $installedBinary -Force

Copy-OptionalArtifact `
    -SourcePath (Join-Path $scriptRoot "pole-player-README.txt") `
    -DestinationPath (Join-Path $resolvedInstallRoot "README.txt")

& $installedBinary "player-start" $resolvedConfigPath

Write-Output "player_install_completed=true"
Write-Output "installed_binary=$installedBinary"
Write-Output "config_path=$resolvedConfigPath"
Write-Output "install_root=$resolvedInstallRoot"
