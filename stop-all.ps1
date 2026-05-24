<#
.SYNOPSIS
Stops all components of the BlackTeaSpeak ecosystem cleanly.

.DESCRIPTION
Stops both the Server and Client GUI/dev servers.
#>

$ErrorActionPreference = "Continue"
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Definition
Set-Location $ScriptDir

$StopServerPath = Join-Path $ScriptDir "stop-server.ps1"
$StopClientPath = Join-Path $ScriptDir "stop-client.ps1"

Write-Host "=============================================" -ForegroundColor DarkRed
Write-Host "   BlackTeaSpeak Full Stop Cleanup           " -ForegroundColor Red -BackgroundColor Black
Write-Host "=============================================" -ForegroundColor DarkRed

# Stop Client
& $StopClientPath

# Stop Server
& $StopServerPath

Write-Host "`n=============================================" -ForegroundColor DarkRed
Write-Host "   All systems stopped cleanly. Ready.       " -ForegroundColor Green
Write-Host "=============================================" -ForegroundColor DarkRed
