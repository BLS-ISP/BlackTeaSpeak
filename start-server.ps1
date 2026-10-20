<#
.SYNOPSIS
Starts the BlackTeaSpeak Server with automatic binary freshness checks, TLS verification, and WebClient readiness.

.PARAMETER QueryBind
The address/port to bind the SSH ServerQuery socket to. Default: 0.0.0.0:10022 (Telnet Query also binds on 0.0.0.0:10011)
.PARAMETER WebBind
The address/port to bind the WebTransport QUIC UDP socket to. Default: 0.0.0.0:9989
.PARAMETER DesktopBind
The address/port to bind the Desktop/Voice UDP & TCP socket to. Default: 0.0.0.0:9987
.PARAMETER FileBind
The address/port to bind the File Transfer TCP socket to. Default: 0.0.0.0:30303
.PARAMETER WebClientBind
The address/port to bind the Web Client HTTPS server to. Default: 0.0.0.0:8080 (HTTP automatically binds on 8081)
.PARAMETER Rebuild
Force a rebuild of the server release binary and Web Client.
.PARAMETER Force
Forcefully terminate any existing processes using the target ports.
.PARAMETER NoNewWindow
Run the server in the current window instead of launching a new console window.
#>

param (
    [string]$QueryBind = "0.0.0.0:10022",
    [string]$WebBind = "0.0.0.0:9989",
    [string]$DesktopBind = "0.0.0.0:9987",
    [string]$FileBind = "0.0.0.0:30303",
    [string]$WebClientBind = "0.0.0.0:8080",
    [switch]$Rebuild,
    [switch]$Force,
    [switch]$NoNewWindow
)

$ErrorActionPreference = "Stop"
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

function Write-Err ([string]$text) {
    Write-Host "[FAIL] $text" -ForegroundColor Red
}

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
    if (-not (Test-Path $CertDir)) {
        New-Item -ItemType Directory -Path $CertDir -Force | Out-Null
    }
    try {
        Write-Info "Running: cargo run --manifest-path BlackTeaSpeak-Server\Cargo.toml -- generate-web-cert"
        & cargo run --manifest-path BlackTeaSpeak-Server\Cargo.toml -- generate-web-cert
        Write-Success "TLS certificates generated successfully!"
    }
    catch {
        Write-Err "Failed to generate certificates: $_"
        Exit 1
    }
} else {
    Write-Success "TLS certificates are ready."
}

# 3. Web Client Build Check
Write-Header "Checking BlackTeaWeb Frontend"
$WebClientDir = Join-Path $ScriptDir "BlackTeaSpeak-Server\BlackTeaWeb"
$WebDistIndex = Join-Path $WebClientDir "dist\index.html"
if (Test-Path (Join-Path $WebClientDir "package.json")) {
    if (-not (Test-Path $WebDistIndex) -or $Rebuild) {
        Write-Info "Building BlackTeaWeb frontend..."
        Push-Location $WebClientDir
        try {
            if (-not (Test-Path (Join-Path $WebClientDir "node_modules"))) {
                & npm install --legacy-peer-deps
            }
            & npm run build-web
            Write-Success "BlackTeaWeb frontend built."
        }
        finally {
            Pop-Location
        }
    } else {
        Write-Success "BlackTeaWeb dist bundle is ready."
    }
}

# 4. Compilation & Binary Freshness Check
Write-Header "Checking Server Binary"
$ServerDir = Join-Path $ScriptDir "BlackTeaSpeak-Server"
$ServerExe = Join-Path $ServerDir "target\release\blackteaspeak_server.exe"
$NeedsBuild = $Rebuild -or (-not (Test-Path $ServerExe))

if (-not $NeedsBuild) {
    $ExeTime = (Get-Item $ServerExe).LastWriteTimeUtc
    $NewestSource = Get-ChildItem -Path (Join-Path $ServerDir "src"), (Join-Path $ServerDir "Cargo.toml") -Recurse -File -ErrorAction SilentlyContinue |
        Sort-Object LastWriteTimeUtc -Descending |
        Select-Object -First 1
    if ($NewestSource -and $NewestSource.LastWriteTimeUtc -gt $ExeTime) {
        Write-Warn "Source file '$($NewestSource.Name)' is newer than release binary. Recompiling automatically..."
        $NeedsBuild = $true
    }
}

if ($NeedsBuild) {
    Write-Info "Compiling BlackTeaSpeak Server in release mode..."
    try {
        & cargo build --release --manifest-path BlackTeaSpeak-Server\Cargo.toml --bin blackteaspeak_server
        if ($LASTEXITCODE -ne 0) {
            throw "cargo build exited with code $LASTEXITCODE"
        }
        Write-Success "Server built successfully."
    }
    catch {
        Write-Err "Compilation failed: $_"
        Exit 1
    }
} else {
    Write-Success "Server binary is up to date: $ServerExe"
}

# 5. Port Conflict Check
Write-Header "Checking Port Availability"

if ($Force) {
    Write-Info "Force flag passed. Stopping any running BlackTeaSpeak Server instances first..."
    & (Join-Path $ScriptDir "stop-server.ps1")
    Start-Sleep -Milliseconds 600
}

$QueryPort = [int](($QueryBind -split ":")[-1])
$WebPort = [int](($WebBind -split ":")[-1])
$FilePort = [int](($FileBind -split ":")[-1])
$WebClientPort = [int](($WebClientBind -split ":")[-1])
$DesktopPort = [int](($DesktopBind -split ":")[-1])

$TcpPortsToCheck = @{
    "SSH ServerQuery Port"  = $QueryPort
    "File Transfer Port"    = $FilePort
    "Web Client HTTPS Port" = $WebClientPort
}
$UdpPortsToCheck = @{
    "Voice / Desktop UDP Port"  = $DesktopPort
    "WebTransport QUIC UDP Port" = $WebPort
}

$Conflicts = @()

$AllTcpConns = Get-NetTCPConnection -State Listen -ErrorAction SilentlyContinue
foreach ($PortName in $TcpPortsToCheck.Keys) {
    $Port = $TcpPortsToCheck[$PortName]
    $Conn = $AllTcpConns | Where-Object { $_.LocalPort -eq $Port } | Select-Object -First 1
    if ($Conn) {
        $OwnerPid = $Conn.OwningProcess
        $Proc = Get-Process -Id $OwnerPid -ErrorAction SilentlyContinue
        $ProcName = if ($Proc) { $Proc.Name } else { "Unknown" }
        Write-Warn "$PortName (TCP $Port) is in use by Process '$ProcName' (PID: $OwnerPid)"
        $Conflicts += [PSCustomObject]@{
            PortName = $PortName
            Port     = $Port
            PID      = $OwnerPid
            ProcName = $ProcName
        }
    }
}

$AllUdpConns = Get-NetUDPEndpoint -ErrorAction SilentlyContinue
foreach ($PortName in $UdpPortsToCheck.Keys) {
    $Port = $UdpPortsToCheck[$PortName]
    $UdpConn = $AllUdpConns | Where-Object { $_.LocalPort -eq $Port } | Select-Object -First 1
    if ($UdpConn) {
        $OwnerPid = $UdpConn.OwningProcess
        $Proc = Get-Process -Id $OwnerPid -ErrorAction SilentlyContinue
        $ProcName = if ($Proc) { $Proc.Name } else { "Unknown" }
        if (-not ($Conflicts | Where-Object { $_.PID -eq $OwnerPid -and $_.Port -eq $Port })) {
            Write-Warn "$PortName (UDP $Port) is in use by Process '$ProcName' (PID: $OwnerPid)"
            $Conflicts += [PSCustomObject]@{
                PortName = $PortName
                Port     = $Port
                PID      = $OwnerPid
                ProcName = $ProcName
            }
        }
    }
}

if ($Conflicts.Count -gt 0) {
    if ($Force) {
        Write-Info "Terminating remaining conflicting processes..."
        foreach ($Conflict in $Conflicts) {
            Stop-Process -Id $Conflict.PID -Force -ErrorAction SilentlyContinue
        }
        Start-Sleep -Seconds 1
        Write-Success "Conflicting processes terminated."
    } else {
        Write-Host "`nPort conflicts detected! The server cannot start." -ForegroundColor Yellow
        Write-Host "Run 'stop-server.bat' first, or pass '-Force' to stop existing processes automatically." -ForegroundColor Gray
        Exit 1
    }
} else {
    Write-Success "All ports are clear."
}

# 6. Launch the Server
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

$WebHttpPort = if ($WebClientPort -eq 8080) { 8081 } elseif ($WebClientPort -eq 443) { 80 } else { 8080 }

Write-Info "Bindings Configured:"
Write-Info " - Voice / TeaSpeak / TS3 / BTEA : udp://$DesktopBind & tcp://$DesktopBind"
Write-Info " - WebTransport (QUIC HTTP/3)    : udp://$WebBind"
Write-Info " - SSH ServerQuery               : tcp://$QueryBind (Telnet: tcp://0.0.0.0:10011)"
Write-Info " - File Transfer                 : tcp://$FileBind"
Write-Info " - Web Client (HTTPS / HTTP)     : https://localhost:$WebClientPort | http://localhost:$WebHttpPort"

$WorkDir = $ServerDir

if ($NoNewWindow) {
    Write-Info "Starting in foreground..."
    Set-Location $WorkDir
    & $ServerExe @ArgsList
} else {
    Write-Info "Starting in a new window..."

    $StartInfo = New-Object System.Diagnostics.ProcessStartInfo
    $StartInfo.FileName = $ServerExe
    $StartInfo.Arguments = $ArgsList -join " "
    $StartInfo.WorkingDirectory = $WorkDir
    $StartInfo.UseShellExecute = $true

    $Process = [System.Diagnostics.Process]::Start($StartInfo)

    if ($Process) {
        Set-Content -Path (Join-Path $ScriptDir ".server.pid") -Value $Process.Id -ErrorAction SilentlyContinue
        Start-Sleep -Milliseconds 1200
        if ($Process.HasExited) {
            Write-Err "Server process (PID $($Process.Id)) exited prematurely with code $($Process.ExitCode)."
            Write-Info "Check logs\server.log or BlackTeaSpeak-Server\logs\server.log for details."
            Exit 1
        }
        Write-Success "Server process is running with PID $($Process.Id)!"
        Write-Info "Logs and live admin console ('permreset') are active in the server window."
    } else {
        Write-Err "Failed to launch server process."
        Exit 1
    }
}

Write-Host ""
Write-Host "Ready!" -ForegroundColor Green
