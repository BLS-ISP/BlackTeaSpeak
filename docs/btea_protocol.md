# BTEA Protocol Documentation

BlackTeaSpeak introduces the BTEA protocol, a customized and simplified handshake mechanism designed to supersede the legacy TeamSpeak 3 handshake process. 

This protocol is tailored for building custom, high-performance clients, bots, and plugins. It avoids complex challenges like TS3 puzzle solving and license verification while keeping the subsequent encrypted data transmission identical to the familiar TS3 format.

## Handshake Process

The BTEA handshake is a single round-trip key exchange over UDP utilizing the **X25519** elliptic curve Diffie-Hellman (ECDH) key agreement protocol.

### 1. Initial Request (Client -> Server)

To initiate a connection, the client generates a new ephemeral X25519 keypair and sends a `BtInitRequest` packet to the server.

**Packet Structure (`BtInitRequest`):**

| Offset | Length | Type | Description |
| :--- | :--- | :--- | :--- |
| 0 | 8 bytes | ASCII String | Magic Bytes: `BTEAINIT` |
| 8 | 1 byte | `u8` | Packet Type: `0x01` (Init Request) |
| 9 | 32 bytes | Byte Array | Client's Public X25519 Key |
| 41 | Variable | ASCII String | Optional: payload or client version info (e.g., `clientinit client_version=BTEA_TEST`) |

*Note: The total size of the minimum valid `BtInitRequest` is 41 bytes.*

### 2. Server Response (Server -> Client)

Upon receiving a valid `BtInitRequest`, the server generates its own ephemeral X25519 keypair, calculates the shared secret using the client's public key, and replies with a `BtInitResponse`.

**Packet Structure (`BtInitResponse`):**

| Offset | Length | Type | Description |
| :--- | :--- | :--- | :--- |
| 0 | 8 bytes | ASCII String | Magic Bytes: `BTEAINIT` |
| 8 | 1 byte | `u8` | Packet Type: `0x02` (Init Response) |
| 9 | 32 bytes | Byte Array | Server's Public X25519 Key |

### 3. Session Key Derivation

Both the client and the server independently derive the shared symmetric encryption keys for the session using SHA-512 hashing, derived directly from the computed X25519 shared secret.

The derivation requires computing two hashes:
- `hasher1`: Generates the Initialization Vector (IV).
- `hasher2`: Generates the Session Shared Secret.

#### Step 3.1: Derive IV (`hasher1`)
Compute the SHA-512 hash of:
1. The server's public key (32 bytes).
2. The calculated X25519 shared secret (32 bytes).

Take the first **64 bytes** (the full SHA-512 digest) of the result. This forms the `iv_struct` (Initialization Vector base structure).

#### Step 3.2: Derive Session Key (`hasher2`)
Compute the SHA-512 hash of:
1. The client's public key (32 bytes).
2. The calculated X25519 shared secret (32 bytes).

Take the first **64 bytes** (the full SHA-512 digest) of the result. This forms the `session_shared_secret`.

### 4. Subsequent Data Transmission

Once the keys are derived, the handshake state transitions to `Connected`. 
All further communication strictly mimics standard TS3 encrypted packets, but uses custom BTEA flags to identify packet types.

**BTEA Packet Flags (`flags & 0x0F`):**
*   `0x00`: Voice (Standard TS3 compatibility)
*   `0x02`: Command Packet (Standard TS3 strings like `clientinit`, `channellist`)
*   `0x0A`: Native BTEA Voice Packet (Opus encoded raw stream)
*   `0x0B`: Native BTEA Video Packet (VP8/H264 encoded raw stream)

*   Packet Headers and MACs are handled identically to TS3.
*   The established session keys are used directly to decrypt standard TS3 payloads using AES-EAX / AES-GCM as implemented by the existing protocol.
*   Packet Command parsing (e.g., `clientinit`, `serverinit`) continues through the TS3 command parser format over the encrypted channel.
*   Native BTEA Media Packets (`0x0A`, `0x0B`) bypass string parsing and are directly broadcasted to all clients in the same channel as the sender using standard server-side UDP multiplexing.

### Native BTEA Video Integration (0x0B)
To facilitate bridging BTEA Desktop video with WebRTC Web clients without transcoding, the `0x0B` Native BTEA Video Packet payload must encapsulate **Standard RTP Payload Fragments** (e.g., RFC 6184 for H.264 or RFC 7741 for VP8).

**Video Packet Layout (`0x0B` Payload):**
| Offset | Length | Type | Description |
| :--- | :--- | :--- | :--- |
| 0 | 1 byte | `u8` | Target Media Flag: `0x00` (Camera) or `0x01` (Screenshare) |
| 1 | Variable | Byte Array | Raw RTP Payload Segment (no RTP headers needed) |

The server automatically extracts this RTP payload segment, prepends the appropriate WebRTC RTP headers (Sequence, SSRC, Timestamp), and pipes it to Web clients.

**Control Commands:**
Since UDP guarantees no delivery, clients can request a new keyframe via the Command channel (`0x02`):
*   `videorequestkeyframe clid=<target_client_id> flag=<0 or 1>`
The server forwards this command or generates it automatically when a WebRTC Web client drops a packet and issues a PLI.

## Security Considerations
*   **Ephemeral Keys**: The handshake provides Forward Secrecy. Each connection negotiates entirely new, random key pairs. 
*   **Performance**: X25519 provides ultra-fast curve operations compared to standard RSA/ECDSA key verifications.
*   **Zero Puzzle Verification**: Replaces the resource-intensive legacy puzzle verification phase entirely, drastically cutting down client connection latency.

## Writing a Custom Bot / Client
To connect a custom plugin to BlackTeaSpeak via BTEA:

1. Import a standard cryptography library for your language that supports **X25519** and **SHA-512**.
2. Generate an ephemeral 32-byte private scalar and its corresponding public point.
3. Send a UDP packet starting with `BTEAINIT\x01` + `[32 byte public key]`.
4. Wait for the `BTEAINIT\x02` + `[32 byte server public key]` UDP response.
5. Derive the `iv` and `session_secret` locally.
6. Encrypt the standard `clientinit` command with the derived symmetric key and send it using the standard TS3 Encrypted Packet Format.

A reference implementation in Rust is available in `src/bin/test_btea_client.rs`.
