import { WebTransportFrame } from '../../types/protocol';

export type FrameHandler = (frame: WebTransportFrame) => void;
export type MediaPacketHandler = (
  packetType: number,
  senderClientId: number,
  payload: Uint8Array
) => void;
export type DisconnectHandler = (reason: string) => void;

export interface WebTransportConnectOptions {
  host: string;
  port: number;
  certHashBase64?: string;
  onFrame: FrameHandler;
  onMediaPacket: MediaPacketHandler;
  onDisconnect: DisconnectHandler;
}

function base64ToUint8Array(base64: string): Uint8Array {
  const clean = base64.trim();
  const binary = atob(clean);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) {
    bytes[i] = binary.charCodeAt(i);
  }
  return bytes;
}

export async function discoverServerCertHash(host?: string): Promise<{
  sha256?: string;
  webtransportPort?: number;
}> {
  const candidates: string[] = ['/cert-hash'];
  if (host) {
    const cleanHost = host.includes(':') ? host.split(':')[0] : host;
    candidates.unshift(`http://${cleanHost}:8081/cert-hash`);
    candidates.push(`https://${cleanHost}:8080/cert-hash`);
  }
  for (const url of candidates) {
    try {
      const resp = await fetch(url, { cache: 'no-store' });
      if (resp.ok) {
        const json = await resp.json();
        return {
          sha256: typeof json.sha256 === 'string' ? json.sha256 : undefined,
          webtransportPort:
            typeof json.webtransport_port === 'number' ? json.webtransport_port : undefined,
        };
      }
    } catch {
      // Try next candidate
    }
  }
  return {};
}

export class WebTransportConnection {
  private transport: WebTransport | null = null;
  private writer: WritableStreamDefaultWriter<Uint8Array> | null = null;
  private datagramWriter: WritableStreamDefaultWriter<Uint8Array> | null = null;
  private encoder = new TextEncoder();
  private decoder = new TextDecoder();
  private keepAliveInterval: number | null = null;
  private closed = false;

  public async connect(options: WebTransportConnectOptions): Promise<void> {
    this.closed = false;
    const url = `https://${options.host}:${options.port}/`;
    const wtOptions: WebTransportOptions = {};

    if (options.certHashBase64 && options.certHashBase64.trim().length > 0) {
      const hashBytes = base64ToUint8Array(options.certHashBase64);
      wtOptions.serverCertificateHashes = [
        {
          algorithm: 'sha-256',
          value: hashBytes.buffer as ArrayBuffer,
        },
      ];
    }

    if (typeof WebTransport === 'undefined') {
      throw new Error(
        'WebTransport (HTTP/3 QUIC) wird von diesem Browser nicht unterstützt. Bitte verwende einen aktuellen Chromium/Edge/Firefox Browser.'
      );
    }

    const transport = new WebTransport(url, wtOptions);
    this.transport = transport;

    await transport.ready;

    // Open primary bidirectional control stream for JSON commands & notifications
    const biStream = await transport.createBidirectionalStream();
    this.writer = biStream.writable.getWriter();
    this.datagramWriter = transport.datagrams.writable.getWriter();

    // Start reading control stream lines
    this.readControlStreamLoop(biStream.readable, options.onFrame, options.onDisconnect);

    // Start reading unreliable QUIC datagrams (Opus voice, 3D spatial coords, small video chunks)
    this.readDatagramsLoop(transport.datagrams.readable, options.onMediaPacket);

    // Start reading incoming unidirectional QUIC streams (WebCodecs video keyframes & screen share)
    this.readUnidirectionalStreamsLoop(
      transport.incomingUnidirectionalStreams,
      options.onMediaPacket
    );

    // Monitor transport closure
    transport.closed
      .then(() => {
        if (!this.closed) {
          this.cleanup();
          options.onDisconnect('WebTransport-Verbindung vom Server geschlossen');
        }
      })
      .catch((err) => {
        if (!this.closed) {
          this.cleanup();
          options.onDisconnect(`WebTransport-Fehler: ${err?.message || String(err)}`);
        }
      });

    // Keepalive heartbeat every 10 seconds
    this.keepAliveInterval = window.setInterval(() => {
      this.sendRawLine('').catch(() => {});
    }, 10_000);
  }

  public async sendFrame(frame: WebTransportFrame): Promise<void> {
    const rows =
      frame.data && frame.data.length > 0
        ? frame.data.map((row, idx) =>
            idx === 0 && frame.code ? { ...row, return_code: frame.code } : row
          )
        : frame.code
        ? [{ return_code: frame.code }]
        : [{}];
    const json = JSON.stringify({
      type: 'command',
      command: frame.command,
      return_code: frame.code ?? '',
      data: rows,
      flags: frame.options ?? [],
    });
    await this.sendRawLine(json);
  }

  public async sendRawLine(line: string): Promise<void> {
    if (!this.writer || this.closed) return;
    const bytes = this.encoder.encode(line + '\n');
    await this.writer.write(bytes);
  }

  /**
   * Sends a low-latency QUIC datagram: [packetType (1B), ...payload]
   * Packet types:
   *  1 = Opus Voice (WebCodecs AudioEncoder)
   *  2 = Camera Video Chunk (WebCodecs VideoEncoder)
   *  3 = Whisper Opus Voice
   *  4 = Screen-Share Video Chunk (WebCodecs VideoEncoder)
   *  5 = 3D Spatial Audio Coordinates (12B: f32 x, y, z)
   */
  public async sendDatagram(packetType: number, payload: Uint8Array): Promise<void> {
    if (!this.datagramWriter || this.closed) return;
    const frame = new Uint8Array(1 + payload.byteLength);
    frame[0] = packetType;
    frame.set(payload, 1);
    try {
      await this.datagramWriter.write(frame);
    } catch {
      // Ignore transient QUIC datagram backpressure drops
    }
  }

  /**
   * Sends a reliable unidirectional QUIC stream for larger WebCodecs frames (e.g. keyframes > MTU)
   */
  public async sendUnidirectionalStream(packetType: number, payload: Uint8Array): Promise<void> {
    if (!this.transport || this.closed) return;
    try {
      const writable = await this.transport.createUnidirectionalStream();
      const writer = writable.getWriter();
      const frame = new Uint8Array(1 + payload.byteLength);
      frame[0] = packetType;
      frame.set(payload, 1);
      await writer.write(frame);
      await writer.close();
    } catch {
      // Fallback to datagram if unidirectional stream creation fails
      await this.sendDatagram(packetType, payload);
    }
  }

  public close(): void {
    this.closed = true;
    this.cleanup();
    try {
      this.transport?.close({ closeCode: 0, reason: 'Client disconnected' });
    } catch {
      // Ignore
    }
    this.transport = null;
  }

  private cleanup(): void {
    if (this.keepAliveInterval !== null) {
      clearInterval(this.keepAliveInterval);
      this.keepAliveInterval = null;
    }
    try {
      this.writer?.releaseLock();
    } catch {
      // Ignore
    }
    try {
      this.datagramWriter?.releaseLock();
    } catch {
      // Ignore
    }
    this.writer = null;
    this.datagramWriter = null;
  }

  private async readControlStreamLoop(
    readable: ReadableStream<Uint8Array>,
    onFrame: FrameHandler,
    onDisconnect: DisconnectHandler
  ): Promise<void> {
    const reader = readable.getReader();
    let buffer = '';
    try {
      while (!this.closed) {
        const { value, done } = await reader.read();
        if (done) break;
        if (value) {
          buffer += this.decoder.decode(value, { stream: true });
          let newlineIndex: number;
          while ((newlineIndex = buffer.indexOf('\n')) !== -1) {
            const line = buffer.slice(0, newlineIndex).trim();
            buffer = buffer.slice(newlineIndex + 1);
            if (!line) continue;
            try {
              const parsed = JSON.parse(line);
              if (parsed && typeof parsed.command === 'string') {
                onFrame({
                  type: parsed.type,
                  command: parsed.command,
                  code: parsed.code,
                  data: Array.isArray(parsed.data) ? parsed.data : [parsed.data ?? {}],
                  options: Array.isArray(parsed.options) ? parsed.options : [],
                });
              }
            } catch {
              // Ignore non-JSON keepalive or malformed frame
            }
          }
        }
      }
    } catch (err: any) {
      if (!this.closed) {
        onDisconnect(`Control-Stream unterbrochen: ${err?.message || String(err)}`);
      }
    } finally {
      try {
        reader.releaseLock();
      } catch {
        // Ignore
      }
    }
  }

  private async readDatagramsLoop(
    readable: ReadableStream<Uint8Array>,
    onMediaPacket: MediaPacketHandler
  ): Promise<void> {
    const reader = readable.getReader();
    try {
      while (!this.closed) {
        const { value, done } = await reader.read();
        if (done) break;
        if (value && value.byteLength >= 9) {
          this.dispatchBinaryMediaFrame(value, onMediaPacket);
        }
      }
    } catch {
      // Datagram stream closed
    } finally {
      try {
        reader.releaseLock();
      } catch {
        // Ignore
      }
    }
  }

  private async readUnidirectionalStreamsLoop(
    incomingStreams: ReadableStream<ReadableStream<Uint8Array>>,
    onMediaPacket: MediaPacketHandler
  ): Promise<void> {
    const streamReader = incomingStreams.getReader();
    try {
      while (!this.closed) {
        const { value: uniStream, done } = await streamReader.read();
        if (done || !uniStream) break;
        this.readSingleUniStream(uniStream, onMediaPacket);
      }
    } catch {
      // Unidirectional stream listener closed
    } finally {
      try {
        streamReader.releaseLock();
      } catch {
        // Ignore
      }
    }
  }

  private async readSingleUniStream(
    uniStream: ReadableStream<Uint8Array>,
    onMediaPacket: MediaPacketHandler
  ): Promise<void> {
    const reader = uniStream.getReader();
    const chunks: Uint8Array[] = [];
    let totalLen = 0;
    try {
      while (true) {
        const { value, done } = await reader.read();
        if (done) break;
        if (value) {
          chunks.push(value);
          totalLen += value.byteLength;
        }
      }
      if (totalLen >= 9) {
        const combined = new Uint8Array(totalLen);
        let offset = 0;
        for (const chunk of chunks) {
          combined.set(chunk, offset);
          offset += chunk.byteLength;
        }
        this.dispatchBinaryMediaFrame(combined, onMediaPacket);
      }
    } catch {
      // Ignore aborted single uni stream
    } finally {
      try {
        reader.releaseLock();
      } catch {
        // Ignore
      }
    }
  }

  private dispatchBinaryMediaFrame(
    raw: Uint8Array,
    onMediaPacket: MediaPacketHandler
  ): void {
    const packetType = raw[0];
    const view = new DataView(raw.buffer, raw.byteOffset + 1, 8);
    // Read u64 little-endian sender_client_id (safe integer in JS up to 2^53)
    const low = view.getUint32(0, true);
    const high = view.getUint32(4, true);
    const senderClientId = high * 0x100000000 + low;
    const payload = raw.subarray(9);
    onMediaPacket(packetType, senderClientId, payload);
  }
}
