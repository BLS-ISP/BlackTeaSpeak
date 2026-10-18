import React from 'react';
import {
  Clock,
  Compass,
  FolderOpen,
  Gavel,
  Key,
  KeyRound,
  Mic,
  MicOff,
  Monitor,
  Music,
  Radio,
  Settings,
  Shield,
  ShieldCheck,
  Sliders,
  UserCheck,
  Video,
  VideoOff,
  Volume2,
  VolumeX,
} from 'lucide-react';
import {
  blackTeaStore,
  getClientCapabilities,
  useBlackTeaStore,
} from '../../store/useBlackTeaStore';

export const TopToolbar: React.FC = () => {
  const state = useBlackTeaStore();
  const isConnected = state.connectionState === 'connected';
  const caps = getClientCapabilities(state);
  const selfClient = state.clients.find((c) => c.clid === state.selfClientId);
  const isAway = Boolean(selfClient?.client_away);
  const isCommander = Boolean(selfClient?.client_is_channel_commander);

  return (
    <header className="top-toolbar">
      <div className="toolbar-brand">
        <div className="brand-logo">BT</div>
        <div className="brand-text">
          <div className="brand-title-row">
            <span className="brand-title">BlackTeaSpeak</span>
            <span className="brand-version-pill">v2.0</span>
          </div>
          <div className="brand-subtitle">HTTP/3 QUIC • WebCodecs • 3D HRTF</div>
        </div>
      </div>

      {/* Audio & Voice Dock */}
      <div className="toolbar-pill-group">
        <button
          className={`tool-btn ${state.inputMuted ? 'danger' : 'active'}`}
          onClick={() => blackTeaStore.toggleInputMute()}
          title={state.inputMuted ? 'Mikrofon aktivieren' : 'Mikrofon stummschalten'}
        >
          {state.inputMuted ? <MicOff size={15} /> : <Mic size={15} />}
          <span>{state.inputMuted ? 'Stumm' : 'Mikrofon'}</span>
        </button>

        <button
          className={`tool-btn ${state.outputDeafened ? 'danger' : ''}`}
          onClick={() => blackTeaStore.toggleOutputDeafen()}
          title={state.outputDeafened ? 'Ton aktivieren' : 'Ton komplett stummschalten'}
        >
          {state.outputDeafened ? <VolumeX size={15} /> : <Volume2 size={15} />}
          <span>{state.outputDeafened ? 'Taub' : 'Sound'}</span>
        </button>

        <button
          className={`tool-btn ${isAway ? 'warning' : ''}`}
          disabled={!isConnected}
          onClick={(e) => {
            if (isAway) {
              void blackTeaStore.toggleAway(false, '');
            } else if (e.shiftKey) {
              const msg = window.prompt('Abwesenheitsnotiz (AFK):', 'Kurz weg');
              if (msg !== null) void blackTeaStore.toggleAway(true, msg);
            } else {
              void blackTeaStore.toggleAway(true, 'AFK');
            }
          }}
          title="Abwesenheitsstatus (AFK) umschalten (Shift+Klick für eigene Nachricht)"
        >
          <Clock size={15} />
          <span>{isAway ? 'AFK' : 'Anwesend'}</span>
        </button>

        <button
          className={`tool-btn ${state.whisperMode ? 'warning' : ''}`}
          onClick={() => blackTeaStore.toggleWhisperMode()}
          title="Whisper-Modus umschalten"
        >
          <Radio size={15} />
          <span>{state.whisperMode ? 'Whisper AN' : 'Whisper'}</span>
        </button>

        <button
          className={`tool-btn ${isCommander ? 'warning' : ''}`}
          disabled={!isConnected}
          onClick={() => void blackTeaStore.toggleChannelCommander()}
          title="Channel Commander Status ein-/ausschalten"
        >
          <Radio size={15} />
          <span>{isCommander ? 'CC AN' : 'CC'}</span>
        </button>

        <button
          className={`tool-btn ${state.soundPack === 'none' ? 'muted-btn' : 'accent'}`}
          onClick={() => blackTeaStore.cycleSoundPack()}
          title="Sprachansagen umschalten (Female / Male / Aus)"
        >
          <Volume2 size={15} />
          <span>
            {state.soundPack === 'default'
              ? 'Female'
              : state.soundPack === 'default_male'
              ? 'Male'
              : 'Lautlos'}
          </span>
        </button>

        <div className="vad-meter" title={`Eingangspegel: ${state.telemetry.inputLevelDb} dB`}>
          <div
            className={`vad-fill ${state.telemetry.vadActive ? 'transmitting' : ''}`}
            style={{
              width: `${Math.max(
                0,
                Math.min(100, ((state.telemetry.inputLevelDb + 80) / 80) * 100)
              )}%`,
            }}
          />
          <span className="vad-label">
            {state.transmissionMode.toUpperCase()} {state.telemetry.inputLevelDb} dB
          </span>
        </div>
      </div>

      {/* Media & Spatial Features */}
      <div className="toolbar-pill-group">
        <button
          className={`tool-btn ${state.cameraSharing ? 'success' : ''}`}
          onClick={() => blackTeaStore.toggleCameraShare()}
          disabled={!isConnected}
          title="WebCodecs Kamera-Stream starten/stoppen"
        >
          {state.cameraSharing ? <Video size={15} /> : <VideoOff size={15} />}
          <span>Kamera</span>
        </button>

        <button
          className={`tool-btn ${state.screenSharing ? 'success' : ''}`}
          onClick={() => blackTeaStore.toggleScreenShare()}
          disabled={!isConnected}
          title="WebCodecs Bildschirmübertragung starten/stoppen"
        >
          <Monitor size={15} />
          <span>Screen</span>
        </button>

        <button
          className={`tool-btn ${state.showSpatialRadar ? 'accent' : ''}`}
          onClick={() =>
            blackTeaStore.patchState({ showSpatialRadar: !state.showSpatialRadar })
          }
          title="3D HRTF Spatial Audio Radar ein-/ausblenden"
        >
          <Compass size={15} />
          <span>3D Audio</span>
        </button>

        <button
          className={`tool-btn ${state.showMusicBotDeck ? 'accent' : ''}`}
          onClick={() => {
            const next = !state.showMusicBotDeck;
            blackTeaStore.patchState({ showMusicBotDeck: next });
            if (next && isConnected) blackTeaStore.refreshMusicBots();
          }}
          disabled={!isConnected}
          title="MusicBot Player & Stream-Deck öffnen"
        >
          <Music size={15} />
          <span>Radio{state.musicBots.length > 0 ? ` (${state.musicBots.length})` : ''}</span>
        </button>

        <button
          className="tool-btn"
          onClick={() => {
            blackTeaStore.patchState({ activeModal: 'files' });
            blackTeaStore.fetchChannelFiles(
              state.selectedChannelId ?? state.currentChannelId,
              '/'
            );
          }}
          disabled={!isConnected}
          title="Channel-Dateibrowser öffnen"
        >
          <FolderOpen size={15} />
          <span>Dateien</span>
        </button>
      </div>

      {/* Role Status, Privilege Key & Permission-Gated Admin Controls */}
      <div className="toolbar-pill-group admin-toolbar-group">
        {isConnected && (
          <div
            className={`role-status-badge ${caps.isAdmin ? 'role-admin' : caps.isModerator ? 'role-mod' : 'role-normal'}`}
            title={
              caps.isAdmin
                ? 'Du bist als Server Admin authentifiziert (Voller Zugriff)'
                : 'Standard-Benutzerrechte aktiv'
            }
          >
            {caps.isAdmin ? <ShieldCheck size={13} /> : <UserCheck size={13} />}
            <span>{caps.roleLabel}</span>
          </div>
        )}

        {!caps.isAdmin && (
          <button
            className="tool-btn key-unlock-btn"
            onClick={() => blackTeaStore.patchState({ activeModal: 'redeem_key' })}
            disabled={!isConnected}
            title="Berechtigungsschlüssel (Privilege Key) einlösen, um Admin-Rechte freizuschalten"
          >
            <KeyRound size={15} />
            <span>Schlüssel einlösen</span>
          </button>
        )}

        {caps.canManagePermissions && (
          <button
            className="tool-btn admin-action-btn"
            onClick={() => blackTeaStore.patchState({ activeModal: 'permissions' })}
            title="5-Tier Permission-System verwalten"
          >
            <Shield size={15} />
            <span>Rechte</span>
          </button>
        )}

        {caps.canManageGroupsAndKeys && (
          <button
            className="tool-btn admin-action-btn"
            onClick={() => {
              blackTeaStore.patchState({ activeModal: 'groups_tokens' });
              blackTeaStore.refreshTokens();
            }}
            title="Server-/Channelgruppen & Berechtigungsschlüssel verwalten"
          >
            <Key size={15} />
            <span>Gruppen &amp; Keys</span>
          </button>
        )}

        {caps.canModerate && (
          <button
            className="tool-btn admin-action-btn"
            onClick={() => {
              blackTeaStore.patchState({ activeModal: 'moderation' });
              blackTeaStore.refreshModerationLists();
            }}
            title="Bannliste, Beschwerden & Offline-Nachrichten verwalten"
          >
            <Gavel size={15} />
            <span>Moderation</span>
          </button>
        )}

        <button
          className={`tool-btn ${caps.canManageServer ? 'admin-action-btn' : ''}`}
          onClick={() => {
            blackTeaStore.patchState({ activeModal: 'server_admin' });
            if (isConnected && caps.canManageServer) {
              blackTeaStore.refreshServerAdminData();
            }
          }}
          title={
            caps.canManageServer
              ? 'Server-Administration, TSDNS & Audio-Einstellungen'
              : 'Audio- & Sprachübertragungs-Einstellungen'
          }
        >
          {caps.canManageServer ? <Settings size={15} /> : <Sliders size={15} />}
          <span>{caps.canManageServer ? 'Admin' : 'Optionen'}</span>
        </button>
      </div>
    </header>
  );
};
