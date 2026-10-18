import React from 'react';
import { KeyRound, Server, ShieldCheck } from 'lucide-react';
import {
  blackTeaStore,
  BlackTeaState,
  ClientCapabilities,
} from '../../../store/useBlackTeaStore';

export interface ServerInspectorViewProps {
  state: BlackTeaState;
  caps: ClientCapabilities;
}

export const ServerInspectorView: React.FC<ServerInspectorViewProps> = ({
  state,
  caps,
}) => {
  return (
    <aside className="inspector-panel">
      <div className="inspector-header">
        <div className="inspector-avatar server">
          <Server size={20} />
        </div>
        <div>
          <h3 className="inspector-title">{state.serverInfo.virtualserver_name}</h3>
          <span className="inspector-subtitle">
            {state.clients.length} / {state.serverInfo.virtualserver_maxclients} Clients online
          </span>
        </div>
      </div>

      {caps.isConnected && (
        <div className="inspector-section">
          <h4>Dein Status auf diesem Server</h4>
          <div className="self-role-card">
            <div className="self-role-row">
              <span>Angemeldet als</span>
              <strong>{state.nickname}</strong>
            </div>
            <div className="self-role-row">
              <span>Berechtigungsstufe</span>
              <span className={`readonly-group-pill ${caps.isAdmin ? 'admin-pill' : ''}`}>
                {caps.isAdmin && <ShieldCheck size={12} />}
                {caps.roleLabel}
              </span>
            </div>
            {!caps.isAdmin && (
              <button
                className="btn-key-inline"
                onClick={() => blackTeaStore.patchState({ activeModal: 'redeem_key' })}
              >
                <KeyRound size={13} /> Admin-Schlüssel einlösen
              </button>
            )}
          </div>
        </div>
      )}

      <div className="inspector-section">
        <h4>Willkommensnachricht</h4>
        <div className="inspector-description-box">
          {state.serverInfo.virtualserver_welcomemessage}
        </div>
      </div>

      <div className="inspector-section">
        <h4>Server-Informationen</h4>
        <div className="kv-list">
          <div className="kv-row">
            <span>Channels</span>
            <span>{state.channels.length}</span>
          </div>
          <div className="kv-row">
            <span>Sprachverschlüsselung</span>
            <span>TLS 1.3 / QUIC AEAD</span>
          </div>
          {caps.isAdmin && (
            <>
              <div className="kv-row">
                <span>Server-Version</span>
                <span>{state.serverInfo.virtualserver_version}</span>
              </div>
              <div className="kv-row">
                <span>Plattform</span>
                <span>{state.serverInfo.virtualserver_platform}</span>
              </div>
            </>
          )}
        </div>
      </div>
    </aside>
  );
};
