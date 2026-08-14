# PoLE 节点启动脚本（需先运行 init-genesis.ps1）
# 在项目根目录执行: .\scripts\start-node.ps1
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
Set-Location $root

if (-not (Test-Path "config/genesis.json")) {
    Write-Host "Run .\scripts\init-genesis.ps1 first."
    exit 1
}
Write-Host "Starting PoLE core node..."
go run ./cmd/core
