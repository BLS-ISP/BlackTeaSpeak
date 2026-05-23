# 1. Introduction and Goals

BlackTeaSpeak is an advanced voice, text, and media server built from the ground up in Rust. It serves as an ultra-high performance, backward-compatible, and future-proof backend replacing legacy TeamSpeak 3 servers.

## 1.1 Requirements Overview
- **Zero-Latency Target**: Routing audio via WebTransport/UDP must take less than 1ms of internal processing.
- **Cross-Compatibility**: Must allow legacy TS3 clients to communicate seamlessly with modern WebRTC/WebTransport browser clients.
- **Bot Extensibility**: Complete removal of legacy Telnet in favor of a secure SSH-based ServerQuery interface. Support for native `btea` protocol bots to bypass complex TS3 puzzles.

## 1.2 Quality Goals
1. **Security**: X25519 ECDH handshakes for the `btea` protocol; strict OpenSSH Host Key management.
2. **Scalability**: Capable of handling thousands of multiplexed voice streams via `tokio::sync::broadcast` and Quinn QUIC streams.
3. **Robustness**: Complete avoidance of memory-safety vulnerabilities using Rust's ownership model.
