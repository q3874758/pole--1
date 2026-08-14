# PoLE Node Launcher - Software runner with profiles
# Usage:
#   .\scripts\run.ps1                    # interactive menu
#   .\scripts\run.ps1 -Profile testnet   # run testnet
#   .\scripts\run.ps1 -Profile mainnet -OpenBrowser
#   .\scripts\run.ps1 -Profile test -Background
param(
    [ValidateSet("mainnet", "testnet", "test")]
    [string]$Profile = "",
    [switch]$OpenBrowser = $false,
    [switch]$Background = $false,
    [switch]$NoBuild = $false,
    [switch]$Mining = $false,
    [string]$DataDir = "",
    [int]$RpcPort = 9090
)

$ErrorActionPreference = "Stop"
$root = if ($PSScriptRoot) { Split-Path -Parent $PSScriptRoot } else { (Get-Location).Path }
Set-Location $root

$profiles = @{
    mainnet = @{
        Name = "Mainnet"
        Genesis = "config\mainnet\genesis.json"
        DataDir = "data\mainnet"
        Network = "mainnet"
    }
    testnet = @{
        Name = "Testnet"
        Genesis = "config\genesis.json"
        DataDir = "data\testnet"
        Network = "testnet"
    }
    test = @{
        Name = "Test (vesting)"
        Genesis = "config\genesis_vesting_test.json"
        DataDir = "data\e2e_test"
        Network = "testnet"
    }
}

function Show-Menu {
    Write-Host ""
    Write-Host "  PoLE Node Launcher" -ForegroundColor Cyan
    Write-Host "  ==================" -ForegroundColor Cyan
    Write-Host "  1) Mainnet   - config\mainnet\genesis.json" -ForegroundColor White
    Write-Host "  2) Testnet   - config\genesis.json" -ForegroundColor White
    Write-Host "  3) Test      - config\genesis_vesting_test.json (lock=0)" -ForegroundColor White
    Write-Host "  q) Quit" -ForegroundColor Gray
    Write-Host ""
    $choice = Read-Host "Select (1/2/3/q)"
    switch ($choice) {
        "1" { return "mainnet" }
        "2" { return "testnet" }
        "3" { return "test" }
        "q" { exit 0 }
        default { return "test" }
    }
}

if (-not $Profile) {
    $Profile = Show-Menu
}

$cfg = $profiles[$Profile]
if (-not $cfg) {
    Write-Host "Unknown profile: $Profile" -ForegroundColor Red
    exit 1
}

$genesisPath = Join-Path $root $cfg.Genesis
$dataDirPath = if ($DataDir) { $DataDir } else { Join-Path $root $cfg.DataDir }

if (-not (Test-Path $genesisPath)) {
    Write-Host "Genesis not found: $genesisPath" -ForegroundColor Red
    exit 1
}

# Build if needed
$exe = Join-Path $root "pole-node.exe"
if (-not $NoBuild) {
    Write-Host "Building node..." -ForegroundColor Yellow
    $null = go build -o $exe ./cmd/node 2>&1
    if ($LASTEXITCODE -ne 0) {
        Write-Host "Build failed." -ForegroundColor Red
        exit 1
    }
    Write-Host "Build OK" -ForegroundColor Green
}

$rpcArg = ":$RpcPort"
$nodeArgs = @(
    "--genesis", $genesisPath,
    "--data-dir", $dataDirPath,
    "--network", $cfg.Network,
    "--rpc-port", $rpcArg
)

# 添加挖矿模式
if ($Mining) {
    $nodeArgs += "--mining"
}

Write-Host ""
Write-Host "Starting PoLE Node [$($cfg.Name)]" -ForegroundColor Cyan
Write-Host "  Genesis:  $($cfg.Genesis)" -ForegroundColor Gray
Write-Host "  Data:     $dataDirPath" -ForegroundColor Gray
Write-Host "  RPC:      http://localhost:$RpcPort" -ForegroundColor Gray
if ($Mining) {
    Write-Host "  Mining:   Enabled (Play-to-Earn)" -ForegroundColor Cyan
}
Write-Host "  Wallet:   http://localhost:$RpcPort (open in browser)" -ForegroundColor Gray
Write-Host ""

if ($OpenBrowser) {
    Start-Job -ScriptBlock {
        Start-Sleep -Seconds 8
        Start-Process "http://127.0.0.1:$using:RpcPort"
    } | Out-Null
}

if ($Background) {
    Start-Process -FilePath $exe -ArgumentList $nodeArgs -WorkingDirectory $root -WindowStyle Normal
    Write-Host "Node started in background. Use .\scripts\check_node.ps1 to verify." -ForegroundColor Green
    exit 0
}

& $exe @nodeArgs
