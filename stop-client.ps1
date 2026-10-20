<#
.SYNOPSIS
Stops the BlackTeaSpeak Client and kills any dev server processes.
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
Write-Host "   BlackTeaSpeak Client Stopping...          " -ForegroundColor Red -BackgroundColor Black
Write-Host "=============================================" -ForegroundColor DarkRed

# 0. Check saved Client Dev PID file if present
$StoppedCount = 0
$PidFile = Join-Path $ScriptDir ".client.pid"
if (Test-Path $PidFile) {
    $SavedPid = (Get-Content $PidFile -ErrorAction SilentlyContinue | Select-Object -First 1).Trim()
    if ($SavedPid -match '^\d+$') {
        $SavedProc = Get-Process -Id ([int]$SavedPid) -ErrorAction SilentlyContinue
        if ($SavedProc) {
            Write-Info "Terminating tracked Client process tree (PID $SavedPid)..."
            & taskkill /F /T /PID $SavedPid 2>&1 | Out-Null
            Stop-Process -Id ([int]$SavedPid) -Force -ErrorAction SilentlyContinue
            $StoppedCount++
        }
    }
    Remove-Item $PidFile -Force -ErrorAction SilentlyContinue
}

# 1. Kill Client GUI Processes
Write-Header "Stopping Client Windows"
$TargetBinaries = @("tauri-app", "BlackTeaSpeak-Client")

foreach ($BinName in $TargetBinaries) {
    $Procs = Get-Process -Name $BinName -ErrorAction SilentlyContinue
    if ($Procs) {
        foreach ($Proc in $Procs) {
            Write-Info "Terminating process '$BinName' (PID: $($Proc.Id))..."
            & taskkill /F /T /PID $Proc.Id 2>&1 | Out-Null
            Stop-Process -Id $Proc.Id -Force -ErrorAction SilentlyContinue
            $StoppedCount++
        }
    }
}

# 2. Target Dev Server Port 1420
Write-Header "Terminating Frontend Dev Server"
$DevPort = 1420
$ViteConns = Get-NetTCPConnection -State Listen -ErrorAction SilentlyContinue | Where-Object { $_.LocalPort -eq $DevPort }

if ($ViteConns) {
    foreach ($Conn in $ViteConns) {
        $OwnerPid = $Conn.OwningProcess
        if ($OwnerPid -and $OwnerPid -ne $PID -and $OwnerPid -ne 0 -and $OwnerPid -ne 4) {
            $Proc = Get-Process -Id $OwnerPid -ErrorAction SilentlyContinue
            if ($Proc) {
                $ProcName = $Proc.Name
                Write-Warn "Found active Dev Server port ($DevPort) held by PID $OwnerPid ($ProcName)."
                Write-Info "Terminating Dev Server process tree..."
                & taskkill /F /T /PID $OwnerPid 2>&1 | Out-Null
                Stop-Process -Id $OwnerPid -Force -ErrorAction SilentlyContinue
                $StoppedCount++
                Write-Success "Dev server stopped."
            }
        }
    }
} else {
    Write-Info "No dev server running on port $DevPort."
}

if ($StoppedCount -gt 0) {
    Write-Success "Stopped $StoppedCount client process(es)."
} else {
    Write-Info "No active Client GUI processes found."
}

Write-Host ""
Write-Host "Client stopped successfully." -ForegroundColor Green
