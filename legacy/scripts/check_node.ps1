# Quick check: is the PoLE node running?
# Usage: .\scripts\check_node.ps1   or   .\scripts\check_node.ps1 -Port 9090
param([int]$Port = 9090)
$url = "http://127.0.0.1:$Port/status"
try {
    $r = Invoke-RestMethod -Uri $url -TimeoutSec 3 -ErrorAction Stop
    if ($r.success) {
        Write-Host "Node is RUNNING" -ForegroundColor Green
        Write-Host "  chain_id: $($r.data.chain_id)"
        Write-Host "  height:   $($r.data.height)"
        exit 0
    }
} catch {}
Write-Host "Node is NOT reachable at $url" -ForegroundColor Red
exit 1
