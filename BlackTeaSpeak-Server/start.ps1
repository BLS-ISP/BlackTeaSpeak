<#
.SYNOPSIS
Starts the BlackTeaSpeak Compatibility Backend.

.DESCRIPTION
This script builds and starts the blackteaspeak_server backend with the requested port bindings.
The state and files will be stored in the workspace directory.
#>

$ErrorActionPreference = "Stop"

Write-Host "Building BlackTeaSpeak Backend..."
cargo build --release --bin blackteaspeak_server

Write-Host "Starting BlackTeaSpeak Backend..."
# The ports requested:
# 9987 udp: Desktop Client Voice/Handshake (--desktop-bind)
# 9987 tcp: BlackTeaWeb TCP Compatibility (--web-bind)
# 10101 tcp: ServerQuery (--query-bind)
# 30303 tcp: File Transfer (--file-bind)
# 80/443 tcp: Web Client HTTP/HTTPS (--web-client-bind)

# Note: WebRTC ports (50000-50020) and STUN (3478) are managed by WebRTC / external STUN servers (like Coturn).
# See docker-compose.yml for a containerized setup with STUN.

$env:RUST_LOG="info"

.\target\release\blackteaspeak_server.exe serve-all `
    --desktop-bind "0.0.0.0:9987" `
    --web-bind "0.0.0.0:9987" `
    --query-bind "0.0.0.0:10101" `
    --file-bind "0.0.0.0:30303" `
    --web-client-bind "0.0.0.0:443"
