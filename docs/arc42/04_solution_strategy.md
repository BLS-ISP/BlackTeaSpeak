# 4. Solution Strategy

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
