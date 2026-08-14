# PoLE 创世文件生成脚本
# 在项目根目录执行: .\scripts\init-genesis.ps1
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
Set-Location $root

Write-Host "Generating genesis.json..."
go run ./cmd/genesis
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
Write-Host "Done. Genesis: config/genesis.json"
