<#
.SYNOPSIS
Starts both the BlackTeaSpeak Server and Client concurrently.

.PARAMETER Mode
The mode to start the Client. 'Dev' (Default) or 'Prod'.
.PARAMETER RebuildServer
Force a rebuild of the Server release binary.
.PARAMETER BuildClient
Force a rebuild of the Client production bundle.
.PARAMETER Force
Forcefully kill any running instances or port bindings before starting.
#>

param (
    [ValidateSet("Dev", "Prod")]
    [string]$Mode = "Dev",
    [switch]$RebuildServer,
    [switch]$BuildClient,
    [switch]$Force
)

$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Definition
Set-Location $ScriptDir

function Write-Header ([string]$text) {
    Write-Host ""
    Write-Host "=== $text ===" -ForegroundColor Cyan
}

Clear-Host
$Banner = @'

  ____  _             _    _____            ____                  _    
 |  _ \| |           | |  |_   _|          / ___|                | |   
 | |_) | | __ _  ___| | __  | | ___  __ _  \___ \ _ __   ___  __ _| | __
 |  _ <| |/ _` |/ __| |/ /  | |/ _ \/ _` |  ___) | '_ \ / _ \/ _` | |/ /
 | |_) | | (_| | (__|   <   | |  __/ (_| | |____/| |_) |  __/ (_| |   < 
 |____/|_|\__,_|\___|_|\_\  |_|\___|\__,_|       | .__/ \___|\__,_|_|\_\
                                                 |_|                   
'@
Write-Host $Banner -ForegroundColor Yellow

Write-Host "=======================================================================" -ForegroundColor DarkCyan
Write-Host "             BlackTeaSpeak Full Stack Control System                    " -ForegroundColor White -BackgroundColor DarkCyan
Write-Host "=======================================================================" -ForegroundColor DarkCyan

$StartServerPath = Join-Path $ScriptDir "start-server.ps1"
$StartClientPath = Join-Path $ScriptDir "start-client.ps1"

# 1. Run Server Start Script
Write-Header "Step 1: Initializing BlackTeaSpeak Server"
$ServerArgs = @{}
if ($RebuildServer) { $ServerArgs["Rebuild"] = $true }
if ($Force) { $ServerArgs["Force"] = $true }

& $StartServerPath @ServerArgs

# 2. Run Client Start Script
Write-Header "Step 2: Initializing BlackTeaSpeak Client"
$ClientArgs = @{
    Mode = $Mode
}
if ($BuildClient) { $ClientArgs["Build"] = $true }
if ($Force) { $ClientArgs["Force"] = $true }

& $StartClientPath @ClientArgs

# 3. Print Unified Dashboard
Write-Host "`n=======================================================================" -ForegroundColor DarkCyan
Write-Host "                    SERVICE ENDPOINTS DASHBOARD                        " -ForegroundColor White -BackgroundColor DarkCyan
Write-Host "=======================================================================" -ForegroundColor DarkCyan
Write-Host "   Service Name             | Endpoint                      | Status    " -ForegroundColor Gray
Write-Host "  --------------------------+-------------------------------+-----------" -ForegroundColor Gray

if ($Mode -eq "Dev") {
    Write-Host "   Client Dev Server        | " -NoNewline; Write-Host "http://localhost:1420" -ForegroundColor Cyan -NoNewline; Write-Host "         | Running" -ForegroundColor Green
} else {
    Write-Host "   Client Application       | " -NoNewline; Write-Host "Desktop GUI (Production)" -ForegroundColor Cyan -NoNewline; Write-Host "    | Running" -ForegroundColor Green
}

Write-Host "   Voice / Desktop Socket   | " -NoNewline; Write-Host "udp://localhost:9987" -ForegroundColor Cyan -NoNewline; Write-Host "          | Listening" -ForegroundColor Green
Write-Host "   WebTransport QUIC Socket | " -NoNewline; Write-Host "udp://localhost:9989" -ForegroundColor Cyan -NoNewline; Write-Host "          | Listening" -ForegroundColor Green
Write-Host "   ServerQuery (SSH/Telnet) | " -NoNewline; Write-Host "tcp://localhost:10022 / 10011" -ForegroundColor Cyan -NoNewline; Write-Host " | Listening" -ForegroundColor Green
Write-Host "   File Transfer Port       | " -NoNewline; Write-Host "tcp://localhost:30303" -ForegroundColor Cyan -NoNewline; Write-Host "         | Listening" -ForegroundColor Green
Write-Host "   Web Client (HTTPS/HTTP)  | " -NoNewline; Write-Host "https://localhost:8080 (8081)" -ForegroundColor Cyan -NoNewline; Write-Host " | Listening" -ForegroundColor Green
Write-Host "=======================================================================" -ForegroundColor DarkCyan

Write-Host ""
Write-Host "[OK] Both Server and Client are running." -ForegroundColor Green
Write-Host "[INFO] To stop all services cleanly, run 'stop-all.bat' or double-click it." -ForegroundColor Gray
Write-Host "=======================================================================" -ForegroundColor DarkCyan
