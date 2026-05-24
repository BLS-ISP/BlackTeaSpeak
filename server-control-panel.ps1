<#
.SYNOPSIS
A premium Windows Forms graphical user interface (GUI) control panel for the BlackTeaSpeak Server.

.DESCRIPTION
Provides a real-time dark-themed dashboard to view server configuration, active port states, PID tracking, and explicit controls to start, stop, or restart the server.
#>

Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName System.ServiceProcess # Just in case

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Definition
Set-Location $ScriptDir

# Sane Default Port Configuration (matching start-server.ps1)
$Config = @{
    QueryPort = 10101
    WebPort = 9987
    VoicePort = 9987
    FilePort = 30303
    WebClientPort = 8081
}

# --- GUI Color Palette (Premium Cyber Dark Theme) ---
$ColorBg = [System.Drawing.Color]::FromArgb(18, 18, 24)        # Deep Charcoal
$ColorCard = [System.Drawing.Color]::FromArgb(28, 28, 36)      # Slate Gray Card
$ColorHeader = [System.Drawing.Color]::FromArgb(36, 36, 47)    # Accent header
$ColorTeal = [System.Drawing.Color]::FromArgb(0, 173, 181)     # Neon Teal accent
$ColorTealHover = [System.Drawing.Color]::FromArgb(0, 210, 220)
$ColorRed = [System.Drawing.Color]::FromArgb(233, 69, 96)       # Neon Red/Coral
$ColorRedHover = [System.Drawing.Color]::FromArgb(255, 90, 110)
$ColorGreen = [System.Drawing.Color]::FromArgb(78, 205, 196)    # Active Mint Green
$ColorGray = [System.Drawing.Color]::FromArgb(142, 142, 160)    # Soft Gray text
$ColorWhite = [System.Drawing.Color]::White

# --- Initialize Form ---
$Form = New-Object System.Windows.Forms.Form
$Form.Text = "BlackTeaSpeak Server Control Panel"
$Form.Size = New-Object System.Drawing.Size(530, 520)
$Form.StartPosition = "CenterScreen"
$Form.BackColor = $ColorBg
$Form.ForeColor = $ColorWhite
$Form.FormBorderStyle = [System.Windows.Forms.FormBorderStyle]::FixedSingle
$Form.MaximizeBox = $false

# --- Fonts ---
$FontTitle = New-Object System.Drawing.Font("Segoe UI", 16, [System.Drawing.FontStyle]::Bold)
$FontSubtitle = New-Object System.Drawing.Font("Segoe UI", 9, [System.Drawing.FontStyle]::Regular)
$FontHeader = New-Object System.Drawing.Font("Segoe UI", 11, [System.Drawing.FontStyle]::Bold)
$FontText = New-Object System.Drawing.Font("Segoe UI", 10, [System.Drawing.FontStyle]::Regular)
$FontBold = New-Object System.Drawing.Font("Segoe UI", 10, [System.Drawing.FontStyle]::Bold)
$FontMono = New-Object System.Drawing.Font("Consolas", 10, [System.Drawing.FontStyle]::Regular)

# =====================================================================
# HEADER PANEL
# =====================================================================
$HeaderPanel = New-Object System.Windows.Forms.Panel
$HeaderPanel.Size = New-Object System.Drawing.Size(530, 70)
$HeaderPanel.Location = New-Object System.Drawing.Point(0, 0)
$HeaderPanel.BackColor = $ColorHeader

$HeaderTitle = New-Object System.Windows.Forms.Label
$HeaderTitle.Text = "BLACKTEASPEAK"
$HeaderTitle.Font = $FontTitle
$HeaderTitle.ForeColor = $ColorWhite
$HeaderTitle.Location = New-Object System.Drawing.Point(15, 12)
$HeaderTitle.AutoSize = $true

$HeaderSubtitle = New-Object System.Windows.Forms.Label
$HeaderSubtitle.Text = "SERVER MANAGEMENT & DIAGNOSTIC PANEL"
$HeaderSubtitle.Font = $FontSubtitle
$HeaderSubtitle.ForeColor = $ColorTeal
$HeaderSubtitle.Location = New-Object System.Drawing.Point(17, 42)
$HeaderSubtitle.AutoSize = $true

$HeaderPanel.Controls.Add($HeaderTitle)
$HeaderPanel.Controls.Add($HeaderSubtitle)
$Form.Controls.Add($HeaderPanel)

# =====================================================================
# STATUS CARD
# =====================================================================
$StatusCard = New-Object System.Windows.Forms.Panel
$StatusCard.Size = New-Object System.Drawing.Size(485, 75)
$StatusCard.Location = New-Object System.Drawing.Point(15, 85)
$StatusCard.BackColor = $ColorCard

# Status Indicator light
$StatusLight = New-Object System.Windows.Forms.Panel
$StatusLight.Size = New-Object System.Drawing.Size(16, 16)
$StatusLight.Location = New-Object System.Drawing.Point(20, 30)
$StatusLight.BackColor = [System.Drawing.Color]::Gray

$StatusLabel = New-Object System.Windows.Forms.Label
$StatusLabel.Text = "Server Status: Checking..."
$StatusLabel.Font = $FontHeader
$StatusLabel.ForeColor = $ColorWhite
$StatusLabel.Location = New-Object System.Drawing.Point(45, 18)
$StatusLabel.AutoSize = $true

$PidLabel = New-Object System.Windows.Forms.Label
$PidLabel.Text = "Process PID: N/A"
$PidLabel.Font = $FontSubtitle
$PidLabel.ForeColor = $ColorGray
$PidLabel.Location = New-Object System.Drawing.Point(45, 42)
$PidLabel.AutoSize = $true

$StatusCard.Controls.Add($StatusLight)
$StatusCard.Controls.Add($StatusLabel)
$StatusCard.Controls.Add($PidLabel)
$Form.Controls.Add($StatusCard)

# =====================================================================
# CONFIGURATION & PORTS CARD
# =====================================================================
$ConfigCard = New-Object System.Windows.Forms.Panel
$ConfigCard.Size = New-Object System.Drawing.Size(485, 230)
$ConfigCard.Location = New-Object System.Drawing.Point(15, 175)
$ConfigCard.BackColor = $ColorCard

$ConfigTitle = New-Object System.Windows.Forms.Label
$ConfigTitle.Text = "Port Bindings & Live Status"
$ConfigTitle.Font = $FontHeader
$ConfigTitle.ForeColor = $ColorTeal
$ConfigTitle.Location = New-Object System.Drawing.Point(15, 12)
$ConfigTitle.AutoSize = $true
$ConfigCard.Controls.Add($ConfigTitle)

# Helper to build rows
$Rows = @(
    @{ Name = "Voice/Desktop UDP Socket"; Port = $Config.VoicePort; Protocol = "UDP"; Bind = "0.0.0.0" },
    @{ Name = "Web Transport TCP Socket"; Port = $Config.WebPort; Protocol = "TCP"; Bind = "0.0.0.0" },
    @{ Name = "ServerQuery TCP Interface"; Port = $Config.QueryPort; Protocol = "TCP"; Bind = "0.0.0.0" },
    @{ Name = "File Transfer TCP Port"; Port = $Config.FilePort; Protocol = "TCP"; Bind = "0.0.0.0" },
    @{ Name = "Web Client HTTPS Port"; Port = $Config.WebClientPort; Protocol = "TCP"; Bind = "0.0.0.0" }
)

$RowControls = @()
$YOffset = 45

for ($i = 0; $i -lt $Rows.Count; $i++) {
    $Row = $Rows[$i]
    
    # Port Light
    $PortLight = New-Object System.Windows.Forms.Panel
    $PortLight.Size = New-Object System.Drawing.Size(10, 10)
    $PortLight.Location = New-Object System.Drawing.Point(20, $YOffset + 4)
    $PortLight.BackColor = [System.Drawing.Color]::Gray
    $ConfigCard.Controls.Add($PortLight)
    
    # Port Name Label
    $NameLbl = New-Object System.Windows.Forms.Label
    $NameLbl.Text = $Row.Name
    $NameLbl.Font = $FontText
    $NameLbl.ForeColor = $ColorWhite
    $NameLbl.Location = New-Object System.Drawing.Point(40, $YOffset)
    $NameLbl.AutoSize = $true
    $ConfigCard.Controls.Add($NameLbl)
    
    # Binding text
    $BindLbl = New-Object System.Windows.Forms.Label
    $BindLbl.Text = "$($Row.Bind):$($Row.Port) ($($Row.Protocol))"
    $BindLbl.Font = $FontMono
    $BindLbl.ForeColor = $ColorGray
    $BindLbl.Location = New-Object System.Drawing.Point(240, $YOffset)
    $BindLbl.AutoSize = $true
    $ConfigCard.Controls.Add($BindLbl)

    # State text
    $StateLbl = New-Object System.Windows.Forms.Label
    $StateLbl.Text = "Checking..."
    $StateLbl.Font = $FontBold
    $StateLbl.ForeColor = $ColorGray
    $StateLbl.Location = New-Object System.Drawing.Point(395, $YOffset)
    $StateLbl.AutoSize = $true
    $ConfigCard.Controls.Add($StateLbl)

    $RowControls += [PSCustomObject]@{
        Port      = $Row.Port
        Protocol  = $Row.Protocol
        Light     = $PortLight
        StateLabel = $StateLbl
    }
    
    $YOffset += 33
}

$Form.Controls.Add($ConfigCard)

# =====================================================================
# ACTION CONTROLS PANEL
# =====================================================================
$ActionsPanel = New-Object System.Windows.Forms.Panel
$ActionsPanel.Size = New-Object System.Drawing.Size(485, 60)
$ActionsPanel.Location = New-Object System.Drawing.Point(15, 415)

# --- Start Button ---
$BtnStart = New-Object System.Windows.Forms.Button
$BtnStart.Text = "START SERVER"
$BtnStart.Size = New-Object System.Drawing.Size(145, 45)
$BtnStart.Location = New-Object System.Drawing.Point(0, 0)
$BtnStart.Font = $FontBold
$BtnStart.BackColor = $ColorTeal
$BtnStart.ForeColor = $ColorWhite
$BtnStart.FlatStyle = [System.Windows.Forms.FlatStyle]::Flat
$BtnStart.FlatAppearance.BorderSize = 0
$BtnStart.Cursor = [System.Windows.Forms.Cursors]::Hand
$BtnStart.add_MouseEnter({ $BtnStart.BackColor = $ColorTealHover })
$BtnStart.add_MouseLeave({ $BtnStart.BackColor = $ColorTeal })

# --- Stop Button (Explict Shutdown) ---
$BtnStop = New-Object System.Windows.Forms.Button
$BtnStop.Text = "SHUTDOWN SERVER"
$BtnStop.Size = New-Object System.Drawing.Size(165, 45)
$BtnStop.Location = New-Object System.Drawing.Point(155, 0)
$BtnStop.Font = $FontBold
$BtnStop.BackColor = $ColorRed
$BtnStop.ForeColor = $ColorWhite
$BtnStop.FlatStyle = [System.Windows.Forms.FlatStyle]::Flat
$BtnStop.FlatAppearance.BorderSize = 0
$BtnStop.Cursor = [System.Windows.Forms.Cursors]::Hand
$BtnStop.add_MouseEnter({ $BtnStop.BackColor = $ColorRedHover })
$BtnStop.add_MouseLeave({ $BtnStop.BackColor = $ColorRed })

# --- Restart Button ---
$BtnRestart = New-Object System.Windows.Forms.Button
$BtnRestart.Text = "RESTART"
$BtnRestart.Size = New-Object System.Drawing.Size(155, 45)
$BtnRestart.Location = New-Object System.Drawing.Point(330, 0)
$BtnRestart.Font = $FontBold
$BtnRestart.BackColor = $ColorHeader
$BtnRestart.ForeColor = $ColorWhite
$BtnRestart.FlatStyle = [System.Windows.Forms.FlatStyle]::Flat
$BtnRestart.FlatAppearance.BorderSize = 0
$BtnRestart.Cursor = [System.Windows.Forms.Cursors]::Hand
$BtnRestart.add_MouseEnter({ $BtnRestart.BackColor = [System.Drawing.Color]::FromArgb(50, 50, 65) })
$BtnRestart.add_MouseLeave({ $BtnRestart.BackColor = $ColorHeader })

$ActionsPanel.Controls.Add($BtnStart)
$ActionsPanel.Controls.Add($BtnStop)
$ActionsPanel.Controls.Add($BtnRestart)
$Form.Controls.Add($ActionsPanel)

# =====================================================================
# SERVER LIFECYCLE CONTROLLER LOGIC
# =====================================================================

# Fetch current server process and port binds
function Get-ServerState {
    $ServerProc = Get-Process -Name "blackteaspeak_server" -ErrorAction SilentlyContinue | Select-Object -First 1
    
    $PortStates = @{}
    
    foreach ($Row in $Rows) {
        $Port = $Row.Port
        $Proto = $Row.Protocol
        $Key = "$Proto-$Port"
        
        if ($Proto -eq "TCP") {
            $Conn = Get-NetTCPConnection -LocalPort $Port -ErrorAction SilentlyContinue | Select-Object -First 1
            if ($Conn) {
                $PortStates[$Key] = [PSCustomObject]@{ Active = $true; PID = $Conn.OwningProcess }
            } else {
                $PortStates[$Key] = [PSCustomObject]@{ Active = $false; PID = $null }
            }
        } else {
            # UDP
            $Conn = Get-NetUDPEndpoint -LocalPort $Port -ErrorAction SilentlyContinue | Select-Object -First 1
            if ($Conn) {
                $PortStates[$Key] = [PSCustomObject]@{ Active = $true; PID = $Conn.OwningProcess }
            } else {
                $PortStates[$Key] = [PSCustomObject]@{ Active = $false; PID = $null }
            }
        }
    }
    
    return [PSCustomObject]@{
        Process = $ServerProc
        Ports   = $PortStates
    }
}

# Update GUI dashboard with current state
function Update-Dashboard {
    $State = Get-ServerState
    
    # 1. Update overall status card
    if ($State.Process) {
        $StatusLight.BackColor = $ColorGreen
        $StatusLabel.Text = "Server Status: RUNNING"
        $StatusLabel.ForeColor = $ColorWhite
        $PidLabel.Text = "Process PID: $($State.Process.Id) | CPU: $( [Math]::Round(($State.Process.CPU), 1) )s"
        
        $BtnStart.Enabled = $false
        $BtnStart.BackColor = [System.Drawing.Color]::FromArgb(40, 60, 60)
        $BtnStop.Enabled = $true
        $BtnStop.BackColor = $ColorRed
    } else {
        $StatusLight.BackColor = $ColorRed
        $StatusLabel.Text = "Server Status: STOPPED"
        $StatusLabel.ForeColor = $ColorWhite
        $PidLabel.Text = "Process PID: N/A"
        
        $BtnStart.Enabled = $true
        $BtnStart.BackColor = $ColorTeal
        $BtnStop.Enabled = $false
        $BtnStop.BackColor = [System.Drawing.Color]::FromArgb(60, 40, 40)
    }
    
    # 2. Update port grid
    foreach ($Ctrl in $RowControls) {
        $Key = "$($Ctrl.Protocol)-$($Ctrl.Port)"
        $PortInfo = $State.Ports[$Key]
        
        if ($PortInfo.Active) {
            $Ctrl.Light.BackColor = $ColorGreen
            $Ctrl.StateLabel.Text = "LISTENING"
            $Ctrl.StateLabel.ForeColor = $ColorGreen
        } else {
            $Ctrl.Light.BackColor = [System.Drawing.Color]::FromArgb(80, 80, 90)
            $Ctrl.StateLabel.Text = "INACTIVE"
            $Ctrl.StateLabel.ForeColor = $ColorGray
        }
    }
}

# =====================================================================
# EVENT HANDLERS
# =====================================================================

# Start Button Click
$BtnStart.add_Click({
    $BtnStart.Enabled = $false
    $StatusLabel.Text = "Server Status: STARTING..."
    $StatusLight.BackColor = [System.Drawing.Color]::Yellow
    $Form.Refresh()
    
    # Execute start-server.ps1 in a new window
    Write-Output "Starting server via start-server.ps1..."
    Start-Process -FilePath "powershell.exe" -ArgumentList "-NoProfile -ExecutionPolicy Bypass -File `"$ScriptDir\start-server.ps1`"" -WorkingDirectory $ScriptDir -NoNewWindow:$false
})

# Stop Button Click (Explicit Shutdown)
$BtnStop.add_Click({
    $Result = [System.Windows.Forms.MessageBox]::Show("Are you sure you want to shut down the BlackTeaSpeak server?", "Confirm Shutdown", [System.Windows.Forms.MessageBoxButtons]::YesNo, [System.Windows.Forms.MessageBoxIcon]::Warning)
    if ($Result -eq [System.Windows.Forms.DialogResult]::Yes) {
        $BtnStop.Enabled = $false
        $StatusLabel.Text = "Server Status: SHUTTING DOWN..."
        $StatusLight.BackColor = [System.Drawing.Color]::Yellow
        $Form.Refresh()
        
        # Execute stop-server.ps1 silently
        Write-Output "Stopping server via stop-server.ps1..."
        Start-Process -FilePath "powershell.exe" -ArgumentList "-NoProfile -ExecutionPolicy Bypass -File `"$ScriptDir\stop-server.ps1`"" -WorkingDirectory $ScriptDir -WindowStyle Hidden -Wait
        
        Update-Dashboard
    }
})

# Restart Button Click
$BtnRestart.add_Click({
    $BtnRestart.Enabled = $false
    $StatusLabel.Text = "Server Status: RESTARTING..."
    $StatusLight.BackColor = [System.Drawing.Color]::Yellow
    $Form.Refresh()
    
    # Call stop, then start
    Start-Process -FilePath "powershell.exe" -ArgumentList "-NoProfile -ExecutionPolicy Bypass -File `"$ScriptDir\stop-server.ps1`"" -WorkingDirectory $ScriptDir -WindowStyle Hidden -Wait
    Start-Process -FilePath "powershell.exe" -ArgumentList "-NoProfile -ExecutionPolicy Bypass -File `"$ScriptDir\start-server.ps1`"" -WorkingDirectory $ScriptDir
    
    Start-Sleep -Seconds 1
    $BtnRestart.Enabled = $true
})

# --- Auto Refresh Timer ---
$Timer = New-Object System.Windows.Forms.Timer
$Timer.Interval = 1500 # Refresh every 1.5 seconds
$Timer.add_Tick({
    Update-Dashboard
})
$Timer.Start()

# Initial Load
$Form.add_Load({
    Update-Dashboard
})

# Show Dialog (blocks thread until closed)
[System.Windows.Forms.Application]::Run($Form)
