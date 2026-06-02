<#
.SYNOPSIS
Starts the BlackTeaSpeak Server with robust checks and beautiful formatting.

.DESCRIPTION
Checks ports, builds the binary if missing, generates certificates if needed, and starts the server in a new window.

.PARAMETER QueryBind
The address/port to bind the ServerQuery TCP socket to. Default: 0.0.0.0:10101
.PARAMETER WebBind
The address/port to bind the Web transport TCP socket to. Default: 0.0.0.0:9987
.PARAMETER DesktopBind
The address/port to bind the Desktop UDP socket to. Default: 0.0.0.0:9987
.PARAMETER FileBind
The address/port to bind the File Transfer TCP socket to. Default: 0.0.0.0:30303
.PARAMETER WebClientBind
The address/port to bind the Web client HTTPS server to. Default: 0.0.0.0:8081
.PARAMETER Rebuild
Force a rebuild of the server release binary.
.PARAMETER Force
Forcefully terminate any existing processes using the target ports.
.PARAMETER NoNewWindow
Run the server in the current window instead of launching a new console window.
#>

param (
    [string]$QueryBind = "0.0.0.0:10101",
    [string]$WebBind = "0.0.0.0:9987",
    [string]$DesktopBind = "0.0.0.0:9987",
    [string]$FileBind = "0.0.0.0:30303",
    [string]$WebClientBind = "0.0.0.0:8081",
    [switch]$Rebuild,
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
Write-Host "   BlackTeaSpeak Server Control Center       " -ForegroundColor Green -BackgroundColor Black
Write-Host "=============================================" -ForegroundColor DarkGreen

# 1. Dependency Checks
Write-Header "Checking Prerequisites"
if (Get-Command "cargo" -ErrorAction SilentlyContinue) {
    Write-Success "Rust (Cargo) is installed."
} else {
    Write-Err "Rust (Cargo) is not installed or not in PATH."
    Write-Info "Please install Rust from https://rustup.rs/ and restart your terminal."
    Exit 1
}

# 2. TLS Certificate Checks
Write-Header "Verifying TLS Certificates"
$CertDir = Join-Path $ScriptDir "BlackTeaSpeak-Server\data\tls"
$CertFile = Join-Path $CertDir "blackteaweb-localhost-cert.pem"
$KeyFile = Join-Path $CertDir "blackteaweb-localhost-key.pem"

if (-not (Test-Path $CertFile) -or -not (Test-Path $KeyFile)) {
    Write-Warn "TLS certificates are missing. Generating new localhost certs..."
    
    # Ensure parent dir exists
    if (-not (Test-Path $CertDir)) {
        New-Item -ItemType Directory -Path $CertDir -Force | Out-Null
    }

    try {
        # Run certificate generator
        Write-Info "Running: cargo run --manifest-path BlackTeaSpeak-Server\Cargo.toml -- generate-web-cert"
        & cargo run --manifest-path BlackTeaSpeak-Server\Cargo.toml -- generate-web-cert
        Write-Success "TLS certificates generated successfully!"
        
        Write-Host "`n+------------------------------------------------------------+" -ForegroundColor Yellow
        Write-Host "| IMPORTANT: To trust this certificate in your browser:       |" -ForegroundColor Yellow
        Write-Host "| Run the following command in an Administrator PowerShell:   |" -ForegroundColor Yellow
        Write-Host "| certutil -user -addstore Root `"$CertFile`"               |" -ForegroundColor Cyan
        Write-Host "+------------------------------------------------------------+`n" -ForegroundColor Yellow
    }
    catch {
        Write-Err "Failed to generate certificates: $_"
        Exit 1
    }
} else {
    Write-Success "TLS certificates are ready."
}

# 3. Compilation Check
Write-Header "Checking Server Binary"
$ServerExe = Join-Path $ScriptDir "BlackTeaSpeak-Server\target\release\blackteaspeak_server.exe"

if (-not (Test-Path $ServerExe) -or $Rebuild) {
    if ($Rebuild) {
        Write-Info "Forced rebuild requested."
    } else {
        Write-Warn "Server binary not found."
    }
    
    Write-Info "Compiling BlackTeaSpeak Server in release mode (this may take a minute)..."
    try {
        & cargo build --release --manifest-path BlackTeaSpeak-Server\Cargo.toml --bin blackteaspeak_server
        Write-Success "Server built successfully."
    }
    catch {
        Write-Err "Compilation failed: $_"
        Exit 1
    }
} else {
    Write-Success "Server binary found: $ServerExe"
}

# 4. Port Conflict Check
Write-Header "Checking Port Availability"

# Parse port numbers out of bindings
$PortsToCheck = @{}
$PortsToCheck["Query Port"] = [int]($QueryBind -split ":")[-1]
$PortsToCheck["Web Transport TCP Port"] = [int]($WebBind -split ":")[-1]
$PortsToCheck["File Transfer Port"] = [int]($FileBind -split ":")[-1]
$PortsToCheck["Web Client HTTPS Port"] = [int]($WebClientBind -split ":")[-1]
$DesktopPort = [int]($DesktopBind -split ":")[-1]

$Conflicts = @()

foreach ($PortName in $PortsToCheck.Keys) {
    $Port = $PortsToCheck[$PortName]
    $Conn = Get-NetTCPConnection -LocalPort $Port -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($Conn) {
        $OwnerPid = $Conn.OwningProcess
        $Proc = Get-Process -Id $OwnerPid -ErrorAction SilentlyContinue
        $ProcName = if ($Proc) { $Proc.Name } else { "Unknown" }
        Write-Warn "$PortName ($Port) is in use by Process '$ProcName' (PID: $OwnerPid)"
        $Conflicts += [PSCustomObject]@{
            PortName = $PortName
            Port     = $Port
            PID      = $OwnerPid
            ProcName = $ProcName
        }
    }
}

# Check UDP port for Desktop (only if different from Web TCP, or check independently)
$UdpConn = Get-NetUDPEndpoint -LocalPort $DesktopPort -ErrorAction SilentlyContinue | Select-Object -First 1
if ($UdpConn) {
    $OwnerPid = $UdpConn.OwningProcess
    $Proc = Get-Process -Id $OwnerPid -ErrorAction SilentlyContinue
    $ProcName = if ($Proc) { $Proc.Name } else { "Unknown" }
    # Only report if it's not already reported under the same PID (sharing ports)
    if (-not ($Conflicts | Where-Object { $_.PID -eq $OwnerPid -and $_.Port -eq $DesktopPort })) {
        Write-Warn "Desktop UDP Port ($DesktopPort) is in use by Process '$ProcName' (PID: $OwnerPid)"
        $Conflicts += [PSCustomObject]@{
            PortName = "Desktop UDP Port"
            Port     = $DesktopPort
            PID      = $OwnerPid
            ProcName = $OwnerPid
        }
    }
}

if ($Conflicts.Count -gt 0) {
    if ($Force) {
        Write-Info "Force flag passed. Terminating conflicting processes..."
        foreach ($Conflict in $Conflicts) {
            Write-Info "Terminating process '$($Conflict.ProcName)' (PID: $($Conflict.PID))..."
            Stop-Process -Id $Conflict.PID -Force -ErrorAction SilentlyContinue
        }
        Start-Sleep -Seconds 1
        Write-Success "Conflicting processes terminated."
    } else {
        Write-Host "`nConflicts detected! The server cannot start." -ForegroundColor Yellow
        Write-Host "You can stop them manually, or run this script with the -Force parameter to kill them automatically." -ForegroundColor Gray
        Write-Host "Alternatively, run 'stop-server.ps1' to clean up." -ForegroundColor Gray
        Exit 1
    }
} else {
    Write-Success "All ports are clear."
}

# 5. Launch the Server
Write-Header "Launching BlackTeaSpeak Server"

$env:RUST_LOG = "info"
$ArgsList = @(
    "serve-all",
    "--query-bind", $QueryBind,
    "--web-bind", $WebBind,
    "--desktop-bind", $DesktopBind,
    "--file-bind", $FileBind,
    "--web-client-bind", $WebClientBind
)

Write-Info "Bindings Configured:"
Write-Info " - Desktop Bind:      udp://$DesktopBind"
Write-Info " - Web Transport:     tcp://$WebBind"
Write-Info " - ServerQuery:       tcp://$QueryBind"
Write-Info " - File Transfer:     tcp://$FileBind"
Write-Info " - Web Client (HTTPS): https://localhost:$($WebClientBind -split ":")[-1]"

$WorkDir = Join-Path $ScriptDir "BlackTeaSpeak-Server"

if ($NoNewWindow) {
    Write-Info "Starting in foreground..."
    # Execute in foreground
    Set-Location $WorkDir
    & $ServerExe $ArgsList
} else {
    Write-Info "Starting in a new window..."
    
    $StartInfo = New-Object System.Diagnostics.ProcessStartInfo
    $StartInfo.FileName = $ServerExe
    $StartInfo.Arguments = $ArgsList -join " "
    $StartInfo.WorkingDirectory = $WorkDir
    $StartInfo.UseShellExecute = $true
    
    $Process = [System.Diagnostics.Process]::Start($StartInfo)
    
    if ($Process) {
        Write-Success "Server process launched with PID $($Process.Id)!"
        Write-Info "Logs and server activity will show in the newly opened window."
    } else {
        Write-Err "Failed to launch server process."
        Exit 1
    }
}

Write-Host ""
Write-Host "Ready!" -ForegroundColor Green
