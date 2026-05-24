<#
.SYNOPSIS
Starts the BlackTeaSpeak Client in development or production mode.

.DESCRIPTION
Checks prerequisites (Node/NPM), installs dependencies if missing, and launches the Tauri desktop client.

.PARAMETER Mode
Specify the launch mode. Options: 'Dev' or 'Prod'. Default: Dev
.PARAMETER Build
Force a production build when using Prod mode.
.PARAMETER Force
Forcefully terminate any existing client processes or frontend dev servers.
.PARAMETER NoNewWindow
Run the client development command in the current terminal window instead of a new one.
#>

param (
    [ValidateSet("Dev", "Prod")]
    [string]$Mode = "Dev",
    [switch]$Build,
    [switch]$Force,
    [switch]$NoNewWindow
)

$ErrorActionPreference = "Stop"
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

function Write-Err ([string]$text) {
    Write-Host "[FAIL] $text" -ForegroundColor Red
}

# --- Banner ---
Write-Host "=============================================" -ForegroundColor DarkGreen
Write-Host "   BlackTeaSpeak Client Control Center       " -ForegroundColor Green -BackgroundColor Black
Write-Host "=============================================" -ForegroundColor DarkGreen

# 1. Check Node & NPM
Write-Header "Checking Prerequisites"
if (Get-Command "node" -ErrorAction SilentlyContinue) {
    $NodeVer = node -v
    Write-Success "Node.js is installed ($NodeVer)."
} else {
    Write-Err "Node.js is not installed or not in PATH."
    Write-Info "Please install Node.js from https://nodejs.org/ and restart your terminal."
    Exit 1
}

if (Get-Command "npm" -ErrorAction SilentlyContinue) {
    Write-Success "NPM is installed."
} else {
    Write-Err "NPM is not installed or not in PATH."
    Exit 1
}

# 2. Check and Install Node Modules
$ClientDir = Join-Path $ScriptDir "BlackTeaSpeak-Client"
$NodeModulesDir = Join-Path $ClientDir "node_modules"

if (-not (Test-Path $NodeModulesDir)) {
    Write-Warn "Client node_modules folder is missing. Installing dependencies..."
    try {
        Set-Location $ClientDir
        & npm install
        Write-Success "Dependencies installed successfully!"
        Set-Location $ScriptDir
    }
    catch {
        Write-Err "Failed to install dependencies: $_"
        Exit 1
    }
} else {
    Write-Success "Node modules are ready."
}

# 3. Kill existing client if Force is enabled
if ($Force) {
    Write-Header "Cleaning up Existing Clients"
    Write-Info "Terminating running client instances..."
    
    # Kill Tauri Client binaries
    Get-Process -Name "tauri-app" -ErrorAction SilentlyContinue | Stop-Process -Force
    Get-Process -Name "BlackTeaSpeak-Client" -ErrorAction SilentlyContinue | Stop-Process -Force
    
    # Kill Vite Dev server on port 1420
    $ViteConn = Get-NetTCPConnection -LocalPort 1420 -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($ViteConn) {
        $Pid = $ViteConn.OwningProcess
        Write-Info "Stopping Vite dev server process (PID: $Pid)..."
        Stop-Process -Id $Pid -Force
    }
    Start-Sleep -Seconds 1
    Write-Success "Clean sweep completed."
}

# 4. Mode-specific operations
if ($Mode -eq "Dev") {
    Write-Header "Launching Client (Development Mode)"
    
    # Check port 1420 conflict
    $ViteConn = Get-NetTCPConnection -LocalPort 1420 -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($ViteConn) {
        $Pid = $ViteConn.OwningProcess
        $Proc = Get-Process -Id $Pid -ErrorAction SilentlyContinue
        $ProcName = if ($Proc) { $Proc.Name } else { "Unknown" }
        Write-Warn "Port 1420 (Vite Dev Server) is already in use by process '$ProcName' (PID: $Pid)."
        Write-Info "Use 'stop-client.ps1' or start with '-Force' parameter to terminate it."
        Exit 1
    }

    Write-Info "Starting Vite dev server and Tauri developer runner..."
    Write-Info "Client will open automatically in a window once Vite compiles."

    if ($NoNewWindow) {
        Set-Location $ClientDir
        & npx tauri dev
    } else {
        # Launch tauri dev in a new cmd window
        # Run npm run tauri dev inside the client directory
        Start-Process -FilePath "cmd.exe" -ArgumentList "/c npm run tauri dev" -WorkingDirectory $ClientDir -NoNewWindow:$false
        Write-Success "Tauri dev runner started in a separate console window."
    }
}
else {
    Write-Header "Launching Client (Production/Release Mode)"
    
    # Target executable path checking both possible release names
    $ReleaseDir = Join-Path $ClientDir "src-tauri\target\release"
    $TauriAppExe = Join-Path $ReleaseDir "tauri-app.exe"
    $ProductExe = Join-Path $ReleaseDir "BlackTeaSpeak-Client.exe"
    
    $ClientExe = if (Test-Path $ProductExe) { $ProductExe } else { $TauriAppExe }
    $ExeExists = Test-Path $ClientExe

    if (-not $ExeExists -or $Build) {
        if ($Build) {
            Write-Info "Rebuild forced."
        } else {
            Write-Warn "Production binary not found."
        }
        
        Write-Info "Building production Tauri client (this might take a few minutes)..."
        try {
            Set-Location $ClientDir
            & npm run tauri build
            Set-Location $ScriptDir
            
            # Recalculate Exe
            $ClientExe = if (Test-Path $ProductExe) { $ProductExe } else { $TauriAppExe }
            if (Test-Path $ClientExe) {
                Write-Success "Production client built successfully!"
            } else {
                Write-Err "Tauri build completed but could not locate the output executable."
                Exit 1
            }
        }
        catch {
            Write-Err "Build failed: $_"
            Exit 1
        }
    }

    Write-Info "Starting production client..."
    Write-Info "Running: $ClientExe"
    Start-Process -FilePath $ClientExe -WorkingDirectory $ReleaseDir
    Write-Success "Production client launched successfully."
}

Write-Host ""
Write-Host "Ready!" -ForegroundColor Green
