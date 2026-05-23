import os

DOCS_DIR = "docs"
ARC42_DIR = os.path.join(DOCS_DIR, "arc42")
PROTO_DIR = os.path.join(DOCS_DIR, "protocols")
INT_DIR = os.path.join(DOCS_DIR, "integration")

os.makedirs(ARC42_DIR, exist_ok=True)
os.makedirs(PROTO_DIR, exist_ok=True)
os.makedirs(INT_DIR, exist_ok=True)

# arc42/04_solution_strategy.md
solution = """# 4. Solution Strategy

The core strategy for BlackTeaSpeak relies on absolute decoupling of the network ingestion layer from the application logic layer.

## Architecture Drivers
1. **Concurrency Model**: Tokio asynchronous runtime is used exclusively.
2. **State Management**: `BaselineRuntime` acts as a monolithic lock state. By encapsulating state within an `Arc<Mutex<BaselineRuntime>>`, we guarantee that state transitions (like joining a channel, receiving permissions, banning) are executed atomically.
3. **Legacy Handshake Decoupling**: TS3 uses complex UDP puzzle exchanges. The `desktop_transport` crate fully abstracts this away into standard internal connection states, ensuring `BaselineRuntime` doesn't care about puzzles.

## Technology Stack
- **Language**: Rust (edition 2021)
- **Networking**: `tokio` (TCP/UDP), `quinn` (WebTransport/QUIC), `russh` (SSH ServerQuery).
- **Crypto**: `x25519-dalek`, `aes-gcm`, `chacha20poly1305`, `ed25519-dalek`.
- **Database**: `rusqlite` for SQLite3 interactions.
"""
with open(os.path.join(ARC42_DIR, "04_solution_strategy.md"), "w", encoding="utf-8") as f: f.write(solution)

# arc42/06_runtime_view.md
runtime = """# 6. Runtime View

## 6.1 Server Startup Sequence
```mermaid
sequenceDiagram
    participant Main
    participant DB
    participant Runtime
    participant WebTransport
    participant Legacy UDP
    participant SSH

    Main->>DB: Check/Init Schema
    Main->>Runtime: Create BaselineRuntime
    Runtime->>DB: Load Configurations (Virtual Servers)
    Runtime->>Runtime: Build Event Pipeline (Broadcast Channels)
    Main->>WebTransport: bind(9987)
    Main->>Legacy UDP: bind(9987)
    Main->>SSH: bind(10022)
    Note over Main: Server is ready to accept connections
```

## 6.2 Permission Evaluation Flow
Whenever a client executes a command:
1. `ssh_query` / `desktop_transport` / `web_transport` converts the payload into `CommandRequest`.
2. `BaselineRuntime::dispatch()` matches the command.
3. Call `BaselineRuntime::has_permission(client_id, PERM_ID)`.
   - Checks Client-Specific Permissions.
   - If inherited: Checks Channel Group Permissions.
   - If inherited: Checks Server Group Permissions.
4. Returns boolean. If true, process command. If false, return `QueryResponse::error(insufficient_client_permissions)`.
"""
with open(os.path.join(ARC42_DIR, "06_runtime_view.md"), "w", encoding="utf-8") as f: f.write(runtime)

# arc42/07_deployment_view.md
deploy = """# 7. Deployment View

BlackTeaSpeak runs as a single static binary. It does not require a complex multi-container setup unless scaling out media routing is strictly required.

```mermaid
flowchart TD
    subgraph VM/BareMetal [Host Machine]
        Bin[blackteaspeak_server.exe]
        DB[(blackteaspeak.db)]
        TLS[(TLS Certificates)]
        Avatars[(File Transfer Cache)]
    end

    Bin --> DB
    Bin --> TLS
    Bin --> Avatars
```

**Required Ports**:
- `9987/UDP`: Legacy Voice and BTEA Media Protocol.
- `9987/TCP`: Fallback TCP connections.
- `8080/TCP`: HTTP BlackTeaWeb Server Client.
- `10022/TCP`: SSH ServerQuery Interface.
- `30303/TCP`: Avatar & Icon Transfers.
"""
with open(os.path.join(ARC42_DIR, "07_deployment_view.md"), "w", encoding="utf-8") as f: f.write(deploy)

# protocols/web_transport.md
wt = """# WebTransport Protocol Implementation

BlackTeaSpeak utilizes HTTP/3 WebTransport via the `quinn` crate to expose a low-latency pipeline to Web Browsers natively.

## Datagram Layer (Opus Audio)
WebTransport Datagrams map directly to `btea` packets. 
When a browser captures audio, it converts it to Opus frames and packs it into a raw QUIC Datagram. 
The server receives this in `web_transport.rs`, unwraps the payload, and forwards it to `BaselineRuntime::route_btea_media_to_desktop`.

## Bidirectional Streams (Commands)
Standard control commands (like fetching server lists, sending chats, updating nicknames) are handled over reliable QUIC Bidirectional Streams.
"""
with open(os.path.join(PROTO_DIR, "web_transport.md"), "w", encoding="utf-8") as f: f.write(wt)

# protocols/desktop_transport.md
dt = """# Legacy Desktop Transport Implementation

Provides backward compatibility with traditional TS3 Clients.

## Cryptography
1. Initial Handshake utilizes standard TS3 Puzzle computation.
2. Exchanges static ECDSA public keys.
3. Derives shared secret.
4. Data payloads are encrypted using standard `AES-EAX` (or `Chacha20-Poly1305` if negotiated).

The implementation is located in `desktop_transport.rs`. `desktop_crypto.rs` handles the byte manipulation and AES block processing.
"""
with open(os.path.join(PROTO_DIR, "desktop_transport.md"), "w", encoding="utf-8") as f: f.write(dt)

# integration/bot_development_guide.md
bot_guide = """# Bot & Plugin Developer Guide

BlackTeaSpeak fully supports legacy ServerQuery bots, but mandates modern secure connections and encourages the custom BTEA protocol for media bots.

## 1. Administrative Bots (Text, Roles, Kicks)
If your bot just needs to send text messages, move clients, or assign server groups, you should use the **SSH Query Transport**.

**Connecting via SSH**:
Connect to `10022` using `serveradmin` as the username and your generated query password. 

```bash
ssh serveradmin@127.0.0.1 -p 10022
```
Once connected, issue `help` to list commands. Issue `servernotifyregister event=server` to subscribe to event feeds.

## 2. Music and Media Bots
For bots streaming audio (e.g., TS3AudioBot alternatives), legacy voice connection involves extreme complexity (cryptography, puzzle solving, fake hardware IDs).

**Use the BTEA Protocol instead!**
BTEA skips puzzle solving and provides an X25519 ECDH handshake taking milliseconds. After handshake, you can blast native Opus packets via UDP and the server distributes it to both Desktop and Web clients.
(See `docs/btea_protocol.md` for specs).
"""
with open(os.path.join(INT_DIR, "bot_development_guide.md"), "w", encoding="utf-8") as f: f.write(bot_guide)

print("Exhaustive documentation generated.")
