import React from 'react';
import {
  ChevronDown,
  Disc,
  MapPin,
  Pause,
  Play,
  SkipBack,
  SkipForward,
  Volume2,
} from 'lucide-react';
import {
  blackTeaStore,
  BlackTeaState,
  ClientCapabilities,
} from '../../../store/useBlackTeaStore';
import { MusicBotInfo } from '../../../types/protocol';
import { formatMs, resolveBotArtwork } from '../utils/musicBotFormatters';

export interface MusicBotMiniBarProps {
  activeBot: MusicBotInfo;
  state: BlackTeaState;
  caps: ClientCapabilities;
  posMs: number;
}

export const MusicBotMiniBar: React.FC<MusicBotMiniBarProps> = ({
  activeBot,
  state,
  caps,
  posMs,
}) => {
  const rawPlaying = activeBot.player_state === 1 || activeBot.player_state === 2;
  const isPaused = activeBot.player_state === 3;
  const isLoading =
    rawPlaying &&
    (activeBot.song_title.startsWith('⏳') ||
      activeBot.song_title.startsWith('📥') ||
      activeBot.song_title.startsWith('🔍'));
  const isPlaying = rawPlaying && !isLoading && !activeBot.song_title.startsWith('❌');
  const durMs = Math.max(0, activeBot.duration_ms || 0);
  const pct = durMs > 0 ? Math.min(100, (posMs / durMs) * 100) : isPlaying ? 100 : 0;
  const chName =
    state.channels.find((c) => c.cid === activeBot.cid)?.channel_name ??
    `Channel #${activeBot.cid}`;
  const artwork = resolveBotArtwork(activeBot);

  return (
    <div className="musicbot-mini-bar">
      <div
        className="mini-bar-artwork"
        onClick={() => blackTeaStore.patchState({ showMusicBotDeck: true })}
        title="Studio-Player öffnen"
      >
        {artwork ? (
          <img src={artwork} alt="" />
        ) : (
          <Disc size={18} className={isPlaying ? 'spin-disc' : ''} />
        )}
      </div>

      <div
        className="mini-bar-info"
        onClick={() => blackTeaStore.patchState({ showMusicBotDeck: true })}
      >
        <div className="mini-bar-top-row">
          <span className="mini-bar-botname">{activeBot.bot_name}</span>
          <span className="mini-bar-channel">{chName}</span>
          {isLoading && <span className="mini-bar-badge loading">LÄDT...</span>}
          {isPlaying && <span className="mini-bar-badge playing">LIVE</span>}
          {isPaused && <span className="mini-bar-badge paused">PAUSIERT</span>}
        </div>
        <div className="mini-bar-title" title={activeBot.song_title}>
          {activeBot.song_title}
        </div>
        <div className="mini-bar-progress">
          <div
            className={`mini-bar-progress-fill ${isLoading ? 'indeterminate' : ''}`}
            style={isLoading ? undefined : { width: `${pct}%` }}
          />
        </div>
      </div>

      <div className="mini-bar-time">
        {durMs > 0 ? `${formatMs(posMs)} / ${formatMs(durMs)}` : formatMs(posMs)}
      </div>

      {caps.canManageMusicBots && (
        <div className="mini-bar-controls">
          <button
            className="btn-icon-xs"
            onClick={() => blackTeaStore.controlMusicBot(activeBot.bot_id, 'prev')}
            title="Vorheriger Titel"
          >
            <SkipBack size={13} />
          </button>
          <button
            className="studio-play-btn mini"
            onClick={() =>
              blackTeaStore.controlMusicBot(
                activeBot.bot_id,
                rawPlaying ? 'pause' : 'play'
              )
            }
            title={rawPlaying ? 'Pausieren' : 'Abspielen'}
          >
            {rawPlaying ? <Pause size={13} /> : <Play size={13} />}
          </button>
          <button
            className="btn-icon-xs"
            onClick={() => blackTeaStore.controlMusicBot(activeBot.bot_id, 'next')}
            title="Nächster Titel"
          >
            <SkipForward size={13} />
          </button>

          <div className="mini-bar-vol">
            <Volume2 size={13} />
            <input
              type="range"
              min={0}
              max={100}
              value={activeBot.volume}
              onChange={(e) =>
                blackTeaStore.controlMusicBot(
                  activeBot.bot_id,
                  'volume',
                  Number(e.target.value)
                )
              }
            />
            <span>{activeBot.volume}%</span>
          </div>

          {activeBot.cid !== state.currentChannelId && (
            <button
              className="btn-secondary-xs"
              onClick={() =>
                blackTeaStore.controlMusicBot(
                  activeBot.bot_id,
                  'summon',
                  state.currentChannelId
                )
              }
              title="MusicBot in meinen aktuellen Channel holen"
            >
              <MapPin size={11} /> Herholen
            </button>
          )}
        </div>
      )}

      <button
        className="btn-primary-xs"
        onClick={() => blackTeaStore.patchState({ showMusicBotDeck: true })}
        title="Großen Studio-Player & Warteschlange öffnen"
      >
        <ChevronDown size={13} /> Studio öffnen
      </button>
    </div>
  );
};
