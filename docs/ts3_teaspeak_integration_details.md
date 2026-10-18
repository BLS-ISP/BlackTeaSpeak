# High-Fidelity Technical Reference: TS3 & TeaSpeak Protocol Integration

This document outlines the complete architectural, mathematical, and cryptographic specifications of the TeamSpeak 3 / TeaSpeak compatibility layer built into `blackteaspeak_server`. This guide is intended to ensure future maintainability and correct implementations of the transport handshake, packet boundaries, and session bootstrap sequence.

---

## 1. Architectural Transport Architecture

The compatibility layer allows official TeamSpeak 3 desktop clients (using the legacy UDP transport) to connect directly to the Rust server runtime (`blackteaspeak_server`). It acts as a bridge, parsing proprietary TS3 binary/encrypted payloads and mapping them onto the internal shared runtime:

```
[Official TS3 Client]
        │ (UDP Transport, Port 9987)
        ▼
[blackteaspeak_server::teaspeak_transport]
   ├─ Decrypts EAX AEAD packets (dummy or session keys)
   ├─ Reassembles fragmented command packets (flag 0x10)
   └─ Dispatches commands / updates the shared server runtime
```

---

## 2. Handshake & Cryptographic Key Exchange (`ot=1`)

The compatibility layer supports the TeamSpeak 3 **New Protocol** handshake (`ot=1`). This handshake utilizes custom Curve25519 scalar multiplications, SHA-512 hashing, and AES-128 EAX authenticated encryption.

### A. Handshake Sequence
1. **SolvePuzzle**: The client solves the server's puzzle and sends its ephemeral public key ($client\_ek$) alongside its $alpha$ seed.
2. **InitExpand2**: The server responds with its ephemeral public key, its $beta$ seed, a signature proof, and the license public key chain.
3. **ClientEK**: The client performs ECDH, derives the session key, and sends a proof payload.
4. **Session Key Activation**: Both client and server encrypt/decrypt subsequent packets using derived EAX keys.

---

### B. Shared Secret Derivation ($get\_shared\_secret2$)
The server derives the shared secret by performing scalar multiplication on Curve25519. The implementation negates coordinates and flips specific bits to match custom TeamSpeak layouts:

$$\text{Scalar} \ s = \text{Scalar::from\_bytes\_mod\_order}(\text{private\_key} \ \text{with} \ \text{byte}[31] \ \&= 0\text{x}7\text{F})$$

$$\text{Point} \ P = \text{CompressedEdwardsY}(\text{public\_key} \ \text{with} \ \text{byte}[31] \ \text{XOR}= 0\text{x}80)$$

$$\text{SharedPoint} \ S = s \cdot P$$

$$\text{SharedBytes} \ B = S.\text{compress}().to\_bytes() \ \text{with} \ \text{byte}[31] \ \text{XOR}= 0\text{x}80$$

$$\text{SharedSecret} \ (64 \text{ bytes}) = \text{SHA-512}(B)$$

#### Implementation (Rust):
```rust
// 1. Copy and mask private key (byte 31 &= 0x7F)
let mut private_key_cpy = [0u8; 32];
private_key_cpy.copy_from_slice(private_key);
private_key_cpy[31] &= 0x7F;
let scalar = Scalar::from_bytes_mod_order(private_key_cpy);

// 2. Load public key and negate Y-coordinate (byte 31 ^= 0x80)
let mut pub_bytes = [0u8; 32];
pub_bytes.copy_from_slice(public_key);
pub_bytes[31] ^= 0x80;
let compressed_pub = CompressedEdwardsY(pub_bytes);
let point = compressed_pub.decompress().ok_or("Invalid Point")?;

// 3. Scalar multiplication and shared point negation
let shared_point = point * scalar;
let mut shared_bytes = shared_point.compress().to_bytes();
shared_bytes[31] ^= 0x80;

// 4. Final SHA-512 hashing to get the 64-byte session secret
let mut hasher = Sha512::new();
hasher.update(&shared_bytes);
let session_secret: [u8; 64] = hasher.finalize().into();
```

---

### C. Initialization Vector Structure ($derive\_iv\_struct$)
The Initialization Vector (IV) structure combines the $shared\_secret$, client seed ($alpha$), and server seed ($beta$):
* **Bytes 0..10**: $shared\_secret[i] \oplus alpha[i]$
* **Bytes 10..64**: $shared\_secret[10+i] \oplus beta[i]$

```rust
pub fn derive_iv_struct(shared_secret: &[u8], alpha: &[u8], beta: &[u8]) -> Vec<u8> {
    let mut iv_struct = vec![0u8; 10 + beta.len()];
    let len1 = alpha.len().min(shared_secret.len()).min(iv_struct.len());
    for i in 0..len1 {
        iv_struct[i] = shared_secret[i] ^ alpha[i];
    }
    for i in 0..beta.len() {
        iv_struct[10 + i] = shared_secret[10 + i] ^ beta[i];
    }
    iv_struct
}
```

---

### D. Session Key Derivation & AES-128 EAX Cryptography
Each packet gets encrypted or decrypted dynamically. The session key ($K$) and nonce ($N$) are derived for every single packet to prevent replay attacks:

$$\text{HashInput} = \text{DirectionByte} \ || \ (\text{packet\_type} \ \& \ 0\text{x}0\text{F}) \ || \ \text{generation\_id} \ || \ \text{iv\_struct}$$
$$\text{Hash} = \text{SHA-256}(\text{HashInput})$$

* **DirectionByte**: `0x30` for Server $\to$ Client, `0x31` for Client $\to$ Server.
* **Key** ($16$ bytes): $Hash[0..16] \oplus packet\_id$ (as big-endian bytes XORed into the first 2 bytes).
* **Nonce** ($16$ bytes): $Hash[16..32]$.

Encryption is performed using **AES-128 EAX** authenticated mode, matching TeamSpeak's custom packet framing (8-byte MAC followed by the header and cipher text).

---

## 3. The Identity Certificate & License Chain

The TeamSpeak 3 client validates the server's public key chain against hardcoded certificate parameters. The server must provide a valid chain proof during `initivexpand2`.

### A. Dynamic Parent Public Key Derivation ($parent\_pub$)
The server license chain requires key alignment at index 3. The parent public key ($parent\_pub$) must mathematically correspond to the private key scalar $identity\_sec$.
* **The Math**: The public key is generated by multiplying the private scalar $s$ by the Edwards curve basepoint $B$:

$$P = s \cdot B$$

* **The Flipping Quirks**: Once the point is compressed to bytes, TeamSpeak expects the MSB of the last byte to be flipped ($byte[31] \oplus= 0\text{x}80$), converting it to TeamSpeak's custom format:

```rust
use curve25519_dalek::constants::ED25519_BASEPOINT_TABLE;
use std::ops::Mul;

let s = Scalar::from_bytes_mod_order(identity_sec);
let r_point = (&ED25519_BASEPOINT_TABLE).mul(&s);
let mut parent_pub = r_point.compress().to_bytes();
parent_pub[31] ^= 0x80; // Flip last byte MSB for TeamSpeak format compatibility
```

---

### B. Custom Ed25519 Signatures ($sign\_with\_raw\_scalar$)
Signature validation on the client side requires standard Edwards curve arithmetic combined with custom byte representation tweaks (sign-bit negation on $R$):

```rust
pub fn sign_with_raw_scalar(
    private_scalar_bytes: &[u8; 32],
    public_key_bytes: &[u8; 32],
    message: &[u8],
) -> [u8; 64] {
    let s = Scalar::from_bytes_mod_order(*private_scalar_bytes);
    let mut r_bytes = [0u8; 64];
    rand::thread_rng().fill_bytes(&mut r_bytes);
    let r = Scalar::from_bytes_mod_order_wide(&r_bytes);

    let r_point = (&ED25519_BASEPOINT_TABLE).mul(&r);
    let mut r_compressed = r_point.compress().to_bytes();
    
    let mut pub_unflipped = *public_key_bytes;
    pub_unflipped[31] ^= 0x80; // Unflip public key for signature hashing AAD

    let mut hasher = Sha512::new();
    hasher.update(&r_compressed);
    hasher.update(&pub_unflipped);
    hasher.update(message);
    let k_scalar = Scalar::from_bytes_mod_order_wide(&hasher.finalize().into());

    let s_scalar = r + (k_scalar * s);
    r_compressed[31] ^= 0x80; // Flip R's sign bit for TS3 custom transport format

    let mut signature = [0u8; 64];
    signature[0..32].copy_from_slice(&r_compressed);
    signature[32..64].copy_from_slice(&s_scalar.to_bytes());
    signature
}
```

---

## 4. Packet Reassembly & Boundary Rules

Large commands (like `clientinit` or `channellist`) are fragmented. The server implements a stateful boundary reassembly parser checking the boundary flag (`0x10`):

```
       Segment 1                     Segment 2                     Segment 3
┌───────────────────────┐     ┌───────────────────────┐     ┌───────────────────────┐
│ Flag: 0x10 (Fragment) │     │ Flag: 0x00            │     │ Flag: 0x10 (Fragment) │
│ [First Chunk of Data] │     │ [Middle Chunk of Data]│     │ [Last Chunk of Data]  │
└───────────────────────┘     └───────────────────────┘     └───────────────────────┘
           │                              │                             │
           └──────────────────────────────┼─────────────────────────────┘
                                          ▼
                         Decompressed & Assembled UTF-8 Command
```

### Boundary Reassembly Rules:
1. **Initiate**: When a packet is received with flag `0x10` set while `session.command_fragment_active` is `false`, a new assembly sequence begins. The buffer is cleared, and the payload is written.
2. **Accumulate**: When a packet is received with flag `0x10` cleared while `session.command_fragment_active` is `true`, it represents a middle chunk. The payload is appended to `session.command_fragment_buffer`.
3. **Finalize**: When a packet is received with flag `0x10` set while `session.command_fragment_active` is `true`, it is the final chunk. The payload is appended, the active flag is cleared, and the fully assembled payload is passed to the command parser.

---

## 5. Bootstrap Protocol Flow

The client demands a strict logical layout during initialization. If a packet is sent out of order, the client's internal connection state machine invalidates the session (`Error while parsing packet: not connected`).

### The Correct Sequence Checklist:
1. **`initserver`**: Sends virtual server configurations and sets the own client ID (`aclid=1`).
2. **`channellist`**: Populates the channels (must contain the default channel ID, e.g. `cid=1`).
3. **`notifychannelsubscribed`**: Subscribes the client to the channel tree.
4. **`notifycliententerview`**: Registers client 1 inside channel 1.
   - **MUST** target `clid=1` (the client's actual ID).
   - **MUST** set `reasonid=2` (`VREASON_SYSTEM`) to denote a system bootstrap event, preventing client-side self-collision assertions.
   - **MUST** set `client_totalconnections=1`.
5. **`channellistfinished`**: Informs the client that initial sync is complete.
6. **`notifyservergrouplist`** / **`notifychannelgrouplist`**: Registers permissions groups.
7. **`notifyclientneededpermissions`**: Updates required permissions mappings.

---

## 6. Post-Connection & Keep-Alives

Once bootstrap is finalized, the client starts polling status updates. The compatibility layer intercepts these queries to keep the client session active:
* **`clientgetvariables`**: Responds with a customized, escaped `notifyclientupdated` matching the client's cached nickname and unique identifier.
* **`servergetvariables`**: Responds with a formatted `notifyserverupdated` packet containing uptime and slots.
* **Pings & Pong**: Incoming `0x84` (Unencrypted | Ping) packets are replied to with `0x85` (Unencrypted | Pong) containing the mirrored packet ID.
* **Control Keep-Alives**: Any unencrypted control packets (`flags = 0x86` / `0x06`) are processed cleanly to maintain connection metrics.
