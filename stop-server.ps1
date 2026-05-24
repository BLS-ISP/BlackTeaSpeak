<#
.SYNOPSIS
Stops any running instances of BlackTeaSpeak Server cleanly.

.DESCRIPTION
Finds running instances of the server executable, kills them, and cleans up any hung sockets on key ports.
#>

$ErrorActionPreference = "Continue" # Don't halt on single process failures
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Definition
Set-Location $ScriptDir

# --- Styling Helpers ---
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

# 1. Kill by Executable Name
Write-Header "Searching for Server Process"
$ServerProcesses = Get-Process -Name "blackteaspeak_server" -ErrorAction SilentlyContinue

if ($ServerProcesses) {
    Write-Info "Found $($ServerProcesses.Count) running server process(es)."
    foreach ($Proc in $ServerProcesses) {
        Write-Info "Stopping server process with PID $($Proc.Id)..."
        Stop-Process -Id $Proc.Id -Force
        Write-Success "Stopped process PID $($Proc.Id)."
    }
} else {
    Write-Info "No 'blackteaspeak_server' executable processes are running."
}

# 2. Port Scour (Clean up orphaned port binds)
Write-Header "Checking for Orphaned Ports"
$ServerPorts = @(9987, 10101, 30303, 8081)
$OrphansFound = 0

foreach ($Port in $ServerPorts) {
    # Check TCP
    $TCPConn = Get-NetTCPConnection -LocalPort $Port -ErrorAction SilentlyContinue
    foreach ($Conn in $TCPConn) {
        $Pid = $Conn.OwningProcess
        if ($Pid -ne $PID -and $Pid -ne 0 -and $Pid -ne 4) { # Don't target own script, idle process, or system
            $Proc = Get-Process -Id $Pid -ErrorAction SilentlyContinue
            $ProcName = if ($Proc) { $Proc.Name } else { "Unknown" }
            Write-Warn "Port TCP/$Port is still held by PID $Pid ($ProcName)."
            Write-Info "Stopping PID $Pid ($ProcName)..."
            Stop-Process -Id $Pid -Force
            $OrphansFound++
        }
    }

    # Check UDP (Desktop voice bind)
    $UDPConn = Get-NetUDPEndpoint -LocalPort $Port -ErrorAction SilentlyContinue
    foreach ($Conn in $UDPConn) {
        $Pid = $Conn.OwningProcess
        if ($Pid -ne $PID -and $Pid -ne 0 -and $Pid -ne 4) {
            $Proc = Get-Process -Id $Pid -ErrorAction SilentlyContinue
            $ProcName = if ($Proc) { $Proc.Name } else { "Unknown" }
            Write-Warn "Port UDP/$Port is still held by PID $Pid ($ProcName)."
            Write-Info "Stopping PID $Pid ($ProcName)..."
            Stop-Process -Id $Pid -Force
            $OrphansFound++
        }
    }
}

if ($OrphansFound -gt 0) {
    Write-Success "Orphaned port bindings ($OrphansFound) terminated."
} else {
    Write-Success "All ports are clean."
}

Write-Host ""
Write-Host "Server stopped successfully." -ForegroundColor Green
