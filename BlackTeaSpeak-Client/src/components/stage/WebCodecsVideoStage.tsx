import React, { useEffect, useRef } from 'react';
import { Video, Monitor, Maximize2, XCircle } from 'lucide-react';
import { blackTeaStore, useBlackTeaStore } from '../../store/useBlackTeaStore';
import { ActiveVideoStream } from '../../types/protocol';

const RemoteStreamTile: React.FC<{ streamKey: string; stream: ActiveVideoStream }> = ({
  streamKey,
  stream,
}) => {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);

  useEffect(() => {
    blackTeaStore.videoEngine.registerCanvas(streamKey, canvasRef.current);
    return () => {
      blackTeaStore.videoEngine.registerCanvas(streamKey, null);
    };
  }, [streamKey]);

  const handleFullscreen = () => {
    if (canvasRef.current?.requestFullscreen) {
      canvasRef.current.requestFullscreen();
    }
  };

  return (
    <div className="video-stage-tile">
      <canvas ref={canvasRef} className="video-stage-canvas" />
      <div className="video-tile-overlay">
        <div className="video-tile-meta">
          {stream.streamKind === 'screen' ? <Monitor size={13} /> : <Video size={13} />}
          <strong>{stream.senderName}</strong>
          <span className="codec-pill">
            {stream.width}x{stream.height} • {stream.codec}
          </span>
        </div>
        <button className="btn-icon-xs" onClick={handleFullscreen} title="Vollbild">
          <Maximize2 size={13} />
        </button>
      </div>
    </div>
  );
};

export const WebCodecsVideoStage: React.FC = () => {
  const state = useBlackTeaStore();
  const localCameraCanvasRef = useRef<HTMLCanvasElement | null>(null);
  const localScreenCanvasRef = useRef<HTMLCanvasElement | null>(null);
  const remoteEntries = Object.entries(state.activeVideoStreams);
  const hasLocalStream = state.cameraSharing || state.screenSharing;

  useEffect(() => {
    if (state.cameraSharing && localCameraCanvasRef.current) {
      blackTeaStore.videoEngine.registerLocalPreviewCanvas('camera', localCameraCanvasRef.current);
    } else {
      blackTeaStore.videoEngine.registerLocalPreviewCanvas('camera', null);
    }
  }, [state.cameraSharing]);

  useEffect(() => {
    if (state.screenSharing && localScreenCanvasRef.current) {
      blackTeaStore.videoEngine.registerLocalPreviewCanvas('screen', localScreenCanvasRef.current);
    } else {
      blackTeaStore.videoEngine.registerLocalPreviewCanvas('screen', null);
    }
  }, [state.screenSharing]);

  if (!hasLocalStream && remoteEntries.length === 0) {
    return null;
  }

  return (
    <section className="webcodecs-video-stage">
      <div className="stage-header">
        <div className="stage-title">
          <Monitor size={15} />
          <span>WebCodecs Live-Stage (HTTP/3 QUIC Stream &amp; Datagram Transport)</span>
        </div>
        {hasLocalStream && (
          <div className="stage-controls">
            {state.cameraSharing && (
              <button
                className="btn-danger-xs"
                onClick={() => blackTeaStore.toggleCameraShare()}
              >
                <XCircle size={12} /> Kamera beenden
              </button>
            )}
            {state.screenSharing && (
              <button
                className="btn-danger-xs"
                onClick={() => blackTeaStore.toggleScreenShare()}
              >
                <XCircle size={12} /> Screen-Share beenden
              </button>
            )}
          </div>
        )}
      </div>

      <div className="stage-grid">
        {state.cameraSharing && (
          <div className="video-stage-tile local-tile">
            <canvas ref={localCameraCanvasRef} className="video-stage-canvas" />
            <div className="video-tile-overlay">
              <div className="video-tile-meta">
                <Video size={13} />
                <strong>{state.nickname} (Lokale Kamera)</strong>
                <span className="codec-pill">WebCodecs Encoder Aktiv</span>
              </div>
            </div>
          </div>
        )}

        {state.screenSharing && (
          <div className="video-stage-tile local-tile">
            <canvas ref={localScreenCanvasRef} className="video-stage-canvas" />
            <div className="video-tile-overlay">
              <div className="video-tile-meta">
                <Monitor size={13} />
                <strong>{state.nickname} (Lokaler Screen-Share)</strong>
                <span className="codec-pill">WebCodecs Encoder Aktiv</span>
              </div>
            </div>
          </div>
        )}

        {remoteEntries.map(([key, stream]) => (
          <RemoteStreamTile key={key} streamKey={key} stream={stream} />
        ))}
      </div>
    </section>
  );
};
