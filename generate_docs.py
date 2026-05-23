import os
import re

def ensure_dir(path):
    if not os.path.exists(path):
        os.makedirs(path)

DOCS_DIR = "docs"
ARC42_DIR = os.path.join(DOCS_DIR, "arc42")
REF_DIR = os.path.join(DOCS_DIR, "reference")
PROTO_DIR = os.path.join(DOCS_DIR, "protocols")

ensure_dir(ARC42_DIR)
ensure_dir(REF_DIR)
ensure_dir(PROTO_DIR)

# 1. Parse src/runtime.rs for query commands
commands = []
try:
    with open("BlackTeaSpeak-Server/src/runtime.rs", "r", encoding="utf-8") as f:
        src = f.read()
        dispatch_match = re.search(r'fn dispatch\(.*?match request\.command\.as_str\(\) \{(.*?)\n\s+\}', src, re.DOTALL)
        if dispatch_match:
            lines = dispatch_match.group(1).split('\n')
            for line in lines:
                m = re.search(r'"([a-z]+)"\s*=>', line)
                if m:
                    commands.append(m.group(1))
except Exception as e:
    print(f"Error parsing runtime.rs: {e}")

# 2. Parse src/database.rs for tables
tables = {}
try:
    with open("BlackTeaSpeak-Server/src/database.rs", "r", encoding="utf-8") as f:
        src = f.read()
        table_matches = re.findall(r'CREATE TABLE IF NOT EXISTS ([a-zA-Z_]+)\s*\((.*?)\)', src, re.DOTALL)
        for t, cols in table_matches:
            col_list = [c.strip() for c in cols.split(',') if c.strip() and not c.strip().startswith('FOREIGN') and not c.strip().startswith('PRIMARY')]
            tables[t] = col_list
except Exception as e:
    print(f"Error parsing database.rs: {e}")

# 3. Generate ARC42 Introduction
intro = """# 1. Introduction and Goals

BlackTeaSpeak is an advanced voice, text, and media server built from the ground up in Rust. It serves as an ultra-high performance, backward-compatible, and future-proof backend replacing legacy TeamSpeak 3 servers.

## 1.1 Requirements Overview
- **Zero-Latency Target**: Routing audio via WebTransport/UDP must take less than 1ms of internal processing.
- **Cross-Compatibility**: Must allow legacy TS3 clients to communicate seamlessly with modern WebRTC/WebTransport browser clients.
- **Bot Extensibility**: Complete removal of legacy Telnet in favor of a secure SSH-based ServerQuery interface. Support for native `btea` protocol bots to bypass complex TS3 puzzles.

## 1.2 Quality Goals
1. **Security**: X25519 ECDH handshakes for the `btea` protocol; strict OpenSSH Host Key management.
2. **Scalability**: Capable of handling thousands of multiplexed voice streams via `tokio::sync::broadcast` and Quinn QUIC streams.
3. **Robustness**: Complete avoidance of memory-safety vulnerabilities using Rust's ownership model.
"""
with open(os.path.join(ARC42_DIR, "01_introduction_and_goals.md"), "w", encoding="utf-8") as f:
    f.write(intro)

# 4. Generate ARC42 System Context
context = """# 3. System Scope and Context

The BlackTeaSpeak server acts as a centralized routing and state management system.

## 3.1 Business Context

```mermaid
flowchart TD
    subgraph Clients
        Web[Web Client (Browser)]
        Desktop[TS3 Legacy Client]
        Bot[ServerQuery Bot]
        MediaBot[BTEA Music Bot]
    end

    Web <-->|WebTransport (QUIC)| Server[BlackTeaSpeak Server]
    Desktop <-->|UDP / TCP (AES-EAX)| Server
    Bot <-->|SSH (Port 10022)| Server
    MediaBot <-->|BTEA Protocol (UDP/TCP)| Server
```

## 3.2 Technical Context
- **Port 9987 (UDP/TCP)**: Desktop Compat Transport & WebTransport Datagrams.
- **Port 10022 (TCP)**: SSH Query Transport (powered by `russh`).
- **Port 30303 (TCP)**: File Transfer server (custom HTTP-like transfer).
- **Database**: SQLite3 local persistent storage (`blackteaspeak.db`).
"""
with open(os.path.join(ARC42_DIR, "03_system_scope_and_context.md"), "w", encoding="utf-8") as f:
    f.write(context)

# 5. Generate ARC42 Building Blocks
blocks = """# 5. Building Block View

## Level 1: Whitebox Overall System

```mermaid
flowchart LR
    subgraph Network Transports
        WT[WebTransport]
        Legacy[Desktop Transport]
        SSH[SSH Query Transport]
        FT[File Transfer]
    end

    subgraph State Management
        RT[BaselineRuntime]
        IMS[InMemoryStore]
    end

    subgraph Persistence
        DB[(SQLite Database)]
        FS[(Local File System)]
    end

    WT --> RT
    Legacy --> RT
    SSH --> RT
    FT --> RT

    RT --> IMS
    RT --> DB
    FT --> FS
```

### Components
1. **`BaselineRuntime` (`src/runtime.rs`)**: Master arbiter. Responsible for executing commands, verifying permissions, and routing media using `route_btea_media_to_desktop`.
2. **`InMemoryStore`**: Manages ephemeral objects like `OnlineClient`, active connections, and `music_bots` tracking.
3. **`Desktop Transport`**: Demultiplexes standard UDP payloads using Chacha20-Poly1305.
4. **`WebTransport`**: Converts standard HTTP/3 QUIC Datagrams into standard `btea` media payloads.
"""
with open(os.path.join(ARC42_DIR, "05_building_block_view.md"), "w", encoding="utf-8") as f:
    f.write(blocks)

# 6. Generate Reference Commands
cmd_ref = "# ServerQuery Commands Reference\\n\\nThe SSH interface (Port 10022) provides a TS3-compatible command suite.\\n\\n## Supported Commands\\n\\n"
for cmd in commands:
    cmd_ref += f"- `{cmd}`\n"
cmd_ref += "\\n*Note: These commands use TS3 syntax escaping (e.g., `\\\\s` for space). Execute via standard SSH execution or interactive shell.*\\n"
with open(os.path.join(REF_DIR, "query_commands.md"), "w", encoding="utf-8") as f:
    f.write(cmd_ref)

# 7. Generate Database Schema Reference
db_ref = "# SQLite Database Schema Reference\\n\\nAll persistent configurations are stored in `blackteaspeak.db`. Integrators should generally use SSH Query, but read-only queries can safely access the database.\\n\\n"
for t, cols in tables.items():
    db_ref += f"## Table: `{t}`\n"
    for c in cols:
        db_ref += f"- `{c}`\n"
    db_ref += "\n"
with open(os.path.join(REF_DIR, "database_schema.md"), "w", encoding="utf-8") as f:
    f.write(db_ref)

# 8. Generate Index
index = """# BlackTeaSpeak Documentation

Welcome to the comprehensive documentation for the BlackTeaSpeak server.

## arc42 Architecture
- [Introduction and Goals](arc42/01_introduction_and_goals.md)
- [System Scope and Context](arc42/03_system_scope_and_context.md)
- [Building Block View](arc42/05_building_block_view.md)

## Reference
- [ServerQuery Commands](reference/query_commands.md)
- [Database Schema](reference/database_schema.md)

## Protocols
- [BTEA Protocol Specification](btea_protocol.md)
"""
with open(os.path.join(DOCS_DIR, "index.md"), "w", encoding="utf-8") as f:
    f.write(index)

print("Documentation generated successfully.")
