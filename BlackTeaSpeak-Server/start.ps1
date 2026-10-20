<#
.SYNOPSIS
Starts the BlackTeaSpeak Compatibility Backend.

.DESCRIPTION
This script builds and starts the blackteaspeak_server backend with the standard port bindings.
#>

$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Definition
Set-Location $ScriptDir

Write-Host "Building BlackTeaSpeak Backend..."
cargo build --release --bin blackteaspeak_server

Write-Host "Starting BlackTeaSpeak Backend..."
$env:RUST_LOG = "info"

.\target\release\blackteaspeak_server.exe serve-all `
    --desktop-bind "0.0.0.0:9987" `
    --web-bind "0.0.0.0:9989" `
    --query-bind "0.0.0.0:10022" `
    --file-bind "0.0.0.0:30303" `
    --web-client-bind "0.0.0.0:8080"
