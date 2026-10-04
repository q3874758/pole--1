param(
    [string]$ConfigPath = ".\node.json",
    [string]$RepoRoot = ".",
    [string]$BinaryPath = ".\target\debug\pole-client.exe",
    [int64]$Ticks = 0
)

$ErrorActionPreference = "Stop"

$scriptPath = Join-Path $PSScriptRoot "start-pole-client.ps1"
& $scriptPath -ConfigPath $ConfigPath -RepoRoot $RepoRoot -BinaryPath $BinaryPath -Ticks $Ticks
