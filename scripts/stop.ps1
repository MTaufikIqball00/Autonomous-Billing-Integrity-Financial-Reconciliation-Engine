#Requires -Version 5.1
$Root = Split-Path -Parent $PSScriptRoot
$PidFile = Join-Path $Root ".runtime\pids.json"

if (Test-Path $PidFile) {
    $pids = Get-Content $PidFile -Raw | ConvertFrom-Json
    foreach ($name in @("frontend", "backend", "doc_intel", "postgres")) {
        $pidVal = $pids.$name
        if ($pidVal) {
            $null = taskkill /PID $pidVal /T /F 2>&1
            Write-Host "[stop] Stopped $name (PID $pidVal)"
        }
    }
    Remove-Item $PidFile -Force -ErrorAction SilentlyContinue
}

# Cleanly stop managed PostgreSQL daemon if postmaster.pid exists
$pgData = Join-Path $Root ".runtime\postgres\data"
$pgCtl = Get-ChildItem -Path (Join-Path $Root ".runtime\postgres\install") -Recurse -Filter "pg_ctl.exe" -ErrorAction SilentlyContinue | Select-Object -First 1
if ($pgCtl -and (Test-Path (Join-Path $pgData "postmaster.pid"))) {
    & $pgCtl.FullName -D $pgData -m fast stop 2>&1 | Out-Null
    Write-Host "[stop] Managed PostgreSQL stopped cleanly."
}

Write-Host "[stop] All ABIFR services stopped."
