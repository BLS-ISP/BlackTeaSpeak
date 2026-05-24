<#
.SYNOPSIS
Stops the BlackTeaSpeak Client and kills any dev server processes.

.DESCRIPTION
Stops running Client windows and safely targets the Vite developer server on port 1420.
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
Write-Host "   BlackTeaSpeak Client Stopping...          " -ForegroundColor Red -BackgroundColor Black
Write-Host "=============================================" -ForegroundColor DarkRed

# 1. Kill Client GUI Processes
Write-Header "Stopping Client Windows"
$TargetBinaries = @("tauri-app", "BlackTeaSpeak-Client")
$StoppedCount = 0

foreach ($BinName in $TargetBinaries) {
    $Procs = Get-Process -Name $BinName -ErrorAction SilentlyContinue
    if ($Procs) {
        foreach ($Proc in $Procs) {
            Write-Info "Terminating process '$BinName' (PID: $($Proc.Id))..."
            Stop-Process -Id $Proc.Id -Force
            $StoppedCount++
        }
    }
}

if ($StoppedCount -gt 0) {
    Write-Success "Stopped $StoppedCount client GUI process(es)."
} else {
    Write-Info "No active Client GUI processes found."
}

# 2. Target Dev Server Port
Write-Header "Terminating Frontend Dev Server"
$DevPort = 1420
$ViteConn = Get-NetTCPConnection -LocalPort $DevPort -ErrorAction SilentlyContinue | Select-Object -First 1

if ($ViteConn) {
    $Pid = $ViteConn.OwningProcess
    if ($Pid -ne $PID -and $Pid -ne 0 -and $Pid -ne 4) {
        $Proc = Get-Process -Id $Pid -ErrorAction SilentlyContinue
        $ProcName = if ($Proc) { $Proc.Name } else { "Unknown" }
        
        Write-Warn "Found active Dev Server port ($DevPort) held by PID $Pid ($ProcName)."
        Write-Info "Terminating Dev Server process..."
        Stop-Process -Id $Pid -Force
        Write-Success "Dev server stopped."
    }
} else {
    Write-Info "No dev server running on port $DevPort."
}

Write-Host ""
Write-Host "Client stopped successfully." -ForegroundColor Green
