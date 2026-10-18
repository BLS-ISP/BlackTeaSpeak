import React, { useEffect, useState } from 'react';
import {
  Bookmark,
  ChevronDown,
  ChevronUp,
  Fingerprint,
  Plug,
  RefreshCw,
  ShieldCheck,
  SlidersHorizontal,
  Star,
  Unplug,
  Wifi,
} from 'lucide-react';
import { blackTeaStore, useBlackTeaStore } from '../../store/useBlackTeaStore';
import { discoverServerCertHash } from '../../protocols/webtransport/WebTransportConnection';

export const ConnectionBanner: React.FC = () => {
  const state = useBlackTeaStore();
  const isConnected = state.connectionState === 'connected';
  const [expanded, setExpanded] = useState(!isConnected);
  const [showAdvancedTls, setShowAdvancedTls] = useState(false);

  // Auto-collapse connection form once connected for a cleaner workspace
  useEffect(() => {
    if (isConnected) {
      setExpanded(false);
    }
  }, [isConnected]);

  const handleAutoDiscoverCert = async () => {
    const res = await discoverServerCertHash(state.host);
    if (res.sha256) {
      blackTeaStore.patchState({
        certHash: res.sha256,
        port: res.webtransportPort ?? state.port,
      });
      blackTeaStore.showToast('Zertifikats-Hash (SHA-256) erfolgreich vom Server geladen');
    } else {
      blackTeaStore.showToast('Kein /cert-hash erreichbar — Standard-TLS wird genutzt');
    }
  };

  return (
    <div className={`connection-banner ${isConnected ? 'is-connected' : ''}`}>
      <div className="conn-summary-bar">
        <div className="conn-status-indicator">
          <span className={`status-dot ${state.connectionState}`} />
          <div className="conn-status-text">
            <strong>
              {isConnected
                ? state.serverInfo.virtualserver_name
                : state.connectionState === 'connecting'
                ? 'Verbinde über HTTP/3 WebTransport (QUIC)...'
                : 'Nicht mit Server verbunden'}
            </strong>
            {isConnected && (
              <span className="conn-endpoint-badge">
                <Wifi size={11} /> {state.host}:{state.port} • Als{' '}
                <em>{state.nickname}</em>
              </span>
            )}
          </div>
          {state.certHash && (
            <span className="cert-pill" title={`Pinned SHA-256: ${state.certHash}`}>
              <ShieldCheck size={12} /> TLS Pinned
            </span>
          )}
        </div>

        <div className="conn-quick-actions">
          {state.desktopFavorites.length > 0 && (
            <select
              className="btn-secondary-sm"
              style={{ maxWidth: '210px', cursor: 'pointer' }}
              value=""
              onChange={(e) => {
                const fav = state.desktopFavorites.find((f) => f.id === e.target.value);
                if (fav) void blackTeaStore.connectToFavorite(fav);
              }}
              title="Schnellverbindung zu einem gespeicherten Server-Favoriten"
            >
              <option value="" disabled>
                ★ Favorit schnell verbinden ({state.desktopFavorites.length})
              </option>
              {state.desktopFavorites.map((fav) => (
                <option key={fav.id} value={fav.id}>
                  {fav.name} ({fav.address})
                </option>
              ))}
            </select>
          )}

          <button
            type="button"
            className="btn-secondary-sm"
            onClick={() => void blackTeaStore.saveCurrentServerAsFavorite()}
            title="Aktuelle Server-Adresse als Favorit speichern"
          >
            <Star size={13} /> Merken
          </button>

          {isConnected ? (
            <button className="btn-danger-sm" onClick={() => blackTeaStore.disconnect()}>
              <Unplug size={13} /> Trennen
            </button>
          ) : (
            <button
              className="btn-primary-sm"
              onClick={() => blackTeaStore.connect()}
              disabled={state.connectionState === 'connecting'}
            >
              <Plug size={13} /> Verbinden
            </button>
          )}
          <button
            className="btn-ghost-sm"
            onClick={() => setExpanded(!expanded)}
            title="Verbindungseinstellungen ein-/ausklappen"
          >
            {expanded ? <ChevronUp size={14} /> : <ChevronDown size={14} />}
            <span>{expanded ? 'Ausblenden' : 'Verbindung'}</span>
          </button>
        </div>
      </div>

      {expanded && (
        <div className="conn-expand-panel">
          <div className="conn-form-grid">
            <div className="form-field">
              <label>Server-Adresse (Host / IP / TSDNS)</label>
              <input
                type="text"
                value={state.host}
                onChange={(e) => blackTeaStore.patchState({ host: e.target.value })}
                placeholder="127.0.0.1"
              />
            </div>

            <div className="form-field small">
              <label>Port</label>
              <input
                type="number"
                value={state.port}
                onChange={(e) => blackTeaStore.patchState({ port: Number(e.target.value) })}
                placeholder="9989"
              />
            </div>

            <div className="form-field">
              <label>Dein Nickname</label>
              <input
                type="text"
                value={state.nickname}
                onChange={(e) => blackTeaStore.patchState({ nickname: e.target.value })}
                placeholder="BlackTeaUser"
              />
            </div>

            <div className="form-field">
              <label>
                <Fingerprint size={11} /> Ed25519-Identität
              </label>
              <select
                value={state.activeIdentityId}
                onChange={(e) => blackTeaStore.selectDesktopIdentity(e.target.value)}
              >
                {state.desktopIdentities.map((id) => (
                  <option key={id.id} value={id.id}>
                    {id.name} ({id.uid.slice(0, 10)}…)
                  </option>
                ))}
              </select>
            </div>

            <div className="form-field">
              <label>Server-Passwort (optional)</label>
              <input
                type="password"
                value={state.serverPassword}
                onChange={(e) =>
                  blackTeaStore.patchState({ serverPassword: e.target.value })
                }
                placeholder="Leer lassen falls keins"
              />
            </div>

            <div className="form-field conn-advanced-toggle-col">
              <label>Erweitert</label>
              <div style={{ display: 'flex', gap: '0.4rem' }}>
                <button
                  type="button"
                  className="btn-secondary-sm"
                  onClick={() => blackTeaStore.patchState({ activeModal: 'desktop_client' })}
                  title="Favoriten & Identitäten verwalten"
                >
                  <Bookmark size={13} /> Favoriten
                </button>
                <button
                  type="button"
                  className={`btn-secondary-sm ${showAdvancedTls ? 'active-choice' : ''}`}
                  onClick={() => setShowAdvancedTls(!showAdvancedTls)}
                >
                  <SlidersHorizontal size={13} /> TLS
                </button>
              </div>
            </div>
          </div>

          {showAdvancedTls && (
            <div className="conn-advanced-row">
              <div className="form-field wide">
                <label>
                  Self-Signed TLS Zertifikat SHA-256 Hash (wird beim Verbinden automatisch via{' '}
                  <code>/cert-hash</code> erkannt)
                </label>
                <div className="input-with-btn">
                  <input
                    type="text"
                    value={state.certHash}
                    onChange={(e) => blackTeaStore.patchState({ certHash: e.target.value })}
                    placeholder="Automatische Erkennung beim Verbinden..."
                  />
                  <button
                    type="button"
                    className="btn-secondary-sm"
                    onClick={handleAutoDiscoverCert}
                    title="Zertifikats-Hash manuell vom Server abrufen"
                  >
                    <RefreshCw size={13} /> Auto-Detect
                  </button>
                </div>
              </div>
            </div>
          )}
        </div>
      )}

      {state.connectionError && (
        <div className="conn-error-banner">{state.connectionError}</div>
      )}
    </div>
  );
};

