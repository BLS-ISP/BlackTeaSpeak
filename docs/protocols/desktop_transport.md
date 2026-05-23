# Legacy Desktop Transport Implementation

Provides backward compatibility with traditional TS3 Clients.

## Cryptography
1. Initial Handshake utilizes standard TS3 Puzzle computation.
2. Exchanges static ECDSA public keys.
3. Derives shared secret.
4. Data payloads are encrypted using standard `AES-EAX` (or `Chacha20-Poly1305` if negotiated).

The implementation is located in `desktop_transport.rs`. `desktop_crypto.rs` handles the byte manipulation and AES block processing.
