#Requires -Version 5.1
<#
.SYNOPSIS
  Isolated launcher for ABIFR (Agentic Billing Integrity & Financial Reconciliation).
  Starts managed PostgreSQL, Python doc-intel worker, Rust backend, and Next.js frontend.
#>
param(
    [switch]$Detached
)

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
Set-Location $Root

$RuntimeDir = Join-Path $Root ".runtime"
$LogsDir    = Join-Path $Root "logs"
$PidFile    = Join-Path $RuntimeDir "pids.json"
New-Item -ItemType Directory -Force -Path $RuntimeDir, $LogsDir | Out-Null

function New-RandomHex([int]$BytesLen) {
    $bytes = New-Object byte[] $BytesLen
    $rng = [System.Security.Cryptography.RandomNumberGenerator]::Create()
    $rng.GetBytes($bytes)
    return ([System.BitConverter]::ToString($bytes) -replace "-", "").ToLowerInvariant()
}

function Test-PortInUse([int]$Port) {
    $conn = Get-NetTCPConnection -LocalPort $Port -State Listen -ErrorAction SilentlyContinue
    return ($null -ne $conn)
}

function Wait-TcpPort([string]$HostAddr, [int]$Port, [int]$TimeoutSec) {
    $deadline = (Get-Date).AddSeconds($TimeoutSec)
    while ((Get-Date) -lt $deadline) {
        try {
            $client = New-Object System.Net.Sockets.TcpClient
            $iar = $client.BeginConnect($HostAddr, $Port, $null, $null)
            if ($iar.AsyncWaitHandle.WaitOne(500, $false)) {
                $client.EndConnect($iar)
                $client.Close()
                return $true
            }
            $client.Close()
        } catch {}
        Start-Sleep -Milliseconds 500
    }
    return $false
}

function Wait-HttpOk([string]$Url, [int]$TimeoutSec) {
    $deadline = (Get-Date).AddSeconds($TimeoutSec)
    while ((Get-Date) -lt $deadline) {
        try {
            $resp = Invoke-WebRequest -Uri $Url -UseBasicParsing -TimeoutSec 3
            if ($resp.StatusCode -eq 200) { return $true }
        } catch {}
        Start-Sleep -Milliseconds 600
    }
    return $false
}

# 1. Prevent duplicate process spawning
if (Test-Path $PidFile) {
    $existing = Get-Content $PidFile -Raw | ConvertFrom-Json
    $stillRunning = @()
    foreach ($prop in $existing.PSObject.Properties) {
        $p = Get-Process -Id $prop.Value -ErrorAction SilentlyContinue
        if ($p) { $stillRunning += "$($prop.Name) (PID $($prop.Value))" }
    }
    if ($stillRunning.Count -gt 0) {
        Write-Error "ABIFR services are already running: $($stillRunning -join ', '). Run scripts\stop.ps1 first."
        exit 1
    }
    Remove-Item $PidFile -Force
}

# 2. Check required dependencies
foreach ($cmd in @("cargo", "node", "npm", "python")) {
    if (-not (Get-Command $cmd -ErrorAction SilentlyContinue)) {
        Write-Error "Required dependency '$cmd' not found in PATH."
        exit 1
    }
}

# 3. Ensure .env exists (generate random local dev secrets without hardcoding any secret)
$EnvPath = Join-Path $Root ".env"
if (-not (Test-Path $EnvPath)) {
    Write-Host "[launcher] Creating local .env from .env.example with generated random secrets..." -ForegroundColor Cyan
    $pgPass  = New-RandomHex 16
    $jwtSec  = New-RandomHex 32
    $intTok  = New-RandomHex 24
    $admPass = "Abifr-" + (New-RandomHex 8) + "!"
    $revPass = "Abifr-" + (New-RandomHex 8) + "!"
    $anaPass = "Abifr-" + (New-RandomHex 8) + "!"
    $audPass = "Abifr-" + (New-RandomHex 8) + "!"
    $tpl = Get-Content (Join-Path $Root ".env.example") -Raw
    $tpl = $tpl.Replace("CHANGE_ME_LOCAL_DEV_PASSWORD", $pgPass)
    $tpl = $tpl.Replace("CHANGE_ME_MIN_32_CHARS_RANDOM_SECRET_KEY", $jwtSec)
    $tpl = $tpl.Replace("CHANGE_ME_INTERNAL_WORKER_SHARED_TOKEN", $intTok)
    $tpl = $tpl.Replace("CHANGE_ME_INITIAL_ADMIN_PASSWORD", $admPass)
    $tpl = $tpl.Replace("CHANGE_ME_INITIAL_REVIEWER_PASSWORD", $revPass)
    $tpl = $tpl.Replace("CHANGE_ME_INITIAL_ANALYST_PASSWORD", $anaPass)
    $tpl = $tpl.Replace("CHANGE_ME_INITIAL_AUDITOR_PASSWORD", $audPass)
    Set-Content -Path $EnvPath -Value $tpl -Encoding UTF8
    Write-Host "[launcher] Generated .env (check BOOTSTRAP_ADMIN_PASSWORD inside .env for initial sign-in)." -ForegroundColor Yellow
}

# Load .env into process environment
Get-Content $EnvPath | ForEach-Object {
    $line = $_.Trim()
    if ($line -and -not $line.StartsWith("#") -and $line.Contains("=")) {
        $idx = $line.IndexOf("=")
        $k = $line.Substring(0, $idx).Trim()
        $v = $line.Substring($idx + 1).Trim()
        [Environment]::SetEnvironmentVariable($k, $v, "Process")
    }
}

$PgHost       = if ($env:POSTGRES_HOST) { $env:POSTGRES_HOST } else { "127.0.0.1" }
$PgPort       = [int]$(if ($env:POSTGRES_PORT) { $env:POSTGRES_PORT } else { "14543" })
$DocIntelPort = [int]$(if ($env:DOC_INTEL_PORT) { $env:DOC_INTEL_PORT } else { "14900" })
$BackendPort  = [int]$(if ($env:BACKEND_PORT) { $env:BACKEND_PORT } else { "14800" })
$FrontendPort = [int]$(if ($env:FRONTEND_PORT) { $env:FRONTEND_PORT } else { "14300" })

# 4. Validate port isolation before starting
foreach ($entry in @(
    @{ Name = "PostgreSQL"; Port = $PgPort },
    @{ Name = "Doc-Intel";  Port = $DocIntelPort },
    @{ Name = "Rust Backend"; Port = $BackendPort },
    @{ Name = "Next.js Frontend"; Port = $FrontendPort }
)) {
    if (Test-PortInUse $entry.Port) {
        Write-Error "Port $($entry.Port) required by $($entry.Name) is already in use. Override in .env."
        exit 1
    }
}

# 5. Build Rust binaries if not built yet
$PgBin      = Join-Path $Root "backend\target\debug\pg-manager.exe"
$BackendBin = Join-Path $Root "backend\target\debug\abifr-backend.exe"
if (-not (Test-Path $PgBin) -or -not (Test-Path $BackendBin)) {
    Write-Host "[launcher] Building Rust binaries (pg-manager, abifr-backend)..." -ForegroundColor Cyan
    Push-Location (Join-Path $Root "backend")
    cargo build --bins
    if ($LASTEXITCODE -ne 0) { Pop-Location; Write-Error "Rust build failed."; exit 1 }
    Pop-Location
}

$PythonExe = Join-Path $Root "doc-intel\.venv\Scripts\python.exe"
if (-not (Test-Path $PythonExe)) { $PythonExe = "python" }

$Pids = @{}
function Stop-AllStarted {
    Write-Host "`n[launcher] Stopping ABIFR child processes cleanly..." -ForegroundColor Yellow
    foreach ($name in @("frontend", "backend", "doc_intel", "postgres")) {
        if ($Pids.ContainsKey($name)) {
            $null = taskkill /PID $Pids[$name] /T /F 2>&1
        }
    }
    $pgData = Join-Path $Root ".runtime\postgres\data"
    $pgCtl = Get-ChildItem -Path (Join-Path $Root ".runtime\postgres\install") -Recurse -Filter "pg_ctl.exe" -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($pgCtl -and (Test-Path (Join-Path $pgData "postmaster.pid"))) {
        & $pgCtl.FullName -D $pgData -m fast stop 2>&1 | Out-Null
    }
    if (Test-Path $PidFile) { Remove-Item $PidFile -Force -ErrorAction SilentlyContinue }
}

try {
    # 6. Start managed PostgreSQL
    Write-Host "[launcher] Starting isolated PostgreSQL 16 on ${PgHost}:${PgPort}..." -ForegroundColor Cyan
    $pgProc = Start-Process -FilePath $PgBin -WorkingDirectory $Root `
        -RedirectStandardOutput (Join-Path $LogsDir "postgres.out.log") `
        -RedirectStandardError (Join-Path $LogsDir "postgres.err.log") `
        -WindowStyle Hidden -PassThru
    $Pids["postgres"] = $pgProc.Id

    if (-not (Wait-TcpPort $PgHost $PgPort 180)) {
        throw "Managed PostgreSQL failed to start within 180s. Check logs\postgres.err.log."
    }
    Write-Host "[launcher] PostgreSQL READY on ${PgHost}:${PgPort}" -ForegroundColor Green

    # 7. Start Python Document Intelligence worker (relative script path avoids spaces in Windows args)
    Write-Host "[launcher] Starting Python Doc-Intel worker on 127.0.0.1:${DocIntelPort}..." -ForegroundColor Cyan
    $docProc = Start-Process -FilePath $PythonExe -ArgumentList "server.py" `
        -WorkingDirectory (Join-Path $Root "doc-intel") `
        -RedirectStandardOutput (Join-Path $LogsDir "doc-intel.out.log") `
        -RedirectStandardError (Join-Path $LogsDir "doc-intel.err.log") `
        -WindowStyle Hidden -PassThru
    $Pids["doc_intel"] = $docProc.Id

    if (-not (Wait-HttpOk "http://127.0.0.1:${DocIntelPort}/internal/health" 45)) {
        throw "Python Doc-Intel worker failed health check. Check logs\doc-intel.err.log."
    }
    Write-Host "[launcher] Doc-Intel READY on http://127.0.0.1:${DocIntelPort}" -ForegroundColor Green

    # 8. Start Rust Backend API
    Write-Host "[launcher] Starting Rust Backend API on 127.0.0.1:${BackendPort}..." -ForegroundColor Cyan
    $beProc = Start-Process -FilePath $BackendBin -WorkingDirectory $Root `
        -RedirectStandardOutput (Join-Path $LogsDir "backend.out.log") `
        -RedirectStandardError (Join-Path $LogsDir "backend.err.log") `
        -WindowStyle Hidden -PassThru
    $Pids["backend"] = $beProc.Id

    if (-not (Wait-HttpOk "http://127.0.0.1:${BackendPort}/api/v1/health" 45)) {
        throw "Rust Backend API failed health check. Check logs\backend.err.log."
    }
    Write-Host "[launcher] Rust Backend READY on http://127.0.0.1:${BackendPort}" -ForegroundColor Green

    # 9. Start Next.js Frontend (relative path to next bin avoids spaces and '&' in workspace paths)
    Write-Host "[launcher] Starting Next.js Frontend on 127.0.0.1:${FrontendPort}..." -ForegroundColor Cyan
    $feProc = Start-Process -FilePath "node" -ArgumentList @("node_modules\next\dist\bin\next", "start", "-H", "127.0.0.1", "-p", "$FrontendPort") `
        -WorkingDirectory (Join-Path $Root "frontend") `
        -RedirectStandardOutput (Join-Path $LogsDir "frontend.out.log") `
        -RedirectStandardError (Join-Path $LogsDir "frontend.err.log") `
        -WindowStyle Hidden -PassThru
    $Pids["frontend"] = $feProc.Id

    if (-not (Wait-HttpOk "http://127.0.0.1:${FrontendPort}" 45)) {
        throw "Next.js Frontend failed to start. Check logs\frontend.err.log."
    }

    $Pids | ConvertTo-Json | Set-Content -Path $PidFile -Encoding UTF8

    Write-Host ""
    Write-Host "==============================================================" -ForegroundColor Green
    Write-Host " ABIFR PLATFORM RUNNING (ISOLATED PORTS)" -ForegroundColor Green
    Write-Host "==============================================================" -ForegroundColor Green
    Write-Host " Frontend UI   : http://127.0.0.1:${FrontendPort}"
    Write-Host " Rust API      : http://127.0.0.1:${BackendPort}/api/v1/health"
    Write-Host " Doc-Intel     : http://127.0.0.1:${DocIntelPort}/internal/health (internal)"
    Write-Host " PostgreSQL 16 : ${PgHost}:${PgPort} (internal)"
    Write-Host " Logs          : $LogsDir"
    Write-Host " Stop command  : .\scripts\stop.ps1"
    Write-Host "==============================================================" -ForegroundColor Green

    if (-not $Detached) {
        Write-Host "Press Ctrl+C to stop all services cleanly..."
        while ($true) { Start-Sleep -Seconds 2 }
    }
} catch {
    Write-Error $_
    Stop-AllStarted
    exit 1
} finally {
    if (-not $Detached) {
        Stop-AllStarted
    }
}
