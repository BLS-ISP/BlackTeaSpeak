<#
.SYNOPSIS
Starts the BlackTeaSpeak Client in development or production mode.

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
        Push-Location $ClientDir
        & npm install
        Pop-Location
        Write-Success "Dependencies installed successfully!"
    }
    catch {
        Pop-Location
        Write-Err "Failed to install dependencies: $_"
        Exit 1
    }
} else {
    Write-Success "Node modules are ready."
}

# 3. Kill existing client if Force is enabled
if ($Force) {
    Write-Header "Cleaning up Existing Clients"
    & (Join-Path $ScriptDir "stop-client.ps1")
    Start-Sleep -Milliseconds 600
}

# 4. Mode-specific operations
if ($Mode -eq "Dev") {
    Write-Header "Launching Client (Development Mode)"

    $ViteConn = Get-NetTCPConnection -LocalPort 1420 -State Listen -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($ViteConn) {
        $OwnerPid = $ViteConn.OwningProcess
        $Proc = Get-Process -Id $OwnerPid -ErrorAction SilentlyContinue
        $ProcName = if ($Proc) { $Proc.Name } else { "Unknown" }
        Write-Warn "Port 1420 (Vite Dev Server) is already in use by process '$ProcName' (PID: $OwnerPid)."
        Write-Info "Use 'stop-client.bat' or start with '-Force' parameter to terminate it."
        Exit 1
    }

    Write-Info "Starting Vite dev server and Tauri Desktop runner..."
    Write-Info "Client window will open automatically once compiled."

    if ($NoNewWindow) {
        Set-Location $ClientDir
        & npm run tauri dev
    } else {
        $DevProc = Start-Process -FilePath "cmd.exe" -ArgumentList "/k npm run tauri dev" -WorkingDirectory $ClientDir -PassThru
        if ($DevProc) {
            Set-Content -Path (Join-Path $ScriptDir ".client.pid") -Value $DevProc.Id -ErrorAction SilentlyContinue
        }
        Write-Success "Tauri dev runner started in a separate console window."
    }
}
else {
    Write-Header "Launching Client (Production/Release Mode)"

    $ReleaseDir = Join-Path $ClientDir "src-tauri\target\release"
    $TauriAppExe = Join-Path $ReleaseDir "tauri-app.exe"
    $ProductExe = Join-Path $ReleaseDir "BlackTeaSpeak-Client.exe"

    $ClientExe = if (Test-Path $ProductExe) { $ProductExe } else { $TauriAppExe }
    $NeedsBuild = $Build -or (-not (Test-Path $ClientExe))

    if (-not $NeedsBuild) {
        $ExeTime = (Get-Item $ClientExe).LastWriteTimeUtc
        $NewestSource = Get-ChildItem -Path (Join-Path $ClientDir "src"), (Join-Path $ClientDir "src-tauri\src") -Recurse -File -ErrorAction SilentlyContinue |
            Sort-Object LastWriteTimeUtc -Descending |
            Select-Object -First 1
        if ($NewestSource -and $NewestSource.LastWriteTimeUtc -gt $ExeTime) {
            Write-Warn "Client source '$($NewestSource.Name)' is newer than release binary. Rebuilding..."
            $NeedsBuild = $true
        }
    }

    if ($NeedsBuild) {
        Write-Info "Building production Tauri client..."
        try {
            Push-Location $ClientDir
            & npm run build
            & cargo build --release --manifest-path src-tauri\Cargo.toml
            Pop-Location

            $ClientExe = if (Test-Path $ProductExe) { $ProductExe } else { $TauriAppExe }
            if (Test-Path $ClientExe) {
                Write-Success "Production client built successfully!"
            } else {
                Write-Err "Build completed but could not locate the output executable."
                Exit 1
            }
        }
        catch {
            Pop-Location
            Write-Err "Build failed: $_"
            Exit 1
        }
    }

    Write-Info "Starting production client: $ClientExe"
    Start-Process -FilePath $ClientExe -WorkingDirectory $ReleaseDir
    Write-Success "Production client launched successfully."
}

Write-Host ""
Write-Host "Ready!" -ForegroundColor Green
