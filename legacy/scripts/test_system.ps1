# PoLE Full System E2E Test
# Run from repo root: .\scripts\test_system.ps1
# With node already running: .\scripts\test_system.ps1 -ChecksOnly
param([switch]$ChecksOnly = $false)

$ErrorActionPreference = "Stop"
# Repo root = parent of scripts folder
$root = if ($PSScriptRoot) { Split-Path -Parent $PSScriptRoot } else { Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path) }
Set-Location $root

$dataDir = "data\e2e_test"
$genesis  = "config\genesis_vesting_test.json"
$baseUrl  = "http://127.0.0.1:9090"
$teamAddr = "dfdb4bdd50f5fa6c499461c78bfc69aa645a281e"
$genesisPath = Join-Path $root $genesis
$dataDirPath = Join-Path $root $dataDir

Write-Host "========================================" -ForegroundColor Cyan
Write-Host "  PoLE Full System Test" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan
Write-Host ""

$proc = $null
if (-not $ChecksOnly) {
    # 1. Build
    Write-Host "[1/5] Building node..." -ForegroundColor Yellow
    $buildOut = go build -o pole-node.exe ./cmd/node 2>&1
    if ($LASTEXITCODE -ne 0) {
        Write-Host "Build failed: $buildOut" -ForegroundColor Red
        exit 1
    }
    Write-Host "    OK" -ForegroundColor Green

    # 2. Clean test data
    Write-Host "[2/5] Cleaning test data..." -ForegroundColor Yellow
    Remove-Item -Recurse -Force $dataDir -ErrorAction SilentlyContinue
    New-Item -ItemType Directory -Path $dataDir -Force | Out-Null
    Write-Host "    OK" -ForegroundColor Green

    # 3. Start node (background)
    Write-Host "[3/5] Starting node..." -ForegroundColor Yellow
    $nodeExe = Join-Path $root "pole-node.exe"
    $proc = Start-Process -FilePath $nodeExe -ArgumentList "--genesis",$genesisPath,"--data-dir",$dataDirPath,"--rpc-port",":9090","--network","testnet" -WorkingDirectory $root -PassThru

    # Wait for RPC
    $maxWait = 50
    $step = 2
    $ready = $false
    for ($i = 0; $i -lt $maxWait; $i += $step) {
        Start-Sleep -Seconds $step
        try {
            $r = Invoke-WebRequest -Uri "$baseUrl/status" -UseBasicParsing -TimeoutSec 3 -ErrorAction Stop
            $j = $r.Content | ConvertFrom-Json
            if ($r.StatusCode -eq 200 -and $j.success) { $ready = $true; break }
        } catch {}
        if ($proc.HasExited) {
            Write-Host "    Node exited with code $($proc.ExitCode)" -ForegroundColor Red
            exit 1
        }
    }
    if (-not $ready) {
        Write-Host "    RPC not ready in $maxWait s. Run with -ChecksOnly when node is already up." -ForegroundColor Red
        try { $proc.Kill() } catch {}
        exit 1
    }
    Write-Host "    Node ready in $i s" -ForegroundColor Green
} else {
    # ChecksOnly: verify RPC is up (/status returns 200 when node is running)
    try {
        $r = Invoke-WebRequest -Uri "$baseUrl/status" -UseBasicParsing -TimeoutSec 3 -ErrorAction Stop
        $j = $r.Content | ConvertFrom-Json
        if (-not $j.success) { throw "status.success not true" }
    } catch {
        Write-Host "RPC not reachable at $baseUrl. Start node first." -ForegroundColor Red
        exit 1
    }
    Write-Host "Node already running, running checks only." -ForegroundColor Cyan
    Write-Host ""
}

$failed = 0

# 4. RPC checks
Write-Host "[4/5] Checking RPC and vesting..." -ForegroundColor Yellow

try {
    $status = Invoke-RestMethod -Uri "$baseUrl/status" -Method Get
    if (-not $status.success) { throw "status.success != true" }
    if (-not $status.data.chain_id) { throw "missing chain_id" }
    Write-Host "    GET /status OK" -ForegroundColor Green
} catch {
    Write-Host "    GET /status FAIL: $_" -ForegroundColor Red
    $failed++
}

try {
    $r = Invoke-WebRequest -Uri "$baseUrl/health" -UseBasicParsing -TimeoutSec 3 -ErrorAction Stop
    $health = $r.Content | ConvertFrom-Json
    Write-Host "    GET /health OK" -ForegroundColor Green
} catch {
    if ($_.Exception.Response.StatusCode.value__ -eq 503) { Write-Host "    GET /health OK (503, height=0)" -ForegroundColor Green }
    else { Write-Host "    GET /health FAIL: $_" -ForegroundColor Red; $failed++ }
}

try {
    $metrics = Invoke-WebRequest -Uri "$baseUrl/metrics" -UseBasicParsing | Select-Object -ExpandProperty Content
    if ($metrics -notmatch "pole_block_height") { throw "metrics missing pole_block_height" }
    Write-Host "    GET /metrics OK" -ForegroundColor Green
} catch {
    Write-Host "    GET /metrics FAIL: $_" -ForegroundColor Red
    $failed++
}

try {
    $vest = Invoke-RestMethod -Uri "$baseUrl/vesting/status?address=$teamAddr" -Method Get
    if (-not $vest.success) { throw "vesting status success != true" }
    if (-not $vest.data.has_schedule) { throw "expected has_schedule true for team address" }
    Write-Host "    GET /vesting/status OK" -ForegroundColor Green
} catch {
    Write-Host "    GET /vesting/status FAIL: $_" -ForegroundColor Red
    $failed++
}

try {
    $body = @{ address = $teamAddr } | ConvertTo-Json
    $claim = Invoke-RestMethod -Uri "$baseUrl/vesting/claim" -Method Post -Body $body -ContentType "application/json"
    if (-not $claim.success) { throw $claim.error }
    Write-Host "    POST /vesting/claim OK" -ForegroundColor Green
} catch {
    Write-Host "    POST /vesting/claim FAIL: $_" -ForegroundColor Red
    $failed++
}

try {
    $accs = Invoke-RestMethod -Uri "$baseUrl/wallet/accounts" -Method Get
    Write-Host "    GET /wallet/accounts OK" -ForegroundColor Green
} catch {
    Write-Host "    GET /wallet/accounts FAIL: $_" -ForegroundColor Red
    $failed++
}

try {
    $miningStatus = Invoke-RestMethod -Uri "$baseUrl/mining/status" -Method Get
    if (-not $miningStatus.success) { throw "mining/status success != true" }
    Write-Host "    GET /mining/status OK" -ForegroundColor Green
} catch {
    Write-Host "    GET /mining/status FAIL: $_" -ForegroundColor Red
    $failed++
}

try {
    $miningBal = Invoke-RestMethod -Uri "$baseUrl/mining/balance?address=$teamAddr" -Method Get
    if (-not $miningBal.success) { throw "mining/balance success != true" }
    Write-Host "    GET /mining/balance OK" -ForegroundColor Green
} catch {
    Write-Host "    GET /mining/balance FAIL: $_" -ForegroundColor Red
    $failed++
}

# 5. Stop node (if we started it)
if ($null -ne $proc -and -not $proc.HasExited) {
    Write-Host "[5/5] Stopping node..." -ForegroundColor Yellow
    $proc.Kill()
    Start-Sleep -Seconds 1
    Write-Host "    OK" -ForegroundColor Green
} else {
    Write-Host "[5/5] Skipped (ChecksOnly or node already stopped)" -ForegroundColor Gray
}

Write-Host ""
if ($failed -eq 0) {
    Write-Host "========================================" -ForegroundColor Green
    Write-Host "  All passed" -ForegroundColor Green
    Write-Host "========================================" -ForegroundColor Green
    exit 0
} else {
    Write-Host "========================================" -ForegroundColor Red
    Write-Host "  Failed: $failed" -ForegroundColor Red
    Write-Host "========================================" -ForegroundColor Red
    exit 1
}
