import React, { useEffect, useState } from 'react';
import {
  Bookmark,
  Cpu,
  Fingerprint,
  KeyRound,
  Mic,
  Plug,
  Plus,
  RefreshCw,
  Star,
  Trash2,
  X,
} from 'lucide-react';
import { blackTeaStore, useBlackTeaStore } from '../../store/useBlackTeaStore';

export const DesktopClientModal: React.FC<{ onClose: () => void }> = ({ onClose }) => {
  const state = useBlackTeaStore();
  const [tab, setTab] = useState<'favorites' | 'identities' | 'hardware_sdk'>('favorites');

  // New Favorite form state
  const [favName, setFavName] = useState('');
  const [favAddress, setFavAddress] = useState(`${state.host}:${state.port}`);
  const [favNick, setFavNick] = useState(state.nickname);
  const [favPassword, setFavPassword] = useState('');
  const [favIdentityId, setFavIdentityId] = useState(state.activeIdentityId);

  // New Identity form state
  const [idName, setIdName] = useState('');
  const [idDefaultNick, setIdDefaultNick] = useState(state.nickname || 'BlackTeaUser');

  useEffect(() => {
    void blackTeaStore.refreshDesktopHardwareAndSdk();
  }, []);

  const activeIdentity =
    state.desktopIdentities.find((i) => i.id === state.activeIdentityId) ??
    state.desktopIdentities[0];

  const handleAddFavorite = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!favAddress.trim()) return;
    await blackTeaStore.addOrUpdateDesktopFavorite({
      name: favName.trim() || `Server ${favAddress.trim()}`,
      address: favAddress.trim(),
      nickname: favNick.trim() || state.nickname,
      password: favPassword,
      identity_id: favIdentityId || state.activeIdentityId,
    });
    setFavName('');
    setFavPassword('');
  };

  const handleCreateIdentity = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!idName.trim()) return;
    await blackTeaStore.createDesktopIdentity(idName.trim(), idDefaultNick.trim());
    setIdName('');
  };

  return (
    <div className="modal-card wide">
      <div className="modal-header">
        <div className="modal-title">
          <Bookmark size={18} />
          <span>Desktop-Client Zentrale — Favoriten, Ed25519-Identitäten &amp; Plugin SDK</span>
        </div>
        <button className="btn-icon-sm" onClick={onClose} title="Schließen">
          <X size={16} />
        </button>
      </div>

      <div className="modal-tabs">
        <button
          className={`modal-tab ${tab === 'favorites' ? 'active' : ''}`}
          onClick={() => setTab('favorites')}
        >
          <Star size={14} /> Server-Favoriten ({state.desktopFavorites.length})
        </button>
        <button
          className={`modal-tab ${tab === 'identities' ? 'active' : ''}`}
          onClick={() => setTab('identities')}
        >
          <Fingerprint size={14} /> Ed25519-Identitäten ({state.desktopIdentities.length})
        </button>
        <button
          className={`modal-tab ${tab === 'hardware_sdk' ? 'active' : ''}`}
          onClick={() => setTab('hardware_sdk')}
        >
          <Cpu size={14} /> Audio-Hardware, Hotkeys &amp; Plugin SDK
        </button>
      </div>

      <div className="modal-body">
        {tab === 'favorites' && (
          <div className="stack-form">
            <div
              style={{
                display: 'flex',
                justifyContent: 'space-between',
                alignItems: 'center',
                gap: '0.75rem',
                flexWrap: 'wrap',
              }}
            >
              <div>
                <strong>Gespeicherte Server-Favoriten (Bookmarks)</strong>
                <div style={{ fontSize: '0.78rem', opacity: 0.75 }}>
                  1-Klick-Verbindung inkl. automatischer Port-Erkennung &amp; hinterlegter Identität.
                </div>
              </div>
              <button
                type="button"
                className="btn-secondary-sm"
                onClick={() => void blackTeaStore.saveCurrentServerAsFavorite()}
              >
                <Star size={13} /> Aktuellen Server ({state.host}:{state.port}) merken
              </button>
            </div>

            <div className="data-table-wrap">
              <table className="data-table">
                <thead>
                  <tr>
                    <th>Server-Name</th>
                    <th>Adresse</th>
                    <th>Nickname</th>
                    <th>Identität</th>
                    <th>Aktionen</th>
                  </tr>
                </thead>
                <tbody>
                  {state.desktopFavorites.length === 0 ? (
                    <tr>
                      <td colSpan={5} style={{ textAlign: 'center', opacity: 0.65 }}>
                        Noch keine Favoriten gespeichert.
                      </td>
                    </tr>
                  ) : (
                    state.desktopFavorites.map((fav) => {
                      const idObj = state.desktopIdentities.find(
                        (i) => i.id === fav.identity_id
                      );
                      return (
                        <tr key={fav.id}>
                          <td>
                            <strong>{fav.name}</strong>
                          </td>
                          <td>
                            <code>{fav.address}</code>
                          </td>
                          <td>{fav.nickname}</td>
                          <td>{idObj?.name ?? 'Standard'}</td>
                          <td>
                            <div style={{ display: 'flex', gap: '0.4rem' }}>
                              <button
                                type="button"
                                className="btn-primary-sm"
                                onClick={() => void blackTeaStore.connectToFavorite(fav)}
                              >
                                <Plug size={12} /> Verbinden
                              </button>
                              <button
                                type="button"
                                className="btn-danger-sm"
                                onClick={() =>
                                  void blackTeaStore.deleteDesktopFavorite(fav.id)
                                }
                                title="Favorit löschen"
                              >
                                <Trash2 size={12} />
                              </button>
                            </div>
                          </td>
                        </tr>
                      );
                    })
                  )}
                </tbody>
              </table>
            </div>

            <form className="stack-form" onSubmit={handleAddFavorite}>
              <strong>Neuen Server-Favoriten anlegen</strong>
              <div className="conn-form-grid">
                <div className="form-field">
                  <label>Bezeichnung</label>
                  <input
                    type="text"
                    value={favName}
                    onChange={(e) => setFavName(e.target.value)}
                    placeholder="z.B. Clan-Hauptserver"
                  />
                </div>
                <div className="form-field">
                  <label>Adresse:Port (oder TSDNS)</label>
                  <input
                    type="text"
                    value={favAddress}
                    onChange={(e) => setFavAddress(e.target.value)}
                    placeholder="127.0.0.1:9989"
                  />
                </div>
                <div className="form-field">
                  <label>Nickname</label>
                  <input
                    type="text"
                    value={favNick}
                    onChange={(e) => setFavNick(e.target.value)}
                    placeholder="BlackTeaUser"
                  />
                </div>
                <div className="form-field">
                  <label>Passwort (optional)</label>
                  <input
                    type="password"
                    value={favPassword}
                    onChange={(e) => setFavPassword(e.target.value)}
                    placeholder="Leer falls keins"
                  />
                </div>
                <div className="form-field">
                  <label>Verknüpfte Identität</label>
                  <select
                    value={favIdentityId}
                    onChange={(e) => setFavIdentityId(e.target.value)}
                  >
                    {state.desktopIdentities.map((id) => (
                      <option key={id.id} value={id.id}>
                        {id.name} ({id.default_nickname})
                      </option>
                    ))}
                  </select>
                </div>
              </div>
              <div>
                <button type="submit" className="btn-primary-sm">
                  <Plus size={13} /> Favorit hinzufügen
                </button>
              </div>
            </form>
          </div>
        )}

        {tab === 'identities' && (
          <div className="stack-form">
            <div>
              <strong>Kryptografische Ed25519-Identitäten</strong>
              <div style={{ fontSize: '0.78rem', opacity: 0.75 }}>
                Deine eindeutige kryptografische Signatur (Public Key &amp; SHA-1 UID) bestimmt deine Server-Gruppen und Rechte auf allen BlackTeaSpeak- und TeaSpeak-Servern.
              </div>
            </div>

            <div className="data-table-wrap">
              <table className="data-table">
                <thead>
                  <tr>
                    <th>Name</th>
                    <th>Standard-Nickname</th>
                    <th>Eindeutige ID (UID)</th>
                    <th>Public Key (Ed25519)</th>
                    <th>Status / Aktion</th>
                  </tr>
                </thead>
                <tbody>
                  {state.desktopIdentities.map((id) => {
                    const isActive = id.id === state.activeIdentityId;
                    return (
                      <tr key={id.id}>
                        <td>
                          <strong>{id.name}</strong>
                        </td>
                        <td>{id.default_nickname}</td>
                        <td>
                          <code title={id.uid}>{id.uid}</code>
                        </td>
                        <td>
                          <code title={id.public_key}>
                            {id.public_key.slice(0, 18)}…
                          </code>
                        </td>
                        <td>
                          <div style={{ display: 'flex', gap: '0.4rem' }}>
                            {isActive ? (
                              <span className="cert-pill">Aktiv</span>
                            ) : (
                              <button
                                type="button"
                                className="btn-secondary-sm"
                                onClick={() => blackTeaStore.selectDesktopIdentity(id.id)}
                              >
                                Auswählen
                              </button>
                            )}
                            {state.desktopIdentities.length > 1 && (
                              <button
                                type="button"
                                className="btn-danger-sm"
                                onClick={() =>
                                  void blackTeaStore.deleteDesktopIdentity(id.id)
                                }
                                title="Identität löschen"
                              >
                                <Trash2 size={12} />
                              </button>
                            )}
                          </div>
                        </td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            </div>

            <form className="stack-form" onSubmit={handleCreateIdentity}>
              <strong>Neue Ed25519-Identität generieren</strong>
              <div className="form-row-2">
                <div className="form-field">
                  <label>Identitäts-Bezeichnung</label>
                  <input
                    type="text"
                    value={idName}
                    onChange={(e) => setIdName(e.target.value)}
                    placeholder="z.B. Server-Admin Profil / Zweit-ID"
                  />
                </div>
                <div className="form-field">
                  <label>Standard-Nickname für dieses Profil</label>
                  <input
                    type="text"
                    value={idDefaultNick}
                    onChange={(e) => setIdDefaultNick(e.target.value)}
                    placeholder="BlackTeaUser"
                  />
                </div>
              </div>
              <div>
                <button type="submit" className="btn-primary-sm">
                  <KeyRound size={13} /> Ed25519 Schlüsselpaar generieren
                </button>
              </div>
            </form>
          </div>
        )}

        {tab === 'hardware_sdk' && activeIdentity && (
          <div className="stack-form">
            <div
              style={{
                display: 'flex',
                justifyContent: 'space-between',
                alignItems: 'center',
              }}
            >
              <strong>
                Audio-Hardware &amp; DSP für aktive Identität ({activeIdentity.name})
              </strong>
              <button
                type="button"
                className="btn-secondary-sm"
                onClick={() => void blackTeaStore.refreshDesktopHardwareAndSdk()}
              >
                <RefreshCw size={13} /> Geräte neu scannen
              </button>
            </div>

            <div className="form-row-2">
              <div className="form-field">
                <label>
                  <Mic size={13} /> Eingabegerät (Mikrofon)
                </label>
                <select
                  value={activeIdentity.audio_input_device ?? ''}
                  onChange={(e) =>
                    void blackTeaStore.updateActiveIdentitySettings({
                      audio_input_device: e.target.value || null,
                    })
                  }
                >
                  <option value="">System-Standard Mikrofon</option>
                  {state.desktopAudioDevices.inputs.map((dev) => (
                    <option key={dev} value={dev}>
                      {dev}
                    </option>
                  ))}
                </select>
              </div>

              <div className="form-field">
                <label>Ausgabegerät (Lautsprecher / Headset)</label>
                <select
                  value={activeIdentity.audio_output_device ?? ''}
                  onChange={(e) =>
                    void blackTeaStore.updateActiveIdentitySettings({
                      audio_output_device: e.target.value || null,
                    })
                  }
                >
                  <option value="">System-Standard Lautsprecher</option>
                  {state.desktopAudioDevices.outputs.map((dev) => (
                    <option key={dev} value={dev}>
                      {dev}
                    </option>
                  ))}
                </select>
              </div>
            </div>

            <div className="form-row-2">
              <div className="form-field">
                <label>Push-To-Talk Taste / Globaler Hotkey</label>
                <input
                  type="text"
                  value={activeIdentity.ptt_hotkey ?? state.pttKey}
                  onChange={(e) =>
                    void blackTeaStore.updateActiveIdentitySettings({
                      ptt_hotkey: e.target.value,
                    })
                  }
                  placeholder="Space / Mouse4 / F13"
                />
              </div>

              <div className="form-field">
                <label>Whisper-Hotkey (optional)</label>
                <input
                  type="text"
                  value={activeIdentity.whisper_hotkey ?? ''}
                  onChange={(e) =>
                    void blackTeaStore.updateActiveIdentitySettings({
                      whisper_hotkey: e.target.value || null,
                    })
                  }
                  placeholder="z.B. Alt+W"
                />
              </div>
            </div>

            <div className="form-field">
              <label>Digitale Signalverarbeitung (DSP)</label>
              <div style={{ display: 'flex', gap: '0.6rem', flexWrap: 'wrap' }}>
                <button
                  type="button"
                  className={`btn-secondary-sm ${
                    activeIdentity.noise_suppression !== false ? 'active-choice' : ''
                  }`}
                  onClick={() =>
                    void blackTeaStore.updateActiveIdentitySettings({
                      noise_suppression: !(activeIdentity.noise_suppression !== false),
                    })
                  }
                >
                  Rauschunterdrückung (RNNoise/DSP):{' '}
                  {activeIdentity.noise_suppression !== false ? 'AN' : 'AUS'}
                </button>

                <button
                  type="button"
                  className={`btn-secondary-sm ${
                    activeIdentity.auto_gain_control !== false ? 'active-choice' : ''
                  }`}
                  onClick={() =>
                    void blackTeaStore.updateActiveIdentitySettings({
                      auto_gain_control: !(activeIdentity.auto_gain_control !== false),
                    })
                  }
                >
                  Auto Gain Control (AGC):{' '}
                  {activeIdentity.auto_gain_control !== false ? 'AN' : 'AUS'}
                </button>

                <button
                  type="button"
                  className={`btn-secondary-sm ${
                    activeIdentity.echo_cancellation !== false ? 'active-choice' : ''
                  }`}
                  onClick={() =>
                    void blackTeaStore.updateActiveIdentitySettings({
                      echo_cancellation: !(activeIdentity.echo_cancellation !== false),
                    })
                  }
                >
                  Echounterdrückung (AEC):{' '}
                  {activeIdentity.echo_cancellation !== false ? 'AN' : 'AUS'}
                </button>
              </div>
            </div>

            <div className="privilege-hint-row">
              <div>
                <strong>
                  Lokaler Plugin-SDK WebSocket-Server (
                  <code>ws://{state.pluginSdkStatus.listen_addr}</code>)
                </strong>
                <div style={{ fontSize: '0.76rem', opacity: 0.8 }}>
                  Ermöglicht externen Spielen &amp; Overlays (z.B. GTA/FiveM/Arma/SimRacing) die Live-Synchronisation von 3D-Spatial-Audio-Koordinaten und Client-Volumes. Verbunden:{' '}
                  <strong>{state.pluginSdkStatus.connected_plugins} Plugins</strong>.
                </div>
              </div>
              <button
                type="button"
                className={
                  state.pluginSdkStatus.enabled ? 'btn-primary-sm' : 'btn-secondary-sm'
                }
                onClick={() =>
                  void blackTeaStore.togglePluginSdk(!state.pluginSdkStatus.enabled)
                }
              >
                {state.pluginSdkStatus.enabled
                  ? 'Plugin SDK ist AKTIV'
                  : 'Plugin SDK einschalten'}
              </button>
            </div>
          </div>
        )}
      </div>
    </div>
  );
};
