import React, { useEffect, useState } from 'react';
import { ChevronUp, Disc, Plus, Radio } from 'lucide-react';
import {
  blackTeaStore,
  getClientCapabilities,
  useBlackTeaStore,
} from '../../store/useBlackTeaStore';
import { MusicBotInfo } from '../../types/protocol';
import { MusicBotMiniBar } from './subcomponents/MusicBotMiniBar';
import { MusicBotNowPlayingTransport } from './subcomponents/MusicBotNowPlayingTransport';
import { MusicBotQueueAndLibraryPanel } from './subcomponents/MusicBotQueueAndLibraryPanel';

export const MusicBotDeck: React.FC = () => {
  const state = useBlackTeaStore();
  const caps = getClientCapabilities(state);

  const [selectedBotId, setSelectedBotId] = useState<number | null>(null);
  const [newBotName, setNewBotName] = useState('BlackTea RadioBot');
  const [showCreateForm, setShowCreateForm] = useState(false);
  const [streamUrl, setStreamUrl] = useState('');
  const [localSearch, setLocalSearch] = useState('');
  const [dragSeekMs, setDragSeekMs] = useState<number | null>(null);
  const [smoothPosMs, setSmoothPosMs] = useState<number>(0);
  const [mutedMemoryVol, setMutedMemoryVol] = useState<Record<number, number>>({});
  const [isDragOverUpload, setIsDragOverUpload] = useState(false);

  const activeBot: MusicBotInfo | null = React.useMemo(() => {
    if (state.musicBots.length === 0) return null;
    if (selectedBotId !== null) {
      const found = state.musicBots.find((b) => b.bot_id === selectedBotId);
      if (found) return found;
    }
    const inMyChannel = state.musicBots.find((b) => b.cid === state.currentChannelId);
    return inMyChannel ?? state.musicBots[0];
  }, [state.musicBots, selectedBotId, state.currentChannelId]);

  useEffect(() => {
    if (state.selectedClientId !== null) {
      const matchingBot = state.musicBots.find(
        (b) => b.client_id === state.selectedClientId
      );
      if (matchingBot) {
        setSelectedBotId(matchingBot.bot_id);
      }
    }
  }, [state.selectedClientId, state.musicBots]);

  useEffect(() => {
    if (state.connectionState !== 'connected') return;
    void blackTeaStore.refreshMusicBots();
    const intervalMs = state.showMusicBotDeck ? 1000 : 2000;
    const timer = window.setInterval(() => {
      void blackTeaStore.refreshMusicBots();
    }, intervalMs);
    return () => window.clearInterval(timer);
  }, [state.connectionState, state.showMusicBotDeck]);

  useEffect(() => {
    if (!activeBot) {
      setSmoothPosMs(0);
      return;
    }
    if (dragSeekMs === null) {
      setSmoothPosMs(Math.max(0, activeBot.position_ms || 0));
    }
  }, [activeBot?.bot_id, activeBot?.position_ms, activeBot?.song_url, dragSeekMs]);

  useEffect(() => {
    if (!activeBot) return;
    const rawPlaying = activeBot.player_state === 1 || activeBot.player_state === 2;
    const isLoading =
      rawPlaying &&
      (activeBot.song_title.startsWith('⏳') ||
        activeBot.song_title.startsWith('📥') ||
        activeBot.song_title.startsWith('🔍'));
    if (!rawPlaying || isLoading || dragSeekMs !== null) return;

    const timer = window.setInterval(() => {
      setSmoothPosMs((prev) => {
        const dur = activeBot.duration_ms || 0;
        const next = prev + 200;
        return dur > 0 ? Math.min(dur, next) : next;
      });
    }, 200);
    return () => window.clearInterval(timer);
  }, [
    activeBot?.bot_id,
    activeBot?.player_state,
    activeBot?.song_title,
    activeBot?.duration_ms,
    dragSeekMs,
  ]);

  const handleCreateBot = async (e: React.FormEvent) => {
    e.preventDefault();
    const clean = newBotName.trim() || 'BlackTea RadioBot';
    await blackTeaStore.createMusicBot(state.currentChannelId, clean);
    setShowCreateForm(false);
  };

  const handleLoadUrl = (playNow: boolean) => {
    if (!activeBot) return;
    const url = streamUrl.trim();
    if (!url) return;
    void blackTeaStore.loadMusicBotSong(activeBot.bot_id, url, playNow);
    setStreamUrl('');
  };

  const handleFileUpload = async (files: FileList | null, playImmediately = false) => {
    if (!files || files.length === 0) return;
    for (let i = 0; i < files.length; i++) {
      const file = files[i];
      await blackTeaStore.uploadLocalMusicFile(
        file,
        activeBot?.bot_id,
        playImmediately && i === 0
      );
    }
  };

  if (!state.showMusicBotDeck && state.musicBots.length === 0) {
    return null;
  }

  const posMs = dragSeekMs !== null ? dragSeekMs : smoothPosMs;

  if (!state.showMusicBotDeck && activeBot) {
    return (
      <MusicBotMiniBar
        activeBot={activeBot}
        state={state}
        caps={caps}
        posMs={posMs}
      />
    );
  }

  return (
    <section className="musicbot-deck studio-deck">
      <div className="studio-top-bar">
        <div className="studio-brand">
          <div className="studio-brand-icon">
            <Radio size={15} />
          </div>
          <span>MusicBot Studio &amp; Radio-Deck</span>
        </div>

        {state.musicBots.length > 0 && (
          <div className="studio-bot-tabs">
            {state.musicBots.map((b) => {
              const bPlaying = b.player_state === 1 || b.player_state === 2;
              const ch =
                state.channels.find((c) => c.cid === b.cid)?.channel_name ?? `#${b.cid}`;
              return (
                <button
                  key={b.bot_id}
                  className={`studio-bot-tab ${
                    activeBot?.bot_id === b.bot_id ? 'active' : ''
                  }`}
                  onClick={() => setSelectedBotId(b.bot_id)}
                >
                  <span className={`bot-tab-dot ${bPlaying ? 'playing' : ''}`} />
                  <span className="bot-tab-name">{b.bot_name}</span>
                  <span className="bot-tab-ch">{ch}</span>
                </button>
              );
            })}
          </div>
        )}

        <div className="studio-top-actions">
          {caps.canManageMusicBots && (
            <>
              {showCreateForm ? (
                <form className="deck-create-form" onSubmit={handleCreateBot}>
                  <input
                    type="text"
                    value={newBotName}
                    onChange={(e) => setNewBotName(e.target.value)}
                    placeholder="Bot-Name..."
                    autoFocus
                  />
                  <button type="submit" className="btn-primary-xs">
                    Erstellen
                  </button>
                  <button
                    type="button"
                    className="btn-ghost-xs"
                    onClick={() => setShowCreateForm(false)}
                  >
                    Abbrechen
                  </button>
                </form>
              ) : (
                <button
                  className="btn-primary-xs"
                  onClick={() => setShowCreateForm(true)}
                  disabled={state.connectionState !== 'connected'}
                  title="Neuen MusicBot im aktuellen Channel spawnen"
                >
                  <Plus size={12} /> Neuer Bot
                </button>
              )}
            </>
          )}

          <button
            className="btn-secondary-xs"
            onClick={() => blackTeaStore.patchState({ showMusicBotDeck: false })}
            title="Auf kompakte Player-Leiste minimieren"
          >
            <ChevronUp size={13} /> Minimieren
          </button>
        </div>
      </div>

      {!activeBot ? (
        <div className="studio-empty-state">
          <Disc size={32} className="studio-empty-icon" />
          <div>
            <strong>Noch kein MusicBot auf dem Server aktiv</strong>
            <p>
              {caps.canManageMusicBots
                ? 'Erstelle direkt einen MusicBot für deinen aktuellen Channel, um YouTube-Videos, Web-Radios oder lokale Server-Audiodateien in Stereo-Opus (48 kHz) abzuspielen.'
                : 'Aktuell ist kein MusicBot auf diesem Server aktiv.'}
            </p>
          </div>
          {caps.canManageMusicBots && (
            <button
              className="btn-primary-sm"
              onClick={() =>
                blackTeaStore.createMusicBot(
                  state.currentChannelId,
                  newBotName.trim() || 'MusicBot'
                )
              }
            >
              <Plus size={14} /> MusicBot in meinem Channel starten
            </button>
          )}
        </div>
      ) : (
        <div className="studio-body">
          <MusicBotNowPlayingTransport
            activeBot={activeBot}
            state={state}
            caps={caps}
            posMs={posMs}
            dragSeekMs={dragSeekMs}
            setDragSeekMs={setDragSeekMs}
            setSmoothPosMs={setSmoothPosMs}
            mutedMemoryVol={mutedMemoryVol}
            setMutedMemoryVol={setMutedMemoryVol}
          />

          <MusicBotQueueAndLibraryPanel
            activeBot={activeBot}
            caps={caps}
            streamUrl={streamUrl}
            setStreamUrl={setStreamUrl}
            handleLoadUrl={handleLoadUrl}
            localSearch={localSearch}
            setLocalSearch={setLocalSearch}
            isDragOverUpload={isDragOverUpload}
            setIsDragOverUpload={setIsDragOverUpload}
            handleFileUpload={handleFileUpload}
          />
        </div>
      )}
    </section>
  );
};
