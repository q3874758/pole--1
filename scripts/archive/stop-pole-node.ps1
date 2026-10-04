param(
    [string]$ConfigPath = ".\node.json",
    [string]$RepoRoot = "."
)

$ErrorActionPreference = "Stop"

$scriptPath = Join-Path $PSScriptRoot "stop-pole-client.ps1"
& $scriptPath -ConfigPath $ConfigPath -RepoRoot $RepoRoot
