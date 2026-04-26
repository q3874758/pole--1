param(
    [string]$ConfigPath = ".\node.json",
    [string]$RepoRoot = ".",
    [string]$BinaryPath = ".\target\debug\pole-client.exe"
)

$ErrorActionPreference = "Stop"

$scriptPath = Join-Path $PSScriptRoot "pole-client-status.ps1"
& $scriptPath -ConfigPath $ConfigPath -RepoRoot $RepoRoot -BinaryPath $BinaryPath
