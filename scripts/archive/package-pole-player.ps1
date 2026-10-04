param(
    [string]$RepoRoot = ".",
    [string]$BinaryPath = ".\target\release\pole-client.exe",
    [string]$OutputDir = ".\dist\pole-player-win64",
    [switch]$SkipBuild,
    [switch]$SkipZip
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

$resolvedRepoRoot = (Resolve-Path $RepoRoot).Path
$resolvedOutputDir = Resolve-InputPath -PathValue $OutputDir -BaseDir $resolvedRepoRoot
$scriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path

if (-not $SkipBuild) {
    Push-Location $resolvedRepoRoot
    try {
        cargo build --release --bin pole-client
    }
    finally {
        Pop-Location
    }
}

$resolvedBinaryPath = Resolve-InputPath -PathValue $BinaryPath -BaseDir $resolvedRepoRoot
if (-not (Test-Path $resolvedBinaryPath)) {
    throw "pole-client binary not found at $resolvedBinaryPath"
}

if (Test-Path $resolvedOutputDir) {
    Remove-Item $resolvedOutputDir -Recurse -Force
}
New-Item -ItemType Directory -Path $resolvedOutputDir -Force | Out-Null

Copy-Item $resolvedBinaryPath (Join-Path $resolvedOutputDir "pole-client.exe") -Force
Copy-Item (Join-Path $scriptRoot "install-pole-player.ps1") (Join-Path $resolvedOutputDir "install-pole-player.ps1") -Force
Copy-Item (Join-Path $scriptRoot "install-pole-player.cmd") (Join-Path $resolvedOutputDir "install-pole-player.cmd") -Force
Copy-Item (Join-Path $scriptRoot "pole-player-README.txt") (Join-Path $resolvedOutputDir "README.txt") -Force

$zipPath = "$resolvedOutputDir.zip"
if (-not $SkipZip) {
    if (Test-Path $zipPath) {
        Remove-Item $zipPath -Force
    }
    Compress-Archive -Path (Join-Path $resolvedOutputDir "*") -DestinationPath $zipPath
}

Write-Output "player_bundle_ready=true"
Write-Output "bundle_dir=$resolvedOutputDir"
Write-Output "binary=$resolvedBinaryPath"
Write-Output "install_entrypoint=$(Join-Path $resolvedOutputDir 'install-pole-player.cmd')"
if (-not $SkipZip) {
    Write-Output "bundle_zip=$zipPath"
}
