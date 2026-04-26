param(
    [string]$ConfigPath = ".\node.json",
    [string]$RepoRoot = "."
)

$ErrorActionPreference = "Stop"

$repo = (Resolve-Path $RepoRoot).Path
if ([System.IO.Path]::IsPathRooted($ConfigPath)) {
    $configInput = $ConfigPath
} else {
    $configInput = Join-Path $repo $ConfigPath
}

$config = (Resolve-Path $configInput).Path
$configDir = Split-Path $config -Parent
$configJson = Get-Content $config -Raw | ConvertFrom-Json
$dataDirSetting = $configJson.runtime.data_dir
if ([System.IO.Path]::IsPathRooted($dataDirSetting)) {
    $dataDir = $dataDirSetting
} else {
    $dataDir = [System.IO.Path]::GetFullPath((Join-Path $configDir $dataDirSetting))
}

$pidFile = Join-Path $dataDir "daemon.pid"

if (-not (Test-Path $pidFile)) {
    Write-Output "pole-client is not running (no pid file)"
    exit 0
}

$nodePid = Get-Content $pidFile | Select-Object -First 1
if (-not $nodePid) {
    Remove-Item $pidFile -Force
    Write-Output "pole-client pid file was empty"
    exit 0
}

$process = Get-Process -Id $nodePid -ErrorAction SilentlyContinue
if ($process) {
    Stop-Process -Id $nodePid
    Write-Output "stopped pole-client PID $nodePid"
} else {
    Write-Output "pole-client PID $nodePid was not running"
}

Remove-Item $pidFile -Force
