import { ActiveVideoStream } from '../../types/protocol';

interface PeerVideoDecoderPipeline {
  decoder: VideoDecoder;
  codec: string;
  width: number;
  height: number;
  framesDecoded: number;
  bytesReceived: number;
  hasReceivedKeyFrame: boolean;
  lastDecodedTimestampUs: number;
  pendingDeltaChunks: Array<{ timestampUs: number; data: Uint8Array }>;
  backingCanvas: HTMLCanvasElement;
  hasRenderedFrame: boolean;
}

export class VideoCodecsEngine {
  private cameraStream: MediaStream | null = null;
  private screenStream: MediaStream | null = null;
  private cameraEncoder: VideoEncoder | null = null;
  private screenEncoder: VideoEncoder | null = null;
  private cameraTimer: number | null = null;
  private screenTimer: number | null = null;

  private peerDecoders = new Map<string, PeerVideoDecoderPipeline>();
  private canvasTargets = new Map<string, HTMLCanvasElement>();
  private localPreviewCanvases = new Map<'camera' | 'screen', HTMLCanvasElement>();

  private videoFramesSent = 0;
  private videoFramesReceived = 0;

  private sendDatagramFn: ((packetType: number, payload: Uint8Array) => void) | null = null;
  private sendUniFn: ((packetType: number, payload: Uint8Array) => void) | null = null;
  private onStreamUpdate: ((streamKey: string, info: Omit<ActiveVideoStream, 'senderName'>) => void) | null = null;

  public configure(options: {
    sendDatagram: (packetType: number, payload: Uint8Array) => void;
    sendUnidirectional: (packetType: number, payload: Uint8Array) => void;
    onStreamUpdate?: (streamKey: string, info: Omit<ActiveVideoStream, 'senderName'>) => void;
  }): void {
    this.sendDatagramFn = options.sendDatagram;
    this.sendUniFn = options.sendUnidirectional;
    this.onStreamUpdate = options.onStreamUpdate ?? null;
  }

  public registerCanvas(streamKey: string, canvas: HTMLCanvasElement | null): void {
    if (canvas) {
      this.canvasTargets.set(streamKey, canvas);
      const pipeline = this.peerDecoders.get(streamKey);
      if (
        pipeline &&
        pipeline.hasRenderedFrame &&
        pipeline.backingCanvas.width > 0 &&
        pipeline.backingCanvas.height > 0
      ) {
        if (
          canvas.width !== pipeline.backingCanvas.width ||
          canvas.height !== pipeline.backingCanvas.height
        ) {
          canvas.width = pipeline.backingCanvas.width;
          canvas.height = pipeline.backingCanvas.height;
        }
        const ctx = canvas.getContext('2d');
        ctx?.drawImage(pipeline.backingCanvas, 0, 0, canvas.width, canvas.height);
      }
    } else {
      this.canvasTargets.delete(streamKey);
    }
  }

  public registerLocalPreviewCanvas(
    kind: 'camera' | 'screen',
    canvas: HTMLCanvasElement | null
  ): void {
    if (canvas) {
      this.localPreviewCanvases.set(kind, canvas);
    } else {
      this.localPreviewCanvases.delete(kind);
    }
  }

  public async startCameraShare(width = 1280, height = 720, fps = 30): Promise<void> {
    this.stopCameraShare();
    this.cameraStream = await navigator.mediaDevices.getUserMedia({
      video: { width: { ideal: width }, height: { ideal: height }, frameRate: { ideal: fps } },
      audio: false,
    });
    await this.startCaptureLoop('camera', this.cameraStream, width, height, fps);
  }

  public stopCameraShare(): void {
    if (this.cameraTimer !== null) {
      clearInterval(this.cameraTimer);
      this.cameraTimer = null;
    }
    if (this.cameraEncoder) {
      try {
        if (this.cameraEncoder.state !== 'closed') this.cameraEncoder.close();
      } catch {
        // Ignore
      }
      this.cameraEncoder = null;
    }
    if (this.cameraStream) {
      for (const track of this.cameraStream.getTracks()) track.stop();
      this.cameraStream = null;
    }
  }

  public async startScreenShare(
    width = 1920,
    height = 1080,
    fps = 30,
    onEnded?: () => void
  ): Promise<void> {
    this.stopScreenShare();
    this.screenStream = await navigator.mediaDevices.getDisplayMedia({
      video: { width: { ideal: width }, height: { ideal: height }, frameRate: { ideal: fps } },
      audio: false,
    });
    const videoTrack = this.screenStream.getVideoTracks()[0];
    if (videoTrack && onEnded) {
      videoTrack.onended = () => {
        this.stopScreenShare();
        onEnded();
      };
    }
    await this.startCaptureLoop('screen', this.screenStream, width, height, fps);
  }

  public stopScreenShare(): void {
    if (this.screenTimer !== null) {
      clearInterval(this.screenTimer);
      this.screenTimer = null;
    }
    if (this.screenEncoder) {
      try {
        if (this.screenEncoder.state !== 'closed') this.screenEncoder.close();
      } catch {
        // Ignore
      }
      this.screenEncoder = null;
    }
    if (this.screenStream) {
      for (const track of this.screenStream.getTracks()) track.stop();
      this.screenStream = null;
    }
  }

  public handleIncomingVideoPacket(
    packetType: number,
    senderClientId: number,
    payload: Uint8Array
  ): void {
    if (payload.byteLength <= 13 || typeof VideoDecoder === 'undefined') return;

    const streamKind: 'camera' | 'screen' = packetType === 4 ? 'screen' : 'camera';
    const streamKey = `${senderClientId}:${streamKind}`;

    const view = new DataView(payload.buffer, payload.byteOffset, payload.byteLength);
    const flags = view.getUint8(0);
    const isKeyFrame = (flags & 0x01) !== 0;
    const codecId = (flags >> 1) & 0x03;
    const width = view.getUint16(1, true);
    const height = view.getUint16(3, true);
    const timestampUs = view.getFloat64(5, true);
    const chunkBytes = payload.subarray(13);

    const codecString = codecId === 1 ? 'vp09.00.10.08' : 'vp8';
    const pipeline = this.getOrCreateDecoderPipeline(streamKey, codecString, width, height);
    if (!pipeline) return;

    if (!pipeline.hasReceivedKeyFrame && !isKeyFrame) {
      // A small delta frame on a separate QUIC stream can finish downloading 1-2ms before
      // a large keyframe. Buffer it briefly so we can decode it right after the keyframe!
      if (pipeline.pendingDeltaChunks.length < 15) {
        const copy = new Uint8Array(chunkBytes.byteLength);
        copy.set(chunkBytes);
        pipeline.pendingDeltaChunks.push({ timestampUs, data: copy });
      }
      return;
    }

    if (
      !isKeyFrame &&
      pipeline.lastDecodedTimestampUs > 0 &&
      timestampUs <= pipeline.lastDecodedTimestampUs
    ) {
      return;
    }

    pipeline.bytesReceived += chunkBytes.byteLength;
    pipeline.framesDecoded++;
    this.videoFramesReceived++;

    try {
      const chunk = new EncodedVideoChunk({
        type: isKeyFrame ? 'key' : 'delta',
        timestamp: Math.round(timestampUs),
        data: chunkBytes,
      });
      pipeline.decoder.decode(chunk);
      pipeline.hasReceivedKeyFrame = true;
      pipeline.lastDecodedTimestampUs = timestampUs;

      if (isKeyFrame && pipeline.pendingDeltaChunks.length > 0) {
        const buffered = pipeline.pendingDeltaChunks.splice(0);
        buffered.sort((a, b) => a.timestampUs - b.timestampUs);
        for (const item of buffered) {
          if (item.timestampUs > pipeline.lastDecodedTimestampUs) {
            const deltaChunk = new EncodedVideoChunk({
              type: 'delta',
              timestamp: Math.round(item.timestampUs),
              data: item.data,
            });
            pipeline.decoder.decode(deltaChunk);
            pipeline.lastDecodedTimestampUs = item.timestampUs;
            pipeline.bytesReceived += item.data.byteLength;
            pipeline.framesDecoded++;
            this.videoFramesReceived++;
          }
        }
      }
    } catch {
      pipeline.hasReceivedKeyFrame = false;
      pipeline.pendingDeltaChunks.length = 0;
    }

    this.onStreamUpdate?.(streamKey, {
      senderClientId,
      streamKind,
      width,
      height,
      fps: 30,
      codec: codecId === 1 ? 'VP9 (WebCodecs)' : 'VP8 (WebCodecs)',
      lastFrameAt: Date.now(),
      framesDecoded: pipeline.framesDecoded,
      bytesReceived: pipeline.bytesReceived,
    });
  }

  public getStats(): { videoFramesSent: number; videoFramesReceived: number } {
    return {
      videoFramesSent: this.videoFramesSent,
      videoFramesReceived: this.videoFramesReceived,
    };
  }

  public dispose(): void {
    this.stopCameraShare();
    this.stopScreenShare();
    for (const [, p] of this.peerDecoders) {
      try {
        if (p.decoder.state !== 'closed') p.decoder.close();
      } catch {
        // Ignore
      }
    }
    this.peerDecoders.clear();
  }

  private async startCaptureLoop(
    kind: 'camera' | 'screen',
    stream: MediaStream,
    targetWidth: number,
    targetHeight: number,
    fps: number
  ): Promise<void> {
    const videoEl = document.createElement('video');
    videoEl.srcObject = stream;
    videoEl.muted = true;
    videoEl.playsInline = true;
    await videoEl.play();

    const actualWidth = videoEl.videoWidth || targetWidth;
    const actualHeight = videoEl.videoHeight || targetHeight;
    const packetType = kind === 'screen' ? 4 : 2;

    const useVp9 = false;
    if (typeof VideoEncoder !== 'undefined') {
      const encoder = new VideoEncoder({
        output: (chunk: EncodedVideoChunk) => {
          const isKey = chunk.type === 'key';
          const flags = (isKey ? 1 : 0) | ((useVp9 ? 1 : 0) << 1);
          const packet = new Uint8Array(13 + chunk.byteLength);
          const view = new DataView(packet.buffer);
          view.setUint8(0, flags);
          view.setUint16(1, actualWidth, true);
          view.setUint16(3, actualHeight, true);
          view.setFloat64(5, chunk.timestamp, true);
          chunk.copyTo(packet.subarray(13));

          this.videoFramesSent++;
          // Always send video frames over Unidirectional QUIC Streams so small P-frames
          // never overtake in-flight keyframes via QUIC Datagrams!
          if (this.sendUniFn) {
            this.sendUniFn(packetType, packet);
          } else {
            this.sendDatagramFn?.(packetType, packet);
          }
        },
        error: (err) => {
          console.warn(`WebCodecs VideoEncoder (${kind}) error:`, err);
        },
      });

      encoder.configure({
        codec: 'vp8',
        width: actualWidth,
        height: actualHeight,
        bitrate: kind === 'screen' ? 2_200_000 : 1_200_000,
        framerate: fps,
        latencyMode: 'realtime',
      });

      if (kind === 'screen') {
        this.screenEncoder = encoder;
      } else {
        this.cameraEncoder = encoder;
      }
    }

    let frameCounter = 0;
    const intervalMs = Math.max(16, Math.round(1000 / fps));
    const timer = window.setInterval(() => {
      if (videoEl.readyState < 2) return;

      // Paint local preview if canvas is registered for this specific stream kind ('camera' | 'screen')
      const previewCanvas = this.localPreviewCanvases.get(kind);
      if (previewCanvas) {
        const ctx = previewCanvas.getContext('2d');
        if (ctx) {
          if (
            previewCanvas.width !== actualWidth ||
            previewCanvas.height !== actualHeight
          ) {
            previewCanvas.width = actualWidth;
            previewCanvas.height = actualHeight;
          }
          ctx.drawImage(videoEl, 0, 0, actualWidth, actualHeight);
        }
      }

      const encoder = kind === 'screen' ? this.screenEncoder : this.cameraEncoder;
      if (encoder && encoder.state === 'configured' && typeof VideoFrame !== 'undefined') {
        try {
          const frame = new VideoFrame(videoEl, {
            timestamp: Math.round(performance.now() * 1000),
          });
          const keyFrame = frameCounter % 30 === 0;
          frameCounter++;
          encoder.encode(frame, { keyFrame });
          frame.close();
        } catch {
          // Ignore transient frame capture error
        }
      }
    }, intervalMs);

    if (kind === 'screen') {
      this.screenTimer = timer;
    } else {
      this.cameraTimer = timer;
    }
  }

  public removeStream(streamKey: string): void {
    const existing = this.peerDecoders.get(streamKey);
    if (existing) {
      try {
        if (existing.decoder.state !== 'closed') existing.decoder.close();
      } catch {
        // Ignore
      }
      this.peerDecoders.delete(streamKey);
    }
  }

  private getOrCreateDecoderPipeline(
    streamKey: string,
    codec: string,
    width: number,
    height: number
  ): PeerVideoDecoderPipeline | null {
    const existing = this.peerDecoders.get(streamKey);
    if (
      existing &&
      existing.codec === codec &&
      existing.width === width &&
      existing.height === height &&
      existing.decoder.state === 'configured'
    ) {
      return existing;
    }

    if (existing) {
      try {
        if (existing.decoder.state !== 'closed') existing.decoder.close();
      } catch {
        // Ignore
      }
    }

    const backingCanvas = existing?.backingCanvas ?? document.createElement('canvas');
    const decoder = new VideoDecoder({
      output: (videoFrame: VideoFrame) => {
        try {
          const fw = videoFrame.displayWidth || width;
          const fh = videoFrame.displayHeight || height;
          if (backingCanvas.width !== fw || backingCanvas.height !== fh) {
            backingCanvas.width = fw;
            backingCanvas.height = fh;
          }
          const bctx = backingCanvas.getContext('2d');
          bctx?.drawImage(videoFrame, 0, 0, fw, fh);
          pipeline.hasRenderedFrame = true;

          const canvas = this.canvasTargets.get(streamKey);
          if (canvas) {
            if (canvas.width !== fw || canvas.height !== fh) {
              canvas.width = fw;
              canvas.height = fh;
            }
            const ctx = canvas.getContext('2d');
            ctx?.drawImage(videoFrame, 0, 0, canvas.width, canvas.height);
          }
        } finally {
          videoFrame.close();
        }
      },
      error: (err) => {
        console.warn(`WebCodecs VideoDecoder (${streamKey}) error:`, err);
      },
    });

    decoder.configure({
      codec,
      optimizeForLatency: true,
    });

    const pipeline: PeerVideoDecoderPipeline = {
      decoder,
      codec,
      width,
      height,
      framesDecoded: existing?.framesDecoded ?? 0,
      bytesReceived: existing?.bytesReceived ?? 0,
      hasReceivedKeyFrame: false,
      lastDecodedTimestampUs: 0,
      pendingDeltaChunks: [],
      backingCanvas,
      hasRenderedFrame: existing?.hasRenderedFrame ?? false,
    };

    this.peerDecoders.set(streamKey, pipeline);
    return pipeline;
  }
}
