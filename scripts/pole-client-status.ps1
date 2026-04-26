param(
    [string]$ConfigPath = ".\node.json",
    [string]$RepoRoot = ".",
    [string]$BinaryPath = ".\target\debug\pole-client.exe"
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

$pidFile = Join-Path $dataDir "daemon.pid"
$stdoutLog = Join-Path $dataDir "daemon.out.log"
$stderrLog = Join-Path $dataDir "daemon.err.log"

& $binary "status" $config

if (-not (Test-Path $pidFile)) {
    Write-Output "daemon_running=false"
    exit 0
}

$nodePid = Get-Content $pidFile | Select-Object -First 1
$process = Get-Process -Id $nodePid -ErrorAction SilentlyContinue

if (-not $process) {
    Write-Output "daemon_running=false"
    Write-Output "daemon_pid=$nodePid"
    Write-Output "pid_file_stale=true"
    Write-Output "stdout_log=$stdoutLog"
    Write-Output "stderr_log=$stderrLog"
    Remove-Item $pidFile -Force
    exit 0
}

Write-Output "daemon_running=true"
Write-Output "daemon_pid=$nodePid"
Write-Output "daemon_priority_class=$($process.PriorityClass)"
Write-Output "daemon_start_time=$($process.StartTime)"
Write-Output "stdout_log=$stdoutLog"
Write-Output "stderr_log=$stderrLog"
