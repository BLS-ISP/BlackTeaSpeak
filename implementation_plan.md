# Replace WebRTC with WebTransport + WebCodecs

This plan outlines the architecture for dropping the complex legacy WebRTC bridging system and replacing it with a modern, high-performance **WebTransport** endpoint. This completely eliminates the need for ICE negotiation, SDP blobs, and strict RTP wrapping. 

## User Review Required

> [!WARNING]
> This requires removing the newly built `rtc.rs` module and stripping out the `webrtc` dependencies from `Cargo.toml`. We will be relying entirely on the browser's native `WebTransport` and `WebCodecs` APIs.

## Open Questions

> [!IMPORTANT]
> 1. **Port Selection:** Should the WebTransport QUIC endpoint bind to the same port as the WebServer (e.g., `8080` UDP) or a dedicated port (e.g., `8081` UDP)?
> 2. **Client Implementation:** Do we have any existing React components for rendering raw video frames to a `<canvas>`, or should I write the complete WebCodecs boilerplate from scratch in the web client?

## Proposed Changes

### Server Implementation

#### [MODIFY] [Cargo.toml](file:///d:/projekt/BlackTeaSpeak/BlackTeaSpeak-Server/Cargo.toml)
- Remove `webrtc`, `webrtc-util`, `rtc-shared`, `rtc`, and `rtc-rtp`.
- Add `wtransport = "0.7.1"`.

#### [DELETE] [rtc.rs](file:///d:/projekt/BlackTeaSpeak/BlackTeaSpeak-Server/src/rtc.rs)
- Remove the entire WebRTC manager and all associated bridging logic.

#### [NEW] `BlackTeaSpeak-Server/src/webtransport_quic.rs`
- Implement a `wtransport::Endpoint` that listens for incoming QUIC connections.
- Accept incoming datagram sessions from authenticated web clients.
- Subscribe to the runtime's media bus and directly forward raw BTEA video (`0x0B`) and voice (`0x0A`) payloads as QUIC datagrams to the connected web clients.

#### [MODIFY] [runtime.rs](file:///d:/projekt/BlackTeaSpeak/BlackTeaSpeak-Server/src/runtime.rs)
- Rename `rtc_btea_media_tx` to `quic_media_tx`.

#### [MODIFY] [desktop_transport.rs](file:///d:/projekt/BlackTeaSpeak/BlackTeaSpeak-Server/src/desktop_transport.rs)
- Remove the WebRTC RTP payload wrapper assumptions. We can now just forward the raw encoded chunks directly!

### Client Implementation

#### [NEW] `BlackTeaSpeak-Client/src/WebTransportManager.ts`
- Implement the `new WebTransport("https://server:8080")` connection loop.
- Read incoming datagrams asynchronously.

#### [NEW] `BlackTeaSpeak-Client/src/MediaDecoder.ts`
- Initialize standard `VideoDecoder` (for VP8/H264) and `AudioDecoder` (for Opus).
- Feed raw datagram chunks into the decoders.
- Output decoded `VideoFrame`s to a dedicated `<canvas>` reference in the UI.

## Verification Plan

### Automated Tests
- Validate successful compilation of the server without WebRTC dependencies.
- Ensure `wtransport` binds successfully.

### Manual Verification
- Deploy the updated server and connect the React Web Client.
- Verify that the WebTransport connection negotiates instantly (no ICE delays).
- Stream dummy BTEA packets and verify that the `<canvas>` successfully renders the frames via `WebCodecs`.
