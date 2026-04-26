param(
    [string]$ConfigPath = ".\node.json",
    [string]$RepoRoot = ".",
    [string]$BinaryPath = ".\target\debug\pole-client.exe",
    [int64]$Ticks = 0
)

$ErrorActionPreference = "Stop"

$repo = (Resolve-Path $RepoRoot).Path
if ([System.IO.Path]::IsPathRooted($ConfigPath)) {
    $configInput = $ConfigPath
} else {
    $configInput = Join-Path $repo $ConfigPath
}
if ([System.IO.Path]::IsPathRooted($BinaryPath)) {
    $binaryInput = $BinaryPath
} else {
    $binaryInput = Join-Path $repo $BinaryPath
}

$config = (Resolve-Path $configInput).Path
$binary = (Resolve-Path $binaryInput).Path

$configDir = Split-Path $config -Parent
$configJson = Get-Content $config -Raw | ConvertFrom-Json
$dataDirSetting = $configJson.runtime.data_dir
if ([System.IO.Path]::IsPathRooted($dataDirSetting)) {
    $dataDir = $dataDirSetting
} else {
    $dataDir = [System.IO.Path]::GetFullPath((Join-Path $configDir $dataDirSetting))
}

if (-not (Test-Path $dataDir)) {
    New-Item -ItemType Directory -Path $dataDir | Out-Null
}

$pidFile = Join-Path $dataDir "daemon.pid"
$stdoutLog = Join-Path $dataDir "daemon.out.log"
$stderrLog = Join-Path $dataDir "daemon.err.log"

if (Test-Path $pidFile) {
    $existingPid = Get-Content $pidFile | Select-Object -First 1
    if ($existingPid) {
        $existingProcess = Get-Process -Id $existingPid -ErrorAction SilentlyContinue
        if ($existingProcess) {
            Write-Output "pole-client already running with PID $existingPid"
            exit 0
        }
    }
    Remove-Item $pidFile -Force
}

$arguments = @("watch", $config)
if ($Ticks -gt 0) {
    $arguments += "$Ticks"
}

$process = Start-Process -FilePath $binary `
    -ArgumentList $arguments `
    -WorkingDirectory $configDir `
    -WindowStyle Hidden `
    -RedirectStandardOutput $stdoutLog `
    -RedirectStandardError $stderrLog `
    -PassThru

$priorityRequested = $true
if ($null -ne $configJson.runtime.os_background_priority) {
    $priorityRequested = [bool]$configJson.runtime.os_background_priority
}

if ($priorityRequested) {
    try {
        $process.PriorityClass = "Idle"
    } catch {
        Write-Output "priority_hint_applied=false"
    }
}

Set-Content -Path $pidFile -Value $process.Id
Write-Output "started pole-client with PID $($process.Id)"
Write-Output "config=$config"
Write-Output "data_dir=$dataDir"
if ($priorityRequested) {
    Write-Output "priority_class=$($process.PriorityClass)"
}
Write-Output "stdout_log=$stdoutLog"
Write-Output "stderr_log=$stderrLog"
