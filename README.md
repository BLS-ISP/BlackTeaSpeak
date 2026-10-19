# BlackTeaSpeak

**BlackTeaSpeak** is a modern, high-performance voice, text, and media routing server built entirely in Rust. Designed to replace legacy TeamSpeak 3 backends, BlackTeaSpeak offers unparalleled backward compatibility with legacy desktop clients while introducing native WebTransport and WebCodecs support for modern browser-based integrations.

> [!CAUTION]
> **SECURITY WARNING – Counterfeit "BlackTeaSpeak" Binaries & Unofficial Domains (`blackteaspeak.com`)**
>
> Counterfeit Windows server archives (`win.zip` containing `BlackTeaSpeakServer.exe`, `v1.0.2-OFF`) are being distributed by third parties (`Mr. Ali / Mirarus` / `Ali Güçlü`) using the **BlackTeaSpeak** name.
>
> * **Not Affiliated:** The official **BlackTeaSpeak** project (`github.com/BLS-ISP/BlackTeaSpeak` / `github.com/GeneraBlack/BlackTeaSpeak`) is written from scratch in **Rust** and **never** phones home to any license server. We do **not** own or operate `blackteaspeak.com` or `license.blackteaspeak.com`.
> * **What the Counterfeit Binary Actually Is:** Reverse engineering confirms that `BlackTeaSpeakServer.exe` (SHA256: `4fa9b70233665882c355cbf6d4bec56cd59d90182aa0d4d528e11aeae1484192`) is a modified C++ build of the legacy *TeaSpeak* codebase with strings find-and-replaced to `BlackTeaSpeak` (including typos such as `Built-in License | BlackBlackTeaSpeak` at offset `0x9b7ba8`).
> * **Security Risks to Users of the Counterfeit Archive:**
>   1. **Hidden Telemetry & Remote Killswitch:** The binary contains XOR-obfuscated strings (`strobf` at offsets `0x9bfb18` and `0x9bfda0`) that silently connect to `license.blackteaspeak.com:27786` (`164.132.105.158`, hosted on OVH) to transmit server/client telemetry and allow the remote operator to forcibly shut down or blacklist any running server instance (`"Kaçak Lisans Kullanımı Tespit Edildi!"` / `"Stopping server!"`).
>   2. **Remote TLS Private Key Injection:** The server accepts and installs default Web-TLS certificates and private keys pushed remotely from `164.132.105.158` (`setWebCertRoot`), allowing the third-party operator to intercept encrypted web traffic.
>   3. **Pre-packaged Private Keys:** The archive ships with static, pre-generated SSL private keys (`certs/default_privatekey.pem` and `certs/query_privatekey.pem`) known to the archive distributor.
>
> **Only download BlackTeaSpeak from this official repository (`github.com/BLS-ISP/BlackTeaSpeak`).**

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

## License / Lizenz

**Copyright © 2026 BLS-ISP / GeneraBlack. All rights reserved.**

BlackTeaSpeak is distributed under the **[BlackTeaSpeak Proprietary Binary Use & Source-Available License](LICENSE)** (this is **not** an open-source license):

- **Permitted (Erlaubt):** Downloading and executing the unmodified **Official Binaries** for personal, private, and non-commercial use (e.g., private community or gaming voice servers free of charge), as well as inspecting the source code in this repository for transparency and security auditing.
- **Strictly Prohibited Without a Written License Agreement (Strengstens untersagt ohne Lizenzvertrag):**
  1. **No External Development or Modification:** Modifying, adapting, patching, rebranding, or creating external forks or derivative works outside the official repository is prohibited.
  2. **No Redistribution:** Redistributing, mirroring, re-hosting, or bundling the source code or binaries on third-party websites or archives is prohibited.
  3. **No Commercial Use:** Renting, leasing, paid hosting (ATHP / Game-Server / Cloud Hosting), SaaS offerings, or any commercial/business use without a separate written **Commercial License Agreement** with the copyright holder is strictly prohibited.

See [`LICENSE`](LICENSE) for the full binding German and English license text.

