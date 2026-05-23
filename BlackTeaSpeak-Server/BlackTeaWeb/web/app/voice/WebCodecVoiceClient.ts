import {VoiceClient} from "tc-shared/voice/VoiceClient";
import {VoicePlayerEvents, VoicePlayerLatencySettings, VoicePlayerState} from "tc-shared/voice/VoicePlayer";
import {Registry} from "tc-shared/events";
import {getAudioBackend} from "tc-shared/audio/Player";
import {LogCategory, logError} from "tc-shared/log";

export class WebCodecVoiceClient implements VoiceClient {
    readonly events: Registry<VoicePlayerEvents>;
    private readonly clientId: number;
    
    private globallyMuted: boolean = false;
    private volume: number = 1;
    private currentState: VoicePlayerState = VoicePlayerState.STOPPED;
    
    private decoder?: any; // AudioDecoder
    private trackGenerator?: any; // MediaStreamTrackGenerator
    private audioContextNode?: MediaStreamAudioSourceNode;
    private gainNode?: GainNode;

    constructor(clientId: number) {
        this.clientId = clientId;
        this.events = new Registry<VoicePlayerEvents>();

        this.initAudioPipeline();
    }

    private initAudioPipeline() {
        if(typeof (window as any).AudioDecoder === 'undefined' || typeof (window as any).MediaStreamTrackGenerator === 'undefined') {
            logError(LogCategory.AUDIO, "WebCodecs API not supported by this browser.");
            return;
        }

        this.trackGenerator = new (window as any).MediaStreamTrackGenerator({ kind: 'audio' });
        const trackWriter = this.trackGenerator.writable.getWriter();

        this.decoder = new (window as any).AudioDecoder({
            output: (data: any) => {
                // data is AudioData
                trackWriter.write(data).catch(e => logError(LogCategory.AUDIO, "Error writing AudioData", e));
                this.setState(VoicePlayerState.PLAYING);
            },
            error: (e: Error) => {
                logError(LogCategory.AUDIO, "AudioDecoder error", e);
            }
        });

        this.decoder.configure({
            codec: 'opus',
            sampleRate: 48000,
            numberOfChannels: 1
        });

        const ctx = getAudioBackend().getAudioContext();
        const stream = new MediaStream([this.trackGenerator]);
        this.audioContextNode = ctx.createMediaStreamSource(stream);
        this.gainNode = ctx.createGain();
        this.audioContextNode.connect(this.gainNode);
        this.gainNode.connect(ctx.destination);
        this.updateVolume();
    }

    public feedOpusChunk(payload: Uint8Array) {
        if(this.decoder && this.decoder.state === 'configured') {
            const chunk = new (window as any).EncodedAudioChunk({
                type: 'key',
                timestamp: performance.now() * 1000,
                duration: 20000, // approx 20ms
                data: payload
            });
            this.decoder.decode(chunk);
        }
    }

    getClientId(): number {
        return this.clientId;
    }

    destroy() {
        this.events.destroy();
        if(this.decoder && this.decoder.state !== 'closed') {
            this.decoder.close();
        }
        if(this.audioContextNode) {
            this.audioContextNode.disconnect();
        }
        if(this.gainNode) {
            this.gainNode.disconnect();
        }
        if(this.trackGenerator) {
            this.trackGenerator.stop();
        }
    }

    setGloballyMuted(muted: boolean) {
        this.globallyMuted = muted;
        this.updateVolume();
    }

    abortReplay() {
        this.setState(VoicePlayerState.STOPPED);
    }

    getState(): VoicePlayerState {
        return this.currentState;
    }

    protected setState(state: VoicePlayerState) {
        if(this.currentState === state) return;
        const oldState = this.currentState;
        this.currentState = state;
        this.events.fire("notify_state_changed", { oldState, newState: state });
    }

    getVolume(): number {
        return this.volume;
    }

    setVolume(volume: number) {
        this.volume = volume;
        this.updateVolume();
    }

    private updateVolume() {
        if(this.gainNode) {
            this.gainNode.gain.value = this.globallyMuted ? 0 : this.volume;
        }
    }

    flushBuffer() {}
    getLatencySettings(): Readonly<VoicePlayerLatencySettings> {
        return { minBufferTime: 0, maxBufferTime: 0 };
    }
    resetLatencySettings() {}
    setLatencySettings(_settings: VoicePlayerLatencySettings) {}
}
