# WebTransport Protocol Implementation

BlackTeaSpeak utilizes HTTP/3 WebTransport via the `quinn` crate to expose a low-latency pipeline to Web Browsers natively.

## Datagram Layer (Opus Audio)
WebTransport Datagrams map directly to `btea` packets. 
When a browser captures audio, it converts it to Opus frames and packs it into a raw QUIC Datagram. 
The server receives this in `web_transport.rs`, unwraps the payload, and forwards it to `BaselineRuntime::route_btea_media_to_desktop`.

## Bidirectional Streams (Commands)
Standard control commands (like fetching server lists, sending chats, updating nicknames) are handled over reliable QUIC Bidirectional Streams.
