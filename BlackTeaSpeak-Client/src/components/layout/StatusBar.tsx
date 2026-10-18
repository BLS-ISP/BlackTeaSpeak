import React from 'react';
import { Activity, Cpu, Radio, ShieldCheck, UserCheck } from 'lucide-react';
import { getClientCapabilities, useBlackTeaStore } from '../../store/useBlackTeaStore';

export const StatusBar: React.FC = () => {
  const state = useBlackTeaStore();
  const { telemetry } = state;
  const caps = getClientCapabilities(state);

  return (
    <footer className="status-bar">
      <div className="status-item">
        <Activity size={13} className="accent-emerald" />
        <span>
          Protokoll: <strong>HTTP/3 QUIC</strong>
        </span>
      </div>

      <div className="status-item">
        <Cpu size={13} />
        <span>
          Audio: <strong>Opus 48kHz</strong> (TX {telemetry.voicePacketsSent} • RX{' '}
          {telemetry.voicePacketsReceived})
        </span>
      </div>

      <div className="status-item">
        <Radio size={13} />
        <span>
          Raumklang: <strong>{telemetry.spatial3DEnabled ? '3D HRTF' : 'Stereo'}</strong>
        </span>
      </div>

      {caps.isConnected && (
        <div className="status-item">
          {caps.isAdmin ? (
            <ShieldCheck size={13} className="accent-amber" />
          ) : (
            <UserCheck size={13} />
          )}
          <span>
            Rolle: <strong>{caps.roleLabel}</strong>
          </span>
        </div>
      )}

      <div className="status-item right">
        <span>{state.serverInfo.virtualserver_version}</span>
      </div>
    </footer>
  );
};
