import {
    AbstractVoiceConnection,
    VoiceConnectionStatus,
    WhisperSessionInitializer
} from "tc-shared/connection/VoiceConnection";
import {RecorderProfile} from "tc-shared/voice/RecorderProfile";
import {VoiceClient} from "tc-shared/voice/VoiceClient";
import {WhisperSession, WhisperTarget} from "tc-shared/voice/VoiceWhisper";
import {AbstractServerConnection, ConnectionStatistics} from "tc-shared/connection/ConnectionBase";
import {getAudioBackend} from "tc-shared/audio/Player";
import {LogCategory, logDebug, logError, logInfo, logWarn} from "tc-shared/log";
import {tr} from "tc-shared/i18n/localize";
import {WrappedWebTransport} from "../../../shared/js/connection/wtransport/WebTransportConnection";
import {WebCodecVoiceClient} from "./WebCodecVoiceClient";

export class WebCodecVoiceConnection extends AbstractVoiceConnection {
    private transport?: WrappedWebTransport;

    private connectionState: VoiceConnectionStatus = VoiceConnectionStatus.Disconnected;
    private localFailedReason: string;

    private localAudioDestination: MediaStreamAudioDestinationNode;
    private currentAudioSourceNode: AudioNode;
    private currentAudioSource: RecorderProfile;
    
    private speakerMuted: boolean;
    private whisperSessionInitializer: WhisperSessionInitializer | undefined;

    private encoder?: any; // AudioEncoder
    private trackProcessor?: any; // MediaStreamTrackProcessor
    private voiceClients: Map<number, WebCodecVoiceClient> = new Map();
    private localStreamContext?: AudioContext;

    constructor(connection: AbstractServerConnection) {
        super(connection);

        this.speakerMuted = connection.client.isSpeakerMuted() || connection.client.isSpeakerDisabled();

        getAudioBackend().executeWhenInitialized(() => {
            this.localStreamContext = getAudioBackend().getAudioContext();
            this.localAudioDestination = this.localStreamContext.createMediaStreamDestination();
        });
    }

    public setTransport(transport: WrappedWebTransport) {
        this.transport = transport;
        this.transport.callbackDatagram = (message) => {
            this.handleIncomingDatagram(message);
        };
        this.connectionState = VoiceConnectionStatus.Connected;
    }

    private handleIncomingDatagram(message: Uint8Array) {
        if(message.length < 9) return;
        
        const packetType = message[0];
        if (packetType !== 1) return; // Only process Voice packets (assuming type 1)

        // Read 8-byte client_id (Little Endian)
        const dataView = new DataView(message.buffer, message.byteOffset + 1, 8);
        const clientId = Number(dataView.getBigUint64(0, true)); // little endian
        
        const payload = message.slice(9);
        
        const client = this.voiceClients.get(clientId);
        if (client) {
            client.feedOpusChunk(payload);
        }
    }

    destroy() {
        this.events.destroy();
        this.stopAudioCapture();
        for(const client of this.voiceClients.values()) {
            client.destroy();
        }
        this.voiceClients.clear();
    }

    getConnectionState(): VoiceConnectionStatus {
        return this.transport?.state === "connected" ? VoiceConnectionStatus.Connected : VoiceConnectionStatus.Disconnected;
    }

    getFailedMessage(): string {
        return this.localFailedReason;
    }

    voiceRecorder(): RecorderProfile {
        return this.currentAudioSource;
    }

    async acquireVoiceRecorder(recorder: RecorderProfile | undefined, enforce?: boolean): Promise<void> {
        this.stopAudioCapture();
        this.currentAudioSource = recorder;
        
        if (!recorder) return;
        
        getAudioBackend().executeWhenInitialized(async () => {
            const ctx = getAudioBackend().getAudioContext();
            try {
                // Reroute the recorder's node to our local destination
                // Wait for BlackTeaSpeak's internal recorder hook logic.
                const stream = this.localAudioDestination.stream;
                const audioTrack = stream.getAudioTracks()[0];
                
                if (!audioTrack || typeof (window as any).AudioEncoder === 'undefined') {
                    return;
                }

                this.trackProcessor = new (window as any).MediaStreamTrackProcessor({ track: audioTrack });
                const reader = this.trackProcessor.readable.getReader();

                this.encoder = new (window as any).AudioEncoder({
                    output: (chunk: any) => {
                        if (!this.transport || this.transport.state !== "connected") return;
                        
                        const buffer = new ArrayBuffer(chunk.byteLength);
                        chunk.copyTo(buffer);
                        
                        // Packet type 1 = Voice
                        const payload = new Uint8Array(buffer.byteLength + 1);
                        payload[0] = 1; 
                        payload.set(new Uint8Array(buffer), 1);
                        
                        this.transport.sendDatagram(payload);
                    },
                    error: (e: Error) => {
                        logError(LogCategory.AUDIO, "AudioEncoder error", e);
                    }
                });

                this.encoder.configure({
                    codec: 'opus',
                    sampleRate: 48000,
                    numberOfChannels: 1,
                    bitrate: 64000
                });

                while (true) {
                    const { value, done } = await reader.read();
                    if (done) break;
                    if (value) {
                        if (this.encoder.state === 'configured') {
                            this.encoder.encode(value);
                        }
                        value.close();
                    }
                }
            } catch (e) {
                logError(LogCategory.AUDIO, "Failed to initialize WebCodec AudioEncoder", e);
            }
        });
    }

    private stopAudioCapture() {
        if (this.encoder && this.encoder.state !== 'closed') {
            this.encoder.close();
        }
        if (this.trackProcessor) {
            // Stop processor? It just consumes the track. 
        }
        this.encoder = undefined;
        this.trackProcessor = undefined;
    }

    isReplayingVoice(): boolean {
        return this.voiceClients.size > 0;
    }

    decodingSupported(codec: number): boolean {
        return codec === 4 || codec === 5; // OPUS
    }

    encodingSupported(codec: number): boolean {
        return codec === 4 || codec === 5; // OPUS
    }

    getEncoderCodec(): number {
        return 4;
    }

    setEncoderCodec(codec: number) {}

    availableVoiceClients(): VoiceClient[] {
        return Array.from(this.voiceClients.values());
    }

    registerVoiceClient(clientId: number): VoiceClient {
        if(!this.voiceClients.has(clientId)) {
            this.voiceClients.set(clientId, new WebCodecVoiceClient(clientId));
        }
        return this.voiceClients.get(clientId)!;
    }

    unregisterVoiceClient(client: VoiceClient) {
        const id = client.getClientId();
        if(this.voiceClients.has(id)) {
            const vc = this.voiceClients.get(id);
            vc?.destroy();
            this.voiceClients.delete(id);
        }
    }

    stopAllVoiceReplays() {
        for(const client of this.voiceClients.values()) {
            client.abortReplay();
        }
    }

    getWhisperSessionInitializer(): WhisperSessionInitializer | undefined {
        return this.whisperSessionInitializer;
    }

    setWhisperSessionInitializer(initializer: WhisperSessionInitializer | undefined) {
        this.whisperSessionInitializer = initializer;
    }

    getWhisperSessions(): WhisperSession[] {
        return [];
    }

    dropWhisperSession(session: WhisperSession) {}

    async startWhisper(target: WhisperTarget): Promise<void> {}

    getWhisperTarget(): WhisperTarget | undefined {
        return undefined;
    }

    stopWhisper() {}

    async getConnectionStats(): Promise<ConnectionStatistics> {
        return {
            bytesReceived: this.transport?.getControlStatistics().bytesReceived ?? 0,
            bytesSend: this.transport?.getControlStatistics().bytesSend ?? 0
        };
    }

    getRetryTimestamp(): number | 0 {
        return 0;
    }
}
