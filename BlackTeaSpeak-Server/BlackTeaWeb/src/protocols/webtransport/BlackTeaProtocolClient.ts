import { WebTransportFrame } from '../../types/protocol';
import {
  discoverServerCertHash,
  MediaPacketHandler,
  WebTransportConnection,
} from './WebTransportConnection';

interface PendingCommand {
  command: string;
  collectedFrames: WebTransportFrame[];
  resolve: (frames: WebTransportFrame[]) => void;
  reject: (err: Error) => void;
  timer: number;
}

export interface ConnectConfig {
  host: string;
  port: number;
  certHash?: string;
  nickname: string;
  serverPassword?: string;
  defaultChannel?: string;
  identityPublicKey: string;
}

export class BlackTeaProtocolClient {
  private connection = new WebTransportConnection();
  private returnCodeCounter = 1;
  private pendingCommands = new Map<string, PendingCommand>();
  private lastActiveReturnCode: string | null = null;
  private onNotificationCallback: ((frame: WebTransportFrame) => void) | null = null;
  private onMediaCallback: MediaPacketHandler | null = null;
  private onDisconnectCallback: ((reason: string) => void) | null = null;

  public setHandlers(handlers: {
    onNotification: (frame: WebTransportFrame) => void;
    onMedia: MediaPacketHandler;
    onDisconnect: (reason: string) => void;
  }): void {
    this.onNotificationCallback = handlers.onNotification;
    this.onMediaCallback = handlers.onMedia;
    this.onDisconnectCallback = handlers.onDisconnect;
  }

  public async connectAndHandshake(config: ConnectConfig): Promise<void> {
    let certHash = config.certHash?.trim();
    let port = config.port;

    if (!certHash) {
      const discovered = await discoverServerCertHash(config.host);
      if (discovered.sha256) {
        certHash = discovered.sha256;
      }
      if (!port && discovered.webtransportPort) {
        port = discovered.webtransportPort;
      }
    }

    await this.connection.connect({
      host: config.host,
      port: port || 9989,
      certHashBase64: certHash,
      onFrame: (frame) => this.handleIncomingFrame(frame),
      onMediaPacket: (packetType, senderClientId, payload) => {
        this.onMediaCallback?.(packetType, senderClientId, payload);
      },
      onDisconnect: (reason) => {
        this.rejectAllPending(reason);
        this.onDisconnectCallback?.(reason);
      },
    });

    // Step 1: handshakebegin
    await this.sendCommand('handshakebegin', [
      {
        intention: '0',
        authentication_method: 'ed25519',
        public_key: config.identityPublicKey,
      },
    ]);

    // Step 2: handshakeidentityproof
    await this.sendCommand('handshakeidentityproof', [
      {
        proof: btoa(`blackteaweb-proof:${config.identityPublicKey}`),
      },
    ]);

    // Step 3: clientinit
    await this.sendCommand('clientinit', [
      {
        client_nickname: config.nickname,
        client_version: 'BlackTeaWeb 2.0 (WebTransport + WebCodecs)',
        client_platform: 'Web (HTTP/3 QUIC)',
        client_input_hardware: '1',
        client_output_hardware: '1',
        client_default_channel: config.defaultChannel ?? '',
        client_server_password: config.serverPassword ?? '',
      },
    ]);
  }

  public async sendCommand(
    command: string,
    data: Record<string, string | number | undefined>[] = [{}],
    options: string[] = []
  ): Promise<WebTransportFrame[]> {
    const code = `rt-${this.returnCodeCounter++}`;
    this.lastActiveReturnCode = code;

    return new Promise<WebTransportFrame[]>((resolve, reject) => {
      const timer = window.setTimeout(() => {
        this.pendingCommands.delete(code);
        reject(new Error(`Befehl '${command}' Zeitüberschreitung (Code ${code})`));
      }, 8000);

      this.pendingCommands.set(code, {
        command,
        collectedFrames: [],
        resolve,
        reject,
        timer,
      });

      this.connection
        .sendFrame({
          command,
          code,
          data,
          options,
        })
        .catch((err) => {
          clearTimeout(timer);
          this.pendingCommands.delete(code);
          reject(err);
        });
    });
  }

  public sendMediaDatagram(packetType: number, payload: Uint8Array): void {
    this.connection.sendDatagram(packetType, payload);
  }

  public sendMediaUnidirectional(packetType: number, payload: Uint8Array): void {
    this.connection.sendUnidirectionalStream(packetType, payload);
  }

  public disconnect(): void {
    this.rejectAllPending('Verbindung getrennt');
    this.connection.close();
  }

  private handleIncomingFrame(frame: WebTransportFrame): void {
    if (frame.command === 'error') {
      const row = frame.data?.[0] ?? {};
      const returnCode = String(row.return_code ?? frame.code ?? '');
      const errorId = Number(row.id ?? 0);
      const errorMsg = String(row.msg ?? 'ok');
      const extraMsg = row.extra_msg ? ` (${row.extra_msg})` : '';

      const pending = returnCode
        ? this.pendingCommands.get(returnCode)
        : this.lastActiveReturnCode
        ? this.pendingCommands.get(this.lastActiveReturnCode)
        : undefined;

      if (pending) {
        clearTimeout(pending.timer);
        if (returnCode) {
          this.pendingCommands.delete(returnCode);
        } else if (this.lastActiveReturnCode) {
          this.pendingCommands.delete(this.lastActiveReturnCode);
        }

        // Empty result set (0x501 = 1281) is normal for empty lists (bans, tokens, complaints, files)
        if (errorId === 0 || errorId === 1281) {
          pending.resolve(pending.collectedFrames);
        } else {
          pending.reject(new Error(`${errorMsg}${extraMsg} (Fehler #${errorId})`));
        }
        return;
      }
    }

    // Collect frame for active pending command AND forward to notification handler
    if (this.lastActiveReturnCode) {
      const active = this.pendingCommands.get(this.lastActiveReturnCode);
      if (active) {
        active.collectedFrames.push(frame);
      }
    }

    this.onNotificationCallback?.(frame);
  }

  private rejectAllPending(reason: string): void {
    for (const [, pending] of this.pendingCommands) {
      clearTimeout(pending.timer);
      pending.reject(new Error(reason));
    }
    this.pendingCommands.clear();
  }
}
