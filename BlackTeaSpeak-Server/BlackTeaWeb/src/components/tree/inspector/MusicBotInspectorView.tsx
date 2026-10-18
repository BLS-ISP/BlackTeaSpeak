import React from 'react';
import {
  Edit3,
  MapPin,
  Music,
  Pause,
  Play,
  SkipBack,
  SkipForward,
  Square,
  Trash2,
  Volume2,
  VolumeX,
} from 'lucide-react';
import {
  blackTeaStore,
  BlackTeaState,
  ClientCapabilities,
} from '../../../store/useBlackTeaStore';
import { ClientInfo, MusicBotInfo } from '../../../types/protocol';

export interface MusicBotInspectorViewProps {
  state: BlackTeaState;
  caps: ClientCapabilities;
  selectedClient: ClientInfo;
  selectedBot: MusicBotInfo;
}

export const MusicBotInspectorView: React.FC<MusicBotInspectorViewProps> = ({
  state,
  caps,
  selectedClient,
  selectedBot,
}) => {
  const rawPlaying =
    selectedBot.player_state === 1 || selectedBot.player_state === 2;
  const isPaused = selectedBot.player_state === 3;

  return (
    <aside className="inspector-panel">
      <div className="inspector-header">
        <div className="inspector-avatar server">
          <Music size={20} />
        </div>
        <div>
          <h3 className="inspector-title">
            {selectedBot.bot_name}
            <span className="you-pill">MusicBot</span>
          </h3>
          <span className="inspector-subtitle">
            Opus 48 kHz Stereo • Bot #{selectedBot.bot_id}
          </span>
        </div>
      </div>

      <div className="inspector-section">
        <h4>Aktuelle Wiedergabe</h4>
        <div className="inspector-description-box">
          <strong style={{ display: 'block', marginBottom: 4, color: '#e2e8f0' }}>
            {selectedBot.song_title}
          </strong>
          <span style={{ fontSize: 11, color: '#94a3b8' }}>
            Status:{' '}
            {rawPlaying
              ? '▶️ Spielt Audio-Stream'
              : isPaused
              ? '⏸️ Pausiert'
              : '⏹️ Bereit'}
            {' • '}Warteschlange: {selectedBot.queue?.length ?? 0} Titel
          </span>
        </div>

        {caps.canManageMusicBots && (
          <div
            className="inspector-action-grid"
            style={{ marginTop: 8, gridTemplateColumns: 'repeat(4, 1fr)' }}
          >
            <button
              className="btn-secondary-sm"
              onClick={() => blackTeaStore.controlMusicBot(selectedBot.bot_id, 'prev')}
              title="Vorheriger Titel"
            >
              <SkipBack size={13} />
            </button>
            <button
              className="btn-primary-sm"
              onClick={() =>
                blackTeaStore.controlMusicBot(
                  selectedBot.bot_id,
                  rawPlaying ? 'pause' : 'play'
                )
              }
              title={rawPlaying ? 'Pause' : 'Play'}
            >
              {rawPlaying ? <Pause size={13} /> : <Play size={13} />}
            </button>
            <button
              className="btn-secondary-sm"
              onClick={() => blackTeaStore.controlMusicBot(selectedBot.bot_id, 'stop')}
              title="Stopp"
            >
              <Square size={13} />
            </button>
            <button
              className="btn-secondary-sm"
              onClick={() => blackTeaStore.controlMusicBot(selectedBot.bot_id, 'next')}
              title="Nächster Titel"
            >
              <SkipForward size={13} />
            </button>
          </div>
        )}
      </div>

      {caps.canManageMusicBots && (
        <div className="inspector-section">
          <h4>Bot-Lautstärke (Server &amp; Lokal)</h4>
          <div className="volume-control-box">
            <div className="volume-label-row">
              <span>Master-Lautstärke ({selectedBot.volume}%)</span>
              <button
                className={`btn-xs ${selectedClient.localMuted ? 'danger' : ''}`}
                onClick={() => blackTeaStore.togglePeerMute(selectedClient.clid)}
              >
                {selectedClient.localMuted ? (
                  <>
                    <VolumeX size={12} /> Stumm
                  </>
                ) : (
                  <>
                    <Volume2 size={12} /> Aktiv
                  </>
                )}
              </button>
            </div>
            <input
              type="range"
              min={0}
              max={100}
              value={selectedBot.volume}
              onChange={(e) =>
                blackTeaStore.controlMusicBot(
                  selectedBot.bot_id,
                  'volume',
                  Number(e.target.value)
                )
              }
            />
          </div>
        </div>
      )}

      <div className="inspector-section">
        <h4>MusicBot-Aktionen</h4>
        <div className="inspector-action-grid">
          <button
            className="btn-primary-sm full-span"
            onClick={() => blackTeaStore.patchState({ showMusicBotDeck: true })}
          >
            <Music size={13} /> Im großen Studio-Player öffnen
          </button>

          {caps.canManageMusicBots && selectedBot.cid !== state.currentChannelId && (
            <button
              className="btn-secondary-sm full-span"
              onClick={() =>
                blackTeaStore.controlMusicBot(
                  selectedBot.bot_id,
                  'summon',
                  state.currentChannelId
                )
              }
            >
              <MapPin size={13} /> In meinen Channel holen
            </button>
          )}

          {caps.canManageMusicBots && (
            <>
              <button
                className="btn-secondary-sm"
                onClick={() => {
                  const nextName = window.prompt(
                    'Neuer Name für den MusicBot:',
                    selectedBot.bot_name
                  );
                  if (nextName && nextName.trim()) {
                    void blackTeaStore.renameMusicBot(selectedBot.bot_id, nextName);
                  }
                }}
              >
                <Edit3 size={13} /> Umbenennen
              </button>
              <button
                className="btn-danger-sm"
                onClick={() => blackTeaStore.deleteMusicBot(selectedBot.bot_id)}
              >
                <Trash2 size={13} /> Bot löschen
              </button>
            </>
          )}
        </div>
      </div>
    </aside>
  );
};
