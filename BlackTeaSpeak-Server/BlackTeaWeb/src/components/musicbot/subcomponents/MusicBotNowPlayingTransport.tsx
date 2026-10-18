import React from 'react';
import {
  Disc,
  Edit3,
  ExternalLink,
  MapPin,
  Pause,
  Play,
  Repeat,
  Repeat1,
  RotateCcw,
  RotateCw,
  Shuffle,
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
import { MusicBotInfo } from '../../../types/protocol';
import {
  formatMs,
  getSourceLabel,
  resolveBotArtwork,
} from '../utils/musicBotFormatters';

export interface MusicBotNowPlayingTransportProps {
  activeBot: MusicBotInfo;
  state: BlackTeaState;
  caps: ClientCapabilities;
  posMs: number;
  dragSeekMs: number | null;
  setDragSeekMs: (v: number | null) => void;
  setSmoothPosMs: (v: number) => void;
  mutedMemoryVol: Record<number, number>;
  setMutedMemoryVol: React.Dispatch<React.SetStateAction<Record<number, number>>>;
}

export const MusicBotNowPlayingTransport: React.FC<
  MusicBotNowPlayingTransportProps
> = ({
  activeBot,
  state,
  caps,
  posMs,
  dragSeekMs,
  setDragSeekMs,
  setSmoothPosMs,
  mutedMemoryVol,
  setMutedMemoryVol,
}) => {
  const rawPlaying = activeBot.player_state === 1 || activeBot.player_state === 2;
  const isPaused = activeBot.player_state === 3;
  const isError = activeBot.song_title.startsWith('❌');
  const isLoading =
    rawPlaying &&
    (activeBot.song_title.startsWith('⏳') ||
      activeBot.song_title.startsWith('📥') ||
      activeBot.song_title.startsWith('🔍'));
  const isPlaying = rawPlaying && !isLoading && !isError;

  const badgeClass = isError
    ? 's-err'
    : isLoading
    ? 's-loading'
    : isPlaying
    ? 's-1'
    : isPaused
    ? 's-2'
    : 's-0';
  const badgeText = isError
    ? 'FEHLER'
    : isLoading
    ? 'LÄDT STREAM...'
    : isPlaying
    ? 'PLAYING'
    : isPaused
    ? 'PAUSED'
    : 'BEREIT';

  const durMs = Math.max(0, activeBot.duration_ms || 0);
  const remMs = durMs > posMs ? durMs - posMs : 0;
  const progressPct =
    durMs > 0 ? Math.min(100, Math.max(0, (posMs / durMs) * 100)) : isPlaying ? 100 : 0;

  const queueItems = activeBot.queue ?? [];
  const currentQueueIndex = queueItems.findIndex((q) => q.is_current);
  const artworkUrl = resolveBotArtwork(activeBot);
  const replayMode = activeBot.replay_mode ?? 0;
  const botChannelName =
    state.channels.find((ch) => ch.cid === activeBot.cid)?.channel_name ??
    `Channel #${activeBot.cid}`;

  return (
    <div className="studio-hero-card">
      {/* Artwork / Visualizer */}
      <div className="studio-artwork-box">
        {artworkUrl ? (
          <img
            src={artworkUrl}
            alt={activeBot.song_title}
            className="studio-artwork-img"
          />
        ) : (
          <div className={`studio-vinyl-fallback ${isPlaying ? 'playing' : ''}`}>
            <Disc size={44} className={isPlaying ? 'spin-disc' : ''} />
            <div className={`eq-bars ${isPlaying ? 'active' : ''}`}>
              <span />
              <span />
              <span />
              <span />
              <span />
            </div>
          </div>
        )}
        <span className="studio-source-pill">
          {getSourceLabel(activeBot.song_url)}
        </span>
      </div>

      {/* Center Track Info + Scrubber + Transport */}
      <div className="studio-main-console">
        <div className="studio-track-header">
          <div className="studio-track-title-group">
            <div className="studio-badges-row">
              <span className={`bot-state-badge ${badgeClass}`}>{badgeText}</span>
              <span className="studio-channel-pill">
                <MapPin size={11} /> {botChannelName}
              </span>
              {currentQueueIndex >= 0 && (
                <span className="studio-queue-pos-pill">
                  Titel #{currentQueueIndex + 1} von {queueItems.length}
                </span>
              )}
              {activeBot.song_url &&
                (activeBot.song_url.startsWith('http://') ||
                  activeBot.song_url.startsWith('https://')) && (
                  <a
                    href={activeBot.song_url}
                    target="_blank"
                    rel="noreferrer"
                    className="studio-ext-link"
                    title="Original-Link im Browser öffnen"
                  >
                    <ExternalLink size={11} /> Quelle öffnen
                  </a>
                )}
            </div>

            <h4 className="studio-song-title" title={activeBot.song_title}>
              {activeBot.song_title}
            </h4>
          </div>

          {caps.canManageMusicBots && (
            <div className="studio-bot-quick-actions">
              {activeBot.cid !== state.currentChannelId && (
                <button
                  className="btn-secondary-xs accent-border"
                  onClick={() =>
                    blackTeaStore.controlMusicBot(
                      activeBot.bot_id,
                      'summon',
                      state.currentChannelId
                    )
                  }
                  title="MusicBot in deinen aktuellen Channel holen"
                >
                  <MapPin size={12} /> In meinen Channel holen
                </button>
              )}
              <button
                className="btn-icon-xs"
                onClick={() => {
                  const nextName = window.prompt(
                    'Neuer Name für den MusicBot:',
                    activeBot.bot_name
                  );
                  if (nextName && nextName.trim()) {
                    void blackTeaStore.renameMusicBot(activeBot.bot_id, nextName);
                  }
                }}
                title="MusicBot umbenennen"
              >
                <Edit3 size={13} />
              </button>
              <button
                className="btn-icon-xs danger"
                onClick={() => blackTeaStore.deleteMusicBot(activeBot.bot_id)}
                title="MusicBot vom Server entfernen"
              >
                <Trash2 size={13} />
              </button>
            </div>
          )}
        </div>

        {/* Interactive Scrubber / Seek Bar */}
        <div className="studio-scrubber-box">
          <div className="studio-scrubber-track-wrap">
            <div className="bot-progress-track studio-progress-track">
              <div
                className={`bot-progress-fill ${isLoading ? 'indeterminate' : ''}`}
                style={isLoading ? undefined : { width: `${progressPct}%` }}
              />
            </div>
            {caps.canManageMusicBots && durMs > 0 && !isLoading && (
              <input
                type="range"
                className="studio-seek-input"
                min={0}
                max={durMs}
                step={1000}
                value={Math.min(durMs, posMs)}
                onChange={(e) => setDragSeekMs(Number(e.target.value))}
                onMouseUp={() => {
                  if (dragSeekMs !== null) {
                    const target = dragSeekMs;
                    setDragSeekMs(null);
                    setSmoothPosMs(target);
                    void blackTeaStore.controlMusicBot(
                      activeBot.bot_id,
                      'seek',
                      target
                    );
                  }
                }}
                onTouchEnd={() => {
                  if (dragSeekMs !== null) {
                    const target = dragSeekMs;
                    setDragSeekMs(null);
                    setSmoothPosMs(target);
                    void blackTeaStore.controlMusicBot(
                      activeBot.bot_id,
                      'seek',
                      target
                    );
                  }
                }}
                title="Klicken oder ziehen, um im Titel zu spulen"
              />
            )}
          </div>

          <div className="studio-scrubber-meta">
            {isLoading ? (
              <>
                <span className="loading-hint-text">
                  ⏳ Extrahiere &amp; puffere Audio-Stream...
                </span>
                <span>Startet automatisch in ca. 1–3 Sek.</span>
              </>
            ) : durMs > 0 ? (
              <>
                <span>
                  {dragSeekMs !== null
                    ? `Springe zu ${formatMs(dragSeekMs)}`
                    : formatMs(posMs)}
                </span>
                <span>-{formatMs(remMs)}</span>
                <span>{formatMs(durMs)}</span>
              </>
            ) : (
              <>
                <span>Laufzeit: {formatMs(posMs)}</span>
                <span>{rawPlaying ? 'LIVE-STREAM / ENDLOS' : '00:00'}</span>
              </>
            )}
          </div>
        </div>

        {/* Pro Transport & Volume Bar */}
        {caps.canManageMusicBots && (
          <div className="studio-transport-row">
            <div className="studio-transport-left">
              <button
                className="btn-icon-sm"
                onClick={() =>
                  blackTeaStore.controlMusicBot(activeBot.bot_id, 'shuffle')
                }
                disabled={queueItems.length < 2}
                title="Warteschlange mischen (Shuffle)"
              >
                <Shuffle size={14} />
              </button>

              <button
                className="btn-icon-sm"
                onClick={() => blackTeaStore.controlMusicBot(activeBot.bot_id, 'prev')}
                title="Vorheriger Titel"
              >
                <SkipBack size={15} />
              </button>

              <button
                className="btn-icon-sm"
                disabled={durMs === 0}
                onClick={() =>
                  blackTeaStore.controlMusicBot(
                    activeBot.bot_id,
                    'seek',
                    Math.max(0, posMs - 10000)
                  )
                }
                title="10 Sekunden zurückspulen"
              >
                <RotateCcw size={14} />
              </button>

              <button
                className={`studio-play-btn ${rawPlaying ? 'playing' : ''}`}
                onClick={() =>
                  blackTeaStore.controlMusicBot(
                    activeBot.bot_id,
                    rawPlaying ? 'pause' : 'play'
                  )
                }
                title={rawPlaying ? 'Pausieren' : 'Wiedergabe starten / fortsetzen'}
              >
                {rawPlaying ? <Pause size={17} /> : <Play size={17} />}
                <span>{rawPlaying ? 'Pause' : 'Play'}</span>
              </button>

              <button
                className="btn-icon-sm"
                onClick={() => blackTeaStore.controlMusicBot(activeBot.bot_id, 'stop')}
                title="Stopp"
              >
                <Square size={14} />
              </button>

              <button
                className="btn-icon-sm"
                disabled={durMs === 0}
                onClick={() =>
                  blackTeaStore.controlMusicBot(
                    activeBot.bot_id,
                    'seek',
                    Math.min(durMs, posMs + 10000)
                  )
                }
                title="10 Sekunden vorspulen"
              >
                <RotateCw size={14} />
              </button>

              <button
                className="btn-icon-sm"
                onClick={() => blackTeaStore.controlMusicBot(activeBot.bot_id, 'next')}
                title="Nächster Titel (Skip)"
              >
                <SkipForward size={15} />
              </button>

              <button
                className={`btn-icon-sm ${replayMode > 0 ? 'active-mode' : ''}`}
                onClick={() =>
                  blackTeaStore.controlMusicBot(
                    activeBot.bot_id,
                    'repeat',
                    (replayMode + 1) % 3
                  )
                }
                title={
                  replayMode === 2
                    ? 'Wiederholung: Aktuellen Titel wiederholen (Repeat 1)'
                    : replayMode === 1
                    ? 'Wiederholung: Gesamte Warteschlange wiederholen (Repeat All)'
                    : 'Wiederholung: Aus'
                }
              >
                {replayMode === 2 ? <Repeat1 size={15} /> : <Repeat size={15} />}
                {replayMode > 0 && (
                  <span className="mode-mini-label">
                    {replayMode === 2 ? '1' : 'ALL'}
                  </span>
                )}
              </button>
            </div>

            {/* Volume Console */}
            <div className="studio-volume-console">
              <button
                className="btn-icon-xs"
                onClick={() => {
                  if (activeBot.volume > 0) {
                    setMutedMemoryVol({
                      ...mutedMemoryVol,
                      [activeBot.bot_id]: activeBot.volume,
                    });
                    void blackTeaStore.controlMusicBot(activeBot.bot_id, 'volume', 0);
                  } else {
                    const restore = mutedMemoryVol[activeBot.bot_id] || 75;
                    void blackTeaStore.controlMusicBot(
                      activeBot.bot_id,
                      'volume',
                      restore
                    );
                  }
                }}
                title={activeBot.volume === 0 ? 'Ton aktivieren' : 'Stummschalten'}
              >
                {activeBot.volume === 0 ? (
                  <VolumeX size={14} className="muted-icon" />
                ) : (
                  <Volume2 size={14} />
                )}
              </button>
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
              <span className="studio-vol-pct">{activeBot.volume}%</span>
              <div className="studio-vol-presets">
                {[25, 50, 100].map((v) => (
                  <button
                    key={v}
                    className={`vol-preset-chip ${activeBot.volume === v ? 'active' : ''}`}
                    onClick={() =>
                      blackTeaStore.controlMusicBot(activeBot.bot_id, 'volume', v)
                    }
                  >
                    {v}%
                  </button>
                ))}
              </div>
            </div>
          </div>
        )}
      </div>
    </div>
  );
};
