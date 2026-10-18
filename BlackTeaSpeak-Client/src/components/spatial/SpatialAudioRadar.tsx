import React, { useState } from 'react';
import { Compass, RotateCcw, Power } from 'lucide-react';
import { blackTeaStore, useBlackTeaStore } from '../../store/useBlackTeaStore';

export const SpatialAudioRadar: React.FC = () => {
  const state = useBlackTeaStore();
  const [draggingClid, setDraggingClid] = useState<number | null>(null);

  if (!state.showSpatialRadar) return null;

  const channelPeers = state.clients.filter(
    (c) => c.cid === state.currentChannelId && c.clid !== state.selfClientId
  );

  const handleRadarPointerMove = (e: React.PointerEvent<SVGSVGElement>) => {
    if (draggingClid === null) return;
    const rect = e.currentTarget.getBoundingClientRect();
    const cx = rect.width / 2;
    const cy = rect.height / 2;
    const px = e.clientX - rect.left;
    const py = e.clientY - rect.top;

    // Map radar pixels (-110..+110) to 3D meters (-10m..+10m)
    const xMeters = Math.max(-10, Math.min(10, ((px - cx) / 100) * 8));
    const zMeters = Math.max(-10, Math.min(10, ((py - cy) / 100) * 8));

    blackTeaStore.updateClientSpatialPosition(draggingClid, {
      x: Number(xMeters.toFixed(2)),
      y: 0,
      z: Number(zMeters.toFixed(2)),
    });
  };

  const resetPositions = () => {
    channelPeers.forEach((peer, index) => {
      const angle = (index / Math.max(1, channelPeers.length)) * Math.PI * 2;
      blackTeaStore.updateClientSpatialPosition(peer.clid, {
        x: Number((Math.cos(angle) * 2.5).toFixed(2)),
        y: 0,
        z: Number((Math.sin(angle) * 2.5).toFixed(2)),
      });
    });
  };

  return (
    <section className="spatial-radar-card">
      <div className="radar-header">
        <div className="radar-title">
          <Compass size={15} />
          <span>3D HRTF Spatial Audio Radar (WebAudio PannerNode)</span>
        </div>
        <div className="radar-actions">
          <button
            className={`btn-xs ${state.telemetry.spatial3DEnabled ? 'success' : ''}`}
            onClick={() => blackTeaStore.toggleSpatial3D()}
          >
            <Power size={12} /> {state.telemetry.spatial3DEnabled ? 'HRTF Aktiv' : 'Stereo'}
          </button>
          <button className="btn-xs" onClick={resetPositions} title="Positionen im Kreis anordnen">
            <RotateCcw size={12} /> Reset
          </button>
        </div>
      </div>

      <div className="radar-body">
        <svg
          className="radar-svg"
          viewBox="-120 -120 240 240"
          onPointerMove={handleRadarPointerMove}
          onPointerUp={() => setDraggingClid(null)}
          onPointerLeave={() => setDraggingClid(null)}
        >
          <circle cx="0" cy="0" r="105" className="radar-ring outer" />
          <circle cx="0" cy="0" r="70" className="radar-ring mid" />
          <circle cx="0" cy="0" r="35" className="radar-ring inner" />
          <line x1="-110" y1="0" x2="110" y2="0" className="radar-axis" />
          <line x1="0" y1="-110" x2="0" y2="110" className="radar-axis" />

          {/* Listener (Self) at center */}
          <g transform="translate(0, 0)">
            <circle r="9" className="radar-self-node" />
            <text y="20" textAnchor="middle" className="radar-label self">
              Du (Hörer)
            </text>
          </g>

          {/* Channel Peers */}
          {channelPeers.map((peer) => {
            const pos = peer.spatialPos ?? { x: 0, y: 0, z: -2 };
            const sx = (pos.x / 8) * 100;
            const sy = (pos.z / 8) * 100;
            return (
              <g
                key={peer.clid}
                transform={`translate(${sx}, ${sy})`}
                className="radar-peer-group"
                onPointerDown={(e) => {
                  e.stopPropagation();
                  setDraggingClid(peer.clid);
                }}
              >
                {peer.isTalking && <circle r="15" className="radar-talk-pulse" />}
                <circle r="8" className="radar-peer-node" />
                <text y="-12" textAnchor="middle" className="radar-label">
                  {peer.client_nickname}
                </text>
              </g>
            );
          })}
        </svg>

        <div className="radar-legend">
          <p>
            Ziehe Teilnehmer mit der Maus im Raum, um Richtung &amp; Distanz über die{' '}
            <strong>WebAudio HRTF PannerNode</strong> Engine in Echtzeit anzupassen.
          </p>
          <div className="radar-peer-list">
            {channelPeers.length === 0 ? (
              <span className="muted-note">Keine weiteren Teilnehmer im aktuellen Channel.</span>
            ) : (
              channelPeers.map((p) => (
                <div key={p.clid} className="radar-peer-chip">
                  <strong>{p.client_nickname}:</strong> X={(p.spatialPos?.x ?? 0).toFixed(1)}m / Z=
                  {(p.spatialPos?.z ?? 0).toFixed(1)}m
                </div>
              ))
            )}
          </div>
        </div>
      </div>
    </section>
  );
};
