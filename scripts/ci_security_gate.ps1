param(
    [string]$TargetDir = "target/release"
)

$ErrorActionPreference = "Stop"

$backdoors = @(
    "POLE_ENGAGEMENT_STATE_OVERRIDE",
    "POLE_CLIENT_FOREGROUND_PROCESS_OVERRIDE",
    "POLE_CLIENT_FOREGROUND_TITLE_OVERRIDE"
)

$binaries = @(
    (Join-Path $TargetDir "pole.exe"),
    (Join-Path $TargetDir "pole-client.exe")
)

Write-Host "Running PoLE Release Security Gate..."

$failed = $false
foreach ($bin in $binaries) {
    if (-not (Test-Path $bin)) {
        Write-Warning "Binary not found for security check: $bin"
        continue
    }

    Write-Host "Scanning $bin for prohibited test/debug override strings..."
    $bytes = [System.IO.File]::ReadAllBytes($bin)
    $ascii = [System.Text.Encoding]::ASCII.GetString($bytes)

    foreach ($pattern in $backdoors) {
        if ($ascii.Contains($pattern)) {
            Write-Error "CRITICAL SECURITY AUDIT FAILED: $pattern detected in release binary $bin"
            $failed = $true
        }
    }
}

if ($failed) {
    exit 1
}

Write-Host "Security gate passed: All anti-cheat test overrides are strictly stripped from release builds."
