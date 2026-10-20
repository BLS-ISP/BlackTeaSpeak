<#
.SYNOPSIS
Stops any running instances of BlackTeaSpeak Server cleanly.
#>

$ErrorActionPreference = "Continue"
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Definition
Set-Location $ScriptDir

function Write-Header ([string]$text) {
    Write-Host ""
    Write-Host "=== $text ===" -ForegroundColor Cyan
}

function Write-Success ([string]$text) {
    Write-Host "[OK] $text" -ForegroundColor Green
}

function Write-Info ([string]$text) {
    Write-Host "[INFO] $text" -ForegroundColor Gray
}

function Write-Warn ([string]$text) {
    Write-Host "[WARN] $text" -ForegroundColor Yellow
}

Write-Host "=============================================" -ForegroundColor DarkRed
Write-Host "   BlackTeaSpeak Server Stopping...          " -ForegroundColor Red -BackgroundColor Black
Write-Host "=============================================" -ForegroundColor DarkRed

# 0. Check saved PID file if present
$PidFile = Join-Path $ScriptDir ".server.pid"
if (Test-Path $PidFile) {
    $SavedPid = (Get-Content $PidFile -ErrorAction SilentlyContinue | Select-Object -First 1).Trim()
    if ($SavedPid -match '^\d+$') {
        $SavedProc = Get-Process -Id ([int]$SavedPid) -ErrorAction SilentlyContinue
        if ($SavedProc) {
            Write-Info "Stopping tracked server process (PID $SavedPid)..."
            & taskkill /F /T /PID $SavedPid 2>&1 | Out-Null
            Stop-Process -Id ([int]$SavedPid) -Force -ErrorAction SilentlyContinue
        }
    }
    Remove-Item $PidFile -Force -ErrorAction SilentlyContinue
}

# 1. Kill by Executable Name
Write-Header "Searching for Server Process"
$ServerProcesses = Get-Process -Name "blackteaspeak_server" -ErrorAction SilentlyContinue

if ($ServerProcesses) {
    Write-Info "Found $(@($ServerProcesses).Count) running server process(es)."
    foreach ($Proc in $ServerProcesses) {
        Write-Info "Stopping server process with PID $($Proc.Id)..."
        & taskkill /F /T /PID $Proc.Id 2>&1 | Out-Null
        Stop-Process -Id $Proc.Id -Force -ErrorAction SilentlyContinue
        Write-Success "Stopped process PID $($Proc.Id)."
    }
} else {
    Write-Info "No 'blackteaspeak_server' executable processes are running."
}

# 2. Port Scour (Single fast query instead of per-port WMI loop)
Write-Header "Checking for Orphaned Ports"
$ServerTcpPorts = @(8080, 8081, 9987, 9988, 10011, 10022, 10101, 30303, 41144)
$ServerUdpPorts = @(9987, 9988, 9989)
$OrphansFound = 0

$TcpConns = Get-NetTCPConnection -State Listen -ErrorAction SilentlyContinue | Where-Object { $_.LocalPort -in $ServerTcpPorts }
foreach ($Conn in $TcpConns) {
    $OwnerPid = $Conn.OwningProcess
    if ($OwnerPid -and $OwnerPid -ne $PID -and $OwnerPid -ne 0 -and $OwnerPid -ne 4) {
        $Proc = Get-Process -Id $OwnerPid -ErrorAction SilentlyContinue
        if ($Proc) {
            Write-Warn "Port TCP/$($Conn.LocalPort) is still held by PID $OwnerPid ($($Proc.Name))."
            Stop-Process -Id $OwnerPid -Force -ErrorAction SilentlyContinue
            $OrphansFound++
        }
    }
}

$UdpConns = Get-NetUDPEndpoint -ErrorAction SilentlyContinue | Where-Object { $_.LocalPort -in $ServerUdpPorts }
foreach ($Conn in $UdpConns) {
    $OwnerPid = $Conn.OwningProcess
    if ($OwnerPid -and $OwnerPid -ne $PID -and $OwnerPid -ne 0 -and $OwnerPid -ne 4) {
        $Proc = Get-Process -Id $OwnerPid -ErrorAction SilentlyContinue
        if ($Proc) {
            Write-Warn "Port UDP/$($Conn.LocalPort) is still held by PID $OwnerPid ($($Proc.Name))."
            Stop-Process -Id $OwnerPid -Force -ErrorAction SilentlyContinue
            $OrphansFound++
        }
    }
}

if ($OrphansFound -gt 0) {
    Write-Success "Orphaned port bindings ($OrphansFound) terminated."
} else {
    Write-Success "All server ports are clean."
}

Write-Host ""
Write-Host "Server stopped successfully." -ForegroundColor Green
