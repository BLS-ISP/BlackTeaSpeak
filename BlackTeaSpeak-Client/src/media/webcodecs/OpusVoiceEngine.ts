import { SpatialPosition } from '../../types/protocol';

interface PeerAudioPipeline {
  decoder: AudioDecoder;
  gainNode: GainNode;
  pannerNode: PannerNode;
  nextPlayTime: number;
  timestampUs: number;
}

export interface VoiceTelemetryCallback {
  (stats: {
    inputLevelDb: number;
    vadActive: boolean;
    voicePacketsSent: number;
    voicePacketsReceived: number;
  }): void;
}

export class OpusVoiceEngine {
  private audioCtx: AudioContext | null = null;
  private micStream: MediaStream | null = null;
  private encoder: AudioEncoder | null = null;
  private processorNode: ScriptProcessorNode | null = null;
  private peers = new Map<number, PeerAudioPipeline>();
  private peerVolumes = new Map<number, number>();
  private peerMuted = new Map<number, boolean>();
  private peerPositions = new Map<number, SpatialPosition>();

  private inputMuted = false;
  private outputDeafened = false;
  private spatial3DEnabled = true;
  private mode: 'vad' | 'ptt' = 'vad';
  private vadThresholdDb = -42;
  private pttPressed = false;
  private isWhisperMode = false;
  private lastVoiceActiveAt = 0;
  private wasTransmitting = false;

  private echoCancellation = true;
  private noiseSuppression = true;
  private autoGainControl = true;
  private preferredInputDevice = 'Default';
  private preferredOutputDevice = 'Default';

  private voicePacketsSent = 0;
  private voicePacketsReceived = 0;
  private encodeTimestampUs = 0;

  private sendDatagramFn: ((packetType: number, payload: Uint8Array) => void) | null = null;
  private onTelemetry: VoiceTelemetryCallback | null = null;

  public configure(options: {
    sendDatagram: (packetType: number, payload: Uint8Array) => void;
    onTelemetry?: VoiceTelemetryCallback;
  }): void {
    this.sendDatagramFn = options.sendDatagram;
    this.onTelemetry = options.onTelemetry ?? null;
  }

  public async setAudioProcessingConfig(config: {
    echoCancellation?: boolean | null;
    noiseSuppression?: boolean | null;
    autoGainControl?: boolean | null;
    inputDevice?: string | null;
    outputDevice?: string | null;
  }): Promise<void> {
    const prevInput = this.preferredInputDevice;
    if (config.echoCancellation !== undefined && config.echoCancellation !== null) {
      this.echoCancellation = config.echoCancellation;
    }
    if (config.noiseSuppression !== undefined && config.noiseSuppression !== null) {
      this.noiseSuppression = config.noiseSuppression;
    }
    if (config.autoGainControl !== undefined && config.autoGainControl !== null) {
      this.autoGainControl = config.autoGainControl;
    }
    if (config.inputDevice !== undefined && config.inputDevice !== null) {
      this.preferredInputDevice = config.inputDevice;
    }
    if (config.outputDevice !== undefined && config.outputDevice !== null) {
      this.preferredOutputDevice = config.outputDevice;
      await this.applyOutputSinkDevice(config.outputDevice);
    }

    if (this.micStream) {
      if (
        config.inputDevice !== undefined &&
        config.inputDevice !== null &&
        config.inputDevice !== prevInput
      ) {
        await this.startMicrophone(config.inputDevice).catch(() => {});
        return;
      }
      for (const track of this.micStream.getAudioTracks()) {
        if (typeof track.applyConstraints === 'function') {
          await track
            .applyConstraints({
              echoCancellation: this.echoCancellation,
              noiseSuppression: this.noiseSuppression,
              autoGainControl: this.autoGainControl,
            })
            .catch(() => {});
        }
      }
    }
  }

  private async resolveBrowserAudioDeviceId(
    kind: 'audioinput' | 'audiooutput',
    deviceIdOrLabel?: string
  ): Promise<string | undefined> {
    if (!deviceIdOrLabel || deviceIdOrLabel === 'Default') return undefined;
    if (!navigator.mediaDevices?.enumerateDevices) return undefined;
    try {
      const devices = await navigator.mediaDevices.enumerateDevices();
      const filtered = devices.filter((d) => d.kind === kind);
      const exact = filtered.find(
        (d) =>
          d.deviceId === deviceIdOrLabel ||
          d.label.toLowerCase() === deviceIdOrLabel.toLowerCase() ||
          d.label.toLowerCase().includes(deviceIdOrLabel.toLowerCase())
      );
      return exact?.deviceId;
    } catch {
      return undefined;
    }
  }

  private async applyOutputSinkDevice(deviceIdOrLabel?: string): Promise<void> {
    if (!this.audioCtx) return;
    const ctxWithSink = this.audioCtx as AudioContext & {
      setSinkId?: (sinkId: string) => Promise<void>;
    };
    if (typeof ctxWithSink.setSinkId !== 'function') return;
    try {
      const resolvedId = await this.resolveBrowserAudioDeviceId(
        'audiooutput',
        deviceIdOrLabel
      );
      await ctxWithSink.setSinkId(resolvedId ?? '');
    } catch {
      // Ignore if browser does not permit sink switching
    }
  }

  public async startMicrophone(deviceId?: string): Promise<void> {
    this.stopMicrophone();

    const AudioCtx = window.AudioContext || (window as unknown as { webkitAudioContext: typeof AudioContext }).webkitAudioContext;
    if (!this.audioCtx || this.audioCtx.state === 'closed') {
      this.audioCtx = new AudioCtx({ sampleRate: 48000, latencyHint: 'interactive' });
    }
    if (this.audioCtx.state === 'suspended') {
      await this.audioCtx.resume();
    }
    await this.applyOutputSinkDevice(this.preferredOutputDevice);

    const resolvedInputId = await this.resolveBrowserAudioDeviceId(
      'audioinput',
      deviceId ?? this.preferredInputDevice
    );

    this.micStream = await navigator.mediaDevices.getUserMedia({
      audio: {
        ...(resolvedInputId ? { deviceId: { ideal: resolvedInputId } } : {}),
        sampleRate: 48000,
        channelCount: 2,
        echoCancellation: this.echoCancellation,
        noiseSuppression: this.noiseSuppression,
        autoGainControl: this.autoGainControl,
      },
      video: false,
    });

    if (typeof AudioEncoder !== 'undefined') {
      this.encoder = new AudioEncoder({
        output: (chunk: EncodedAudioChunk) => {
          const buf = new Uint8Array(chunk.byteLength);
          chunk.copyTo(buf);
          const pktType = this.isWhisperMode ? 3 : 1;
          this.sendDatagramFn?.(pktType, buf);
          this.voicePacketsSent++;
        },
        error: (err) => {
          console.warn('WebCodecs AudioEncoder error:', err);
        },
      });

      this.encoder.configure({
        codec: 'opus',
        sampleRate: 48000,
        numberOfChannels: 2,
        bitrate: 64000,
      });
    }

    const source = this.audioCtx.createMediaStreamSource(this.micStream);
    // 1024 samples @ 48kHz (~21.3ms frames)
    const processor = this.audioCtx.createScriptProcessor(1024, 2, 2);
    this.processorNode = processor;

    processor.onaudioprocess = (event: AudioProcessingEvent) => {
      const inputBuffer = event.inputBuffer;
      const ch0 = inputBuffer.getChannelData(0);
      const ch1 = inputBuffer.numberOfChannels > 1 ? inputBuffer.getChannelData(1) : ch0;

      // Compute RMS level in dBFS
      let sumSq = 0;
      for (let i = 0; i < ch0.length; i++) {
        sumSq += ch0[i] * ch0[i];
      }
      const rms = Math.sqrt(sumSq / ch0.length);
      const db = Math.max(-90, 20 * Math.log10(rms + 1e-6));

      const now = performance.now();
      let shouldTransmit = false;

      if (!this.inputMuted && !this.outputDeafened) {
        if (this.mode === 'ptt') {
          shouldTransmit = this.pttPressed;
        } else {
          if (db >= this.vadThresholdDb) {
            this.lastVoiceActiveAt = now;
            shouldTransmit = true;
          } else if (now - this.lastVoiceActiveAt < 240) {
            // 240ms VAD hold time for smooth sentence endings
            shouldTransmit = true;
          }
        }
      }

      if (shouldTransmit) {
        this.wasTransmitting = true;
        if (this.encoder && this.encoder.state === 'configured' && typeof AudioData !== 'undefined') {
          const planar = new Float32Array(ch0.length * 2);
          planar.set(ch0, 0);
          planar.set(ch1, ch0.length);

          const audioData = new AudioData({
            format: 'f32-planar',
            sampleRate: inputBuffer.sampleRate,
            numberOfFrames: ch0.length,
            numberOfChannels: 2,
            timestamp: this.encodeTimestampUs,
            data: planar,
          });
          this.encodeTimestampUs += Math.round((ch0.length / inputBuffer.sampleRate) * 1_000_000);
          this.encoder.encode(audioData);
          audioData.close();
        }
      } else if (this.wasTransmitting) {
        this.wasTransmitting = false;
        // Send empty stop-talking datagram so server clears TalkStatus
        this.sendDatagramFn?.(1, new Uint8Array(0));
      }

      this.onTelemetry?.({
        inputLevelDb: Math.round(db),
        vadActive: shouldTransmit,
        voicePacketsSent: this.voicePacketsSent,
        voicePacketsReceived: this.voicePacketsReceived,
      });
    };

    source.connect(processor);
    processor.connect(this.audioCtx.destination);
  }

  public stopMicrophone(): void {
    if (this.wasTransmitting) {
      this.wasTransmitting = false;
      this.sendDatagramFn?.(1, new Uint8Array(0));
    }
    if (this.processorNode) {
      this.processorNode.disconnect();
      this.processorNode.onaudioprocess = null;
      this.processorNode = null;
    }
    if (this.encoder) {
      try {
        if (this.encoder.state !== 'closed') this.encoder.close();
      } catch {
        // Ignore
      }
      this.encoder = null;
    }
    if (this.micStream) {
      for (const track of this.micStream.getTracks()) {
        track.stop();
      }
      this.micStream = null;
    }
  }

  public handleIncomingOpusPacket(senderClientId: number, payload: Uint8Array): void {
    if (this.outputDeafened || payload.byteLength === 0) return;
    if (this.peerMuted.get(senderClientId)) return;

    this.voicePacketsReceived++;
    const pipeline = this.getOrCreatePeerPipeline(senderClientId);
    if (!pipeline || pipeline.decoder.state !== 'configured') return;

    try {
      const chunk = new EncodedAudioChunk({
        type: 'key',
        timestamp: pipeline.timestampUs,
        data: payload,
      });
      pipeline.timestampUs += 20_000;
      pipeline.decoder.decode(chunk);
    } catch {
      // Ignore malformed Opus frame
    }
  }

  public setInputMuted(muted: boolean): void {
    this.inputMuted = muted;
    if (muted && this.wasTransmitting) {
      this.wasTransmitting = false;
      this.sendDatagramFn?.(1, new Uint8Array(0));
    }
  }

  public setOutputDeafened(deafened: boolean): void {
    this.outputDeafened = deafened;
  }

  public setTransmissionMode(mode: 'vad' | 'ptt', vadThresholdDb = -42): void {
    this.mode = mode;
    this.vadThresholdDb = vadThresholdDb;
  }

  public setPttPressed(pressed: boolean): void {
    this.pttPressed = pressed;
  }

  public setWhisperMode(whisper: boolean): void {
    this.isWhisperMode = whisper;
  }

  public setPeerVolume(clientId: number, gain: number): void {
    this.peerVolumes.set(clientId, gain);
    const peer = this.peers.get(clientId);
    if (peer) {
      peer.gainNode.gain.value = this.peerMuted.get(clientId) ? 0 : gain;
    }
  }

  public setPeerMuted(clientId: number, muted: boolean): void {
    this.peerMuted.set(clientId, muted);
    const peer = this.peers.get(clientId);
    if (peer) {
      peer.gainNode.gain.value = muted ? 0 : (this.peerVolumes.get(clientId) ?? 1.0);
    }
  }

  public setSpatial3DEnabled(enabled: boolean): void {
    this.spatial3DEnabled = enabled;
    for (const [clientId, peer] of this.peers) {
      const pos = this.peerPositions.get(clientId) ?? { x: 0, y: 0, z: 0 };
      this.applyPannerCoords(peer.pannerNode, enabled ? pos : { x: 0, y: 0, z: 0 });
    }
  }

  public updatePeerSpatialPosition(clientId: number, pos: SpatialPosition): void {
    this.peerPositions.set(clientId, pos);
    const peer = this.peers.get(clientId);
    if (peer && this.spatial3DEnabled) {
      this.applyPannerCoords(peer.pannerNode, pos);
    }
  }

  public broadcastSelfSpatialPosition(pos: SpatialPosition): void {
    const buf = new ArrayBuffer(12);
    const view = new DataView(buf);
    view.setFloat32(0, pos.x, true);
    view.setFloat32(4, pos.y, true);
    view.setFloat32(8, pos.z, true);
    this.sendDatagramFn?.(5, new Uint8Array(buf));
  }

  public dispose(): void {
    this.stopMicrophone();
    for (const [, peer] of this.peers) {
      try {
        if (peer.decoder.state !== 'closed') peer.decoder.close();
      } catch {
        // Ignore
      }
      peer.gainNode.disconnect();
      peer.pannerNode.disconnect();
    }
    this.peers.clear();
  }

  private getOrCreatePeerPipeline(clientId: number): PeerAudioPipeline | null {
    const existing = this.peers.get(clientId);
    if (existing) return existing;

    if (typeof AudioDecoder === 'undefined') return null;
    if (!this.audioCtx || this.audioCtx.state === 'closed') {
      const AudioCtx = window.AudioContext || (window as unknown as { webkitAudioContext: typeof AudioContext }).webkitAudioContext;
      this.audioCtx = new AudioCtx({ sampleRate: 48000, latencyHint: 'interactive' });
    }
    const ctx = this.audioCtx;

    const gainNode = ctx.createGain();
    gainNode.gain.value = this.peerMuted.get(clientId)
      ? 0
      : (this.peerVolumes.get(clientId) ?? 1.0);

    const pannerNode = ctx.createPanner();
    pannerNode.panningModel = 'HRTF';
    pannerNode.distanceModel = 'inverse';
    pannerNode.refDistance = 1;
    pannerNode.maxDistance = 50;
    pannerNode.rolloffFactor = 1.2;

    const initialPos = this.peerPositions.get(clientId) ?? { x: 0, y: 0, z: -1 };
    this.applyPannerCoords(
      pannerNode,
      this.spatial3DEnabled ? initialPos : { x: 0, y: 0, z: 0 }
    );

    gainNode.connect(pannerNode);
    pannerNode.connect(ctx.destination);

    const pipeline: PeerAudioPipeline = {
      decoder: null as unknown as AudioDecoder,
      gainNode,
      pannerNode,
      nextPlayTime: ctx.currentTime,
      timestampUs: 0,
    };

    const decoder = new AudioDecoder({
      output: (audioData: AudioData) => {
        try {
          const numFrames = audioData.numberOfFrames;
          const numChannels = audioData.numberOfChannels;
          const sampleRate = audioData.sampleRate;
          const audioBuffer = ctx.createBuffer(numChannels, numFrames, sampleRate);

          for (let ch = 0; ch < numChannels; ch++) {
            const channelFloats = new Float32Array(numFrames);
            audioData.copyTo(channelFloats, {
              planeIndex: ch,
              format: 'f32-planar',
            });
            audioBuffer.copyToChannel(channelFloats, ch);
          }

          const src = ctx.createBufferSource();
          src.buffer = audioBuffer;
          src.connect(pipeline.gainNode);

          const now = ctx.currentTime;
          if (pipeline.nextPlayTime < now || pipeline.nextPlayTime > now + 0.18) {
            pipeline.nextPlayTime = now + 0.015; // 15ms jitter cushion
          }
          src.start(pipeline.nextPlayTime);
          pipeline.nextPlayTime += audioBuffer.duration;
        } finally {
          audioData.close();
        }
      },
      error: (err) => {
        console.warn(`WebCodecs AudioDecoder error for peer ${clientId}:`, err);
      },
    });

    decoder.configure({
      codec: 'opus',
      sampleRate: 48000,
      numberOfChannels: 2,
    });

    pipeline.decoder = decoder;
    this.peers.set(clientId, pipeline);
    return pipeline;
  }

  private applyPannerCoords(panner: PannerNode, pos: SpatialPosition): void {
    if (panner.positionX) {
      panner.positionX.value = pos.x;
      panner.positionY.value = pos.y;
      panner.positionZ.value = pos.z;
    } else {
      panner.setPosition(pos.x, pos.y, pos.z);
    }
  }
}
