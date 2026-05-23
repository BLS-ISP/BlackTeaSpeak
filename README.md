# BlackTeaSpeak

**BlackTeaSpeak** is a modern, high-performance voice, text, and media routing server built entirely in Rust. Designed to replace legacy TeamSpeak 3 backends, BlackTeaSpeak offers unparalleled backward compatibility with legacy desktop clients while introducing native WebTransport and WebCodecs support for modern browser-based integrations.

## Features

- **Legacy Backward Compatibility**: Connect using traditional TS3 clients. The server supports standard AES-EAX and Chacha20-Poly1305 encryption payloads.
- **WebTransport / WebCodecs (Port 9987)**: Run native, zero-installation clients in any modern web browser using HTTP/3 QUIC datagrams and low-latency Opus audio streams.
- **BTEA Protocol**: A lightweight, puzzle-free, X25519 ECDH-based handshake protocol for high-performance third-party bots and media routing (e.g., Music Bots).
- **Secure ServerQuery**: Legacy unencrypted Telnet has been entirely removed and replaced with a highly secure, port 10022 SSH interface (powered by `russh`).
- **File Transfer Support (Port 30303)**: Built-in fast avatar and icon hosting.
- **Asynchronous & Thread-Safe**: Built on the Tokio asynchronous runtime with an extremely low-latency event pub/sub routing core.
- **Persistent Data**: Zero-configuration, local SQLite database storage for users, channels, and permissions.

## Documentation

Extensive **arc42** architectural and integration documentation is provided in the `/docs` directory.
Start by viewing [`docs/index.md`](docs/index.md) to explore:
- High-level System Context and Component Diagrams
- The BTEA Protocol Specification
- Integration guides for 3rd-party Music Bots and ServerQuery scripts
- Internal SQLite Schema mappings

## Building & Running

### Server
```bash
cd BlackTeaSpeak-Server
cargo build --release
cargo run --release
```

### Web / Desktop Client
```bash
cd BlackTeaSpeak-Client
npm install
# Run Web interface
npm run dev
# Run Tauri desktop app
npm run tauri dev
```

## Contributing
BlackTeaSpeak is currently in the **INDEV** phase. All architectural flows, protocol packets, and schema interactions are documented in the `docs/` tree. Please consult the documentation before opening pull requests to ensure changes align with the overarching state machine design.
