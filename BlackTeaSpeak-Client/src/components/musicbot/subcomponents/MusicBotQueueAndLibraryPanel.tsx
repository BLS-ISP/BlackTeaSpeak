import React, { useRef } from 'react';
import {
  ArrowDown,
  ArrowUp,
  FolderOpen,
  Link2,
  ListMusic,
  Play,
  Plus,
  Search,
  Shuffle,
  Trash2,
  Upload,
  X,
} from 'lucide-react';
import {
  blackTeaStore,
  ClientCapabilities,
} from '../../../store/useBlackTeaStore';
import { MusicBotInfo } from '../../../types/protocol';
import {
  formatBytes,
  formatMs,
  getSourceLabel,
} from '../utils/musicBotFormatters';

export interface MusicBotQueueAndLibraryPanelProps {
  activeBot: MusicBotInfo;
  caps: ClientCapabilities;
  streamUrl: string;
  setStreamUrl: (v: string) => void;
  handleLoadUrl: (playNow: boolean) => void;
  localSearch: string;
  setLocalSearch: (v: string) => void;
  isDragOverUpload: boolean;
  setIsDragOverUpload: (v: boolean) => void;
  handleFileUpload: (files: FileList | null, playImmediately?: boolean) => Promise<void>;
}

export const MusicBotQueueAndLibraryPanel: React.FC<
  MusicBotQueueAndLibraryPanelProps
> = ({
  activeBot,
  caps,
  streamUrl,
  setStreamUrl,
  handleLoadUrl,
  localSearch,
  setLocalSearch,
  isDragOverUpload,
  setIsDragOverUpload,
  handleFileUpload,
}) => {
  const fileInputRef = useRef<HTMLInputElement | null>(null);
  const queueItems = activeBot.queue ?? [];
  const localFiles = activeBot.local_files ?? [];
  const filteredLocalFiles = localFiles.filter(
    (f) =>
      !localSearch.trim() ||
      f.name.toLowerCase().includes(localSearch.trim().toLowerCase()) ||
      f.source.toLowerCase().includes(localSearch.trim().toLowerCase())
  );
  const totalQueueDurationSecs = queueItems.reduce(
    (acc, item) => acc + (item.duration_secs || 0),
    0
  );

  return (
    <>
      {/* URL / STREAM LOADER BAR */}
      {caps.canManageMusicBots && (
        <div className="studio-loader-bar">
          <div className="studio-loader-input-wrap">
            <Link2 size={14} className="loader-icon" />
            <input
              type="text"
              placeholder="YouTube-Link, Radio-Stream-URL (mp3/ogg/m3u8) oder lokalen Dateinamen eingeben..."
              value={streamUrl}
              onChange={(e) => setStreamUrl(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter') {
                  e.preventDefault();
                  handleLoadUrl(true);
                }
              }}
            />
            {streamUrl && (
              <button
                className="btn-icon-xs"
                onClick={() => setStreamUrl('')}
                title="Eingabe leeren"
              >
                <X size={12} />
              </button>
            )}
          </div>

          <button
            className="btn-primary-xs"
            onClick={() => handleLoadUrl(true)}
            disabled={!streamUrl.trim()}
            title="Sofort abspielen"
          >
            <Play size={12} /> Sofort abspielen
          </button>
          <button
            className="btn-secondary-xs"
            onClick={() => handleLoadUrl(false)}
            disabled={!streamUrl.trim()}
            title="Am Ende der Warteschlange einreihen"
          >
            <Plus size={12} /> In Queue
          </button>
        </div>
      )}

      {/* 2-COLUMN QUEUE & LOCAL LIBRARY */}
      <div className="bot-subpanels-grid">
        {/* LEFT: WARTESCHLANGE (QUEUE) */}
        <div className="bot-subpanel">
          <div className="bot-subpanel-header">
            <span className="bot-subpanel-title">
              <ListMusic size={13} />
              Warteschlange ({queueItems.length})
              {totalQueueDurationSecs > 0 && (
                <span className="bot-subpanel-hint">
                  · Gesamt {formatMs(totalQueueDurationSecs * 1000)}
                </span>
              )}
            </span>
            {caps.canManageMusicBots && queueItems.length > 0 && (
              <div className="bot-subpanel-actions">
                {queueItems.length > 1 && (
                  <button
                    className="btn-ghost-xs"
                    onClick={() =>
                      blackTeaStore.controlMusicBot(activeBot.bot_id, 'shuffle')
                    }
                    title="Warteschlange zufällig mischen"
                  >
                    <Shuffle size={10} /> Mischen
                  </button>
                )}
                <button
                  className="btn-ghost-xs danger-hover"
                  onClick={() =>
                    blackTeaStore.controlMusicBot(activeBot.bot_id, 'clear')
                  }
                  title="Gesamte Warteschlange leeren"
                >
                  <Trash2 size={10} /> Leeren
                </button>
              </div>
            )}
          </div>

          {queueItems.length === 0 ? (
            <div className="bot-subpanel-empty">
              Die Warteschlange ist leer. Füge oben einen YouTube-/Stream-Link ein oder wähle rechts eine lokale Audiodatei aus.
            </div>
          ) : (
            <div className="bot-queue-list">
              {queueItems.map((item, idx) => (
                <div
                  key={item.song_id}
                  className={`bot-queue-item ${item.is_current ? 'active' : ''}`}
                  onDoubleClick={() => {
                    if (caps.canManageMusicBots) {
                      void blackTeaStore.controlMusicBot(
                        activeBot.bot_id,
                        'select',
                        item.song_id
                      );
                    }
                  }}
                >
                  <span className="bot-queue-index">
                    {item.is_current ? '▶' : `${idx + 1}.`}
                  </span>
                  <div className="bot-queue-info" title={item.url}>
                    <span className="bot-queue-title">{item.title}</span>
                    <span className="bot-queue-meta">
                      {getSourceLabel(item.url)} ·{' '}
                      {item.duration_secs > 0
                        ? formatMs(item.duration_secs * 1000)
                        : item.live_stream
                        ? 'LIVE'
                        : item.loader}
                    </span>
                  </div>
                  {caps.canManageMusicBots && (
                    <div className="bot-queue-actions">
                      {!item.is_current && (
                        <button
                          className="btn-icon-xs"
                          onClick={() =>
                            blackTeaStore.controlMusicBot(
                              activeBot.bot_id,
                              'select',
                              item.song_id
                            )
                          }
                          title="Diesen Titel jetzt abspielen"
                        >
                          <Play size={11} />
                        </button>
                      )}
                      <button
                        className="btn-icon-xs"
                        disabled={idx === 0}
                        onClick={() =>
                          blackTeaStore.controlMusicBot(
                            activeBot.bot_id,
                            'move_up',
                            item.song_id
                          )
                        }
                        title="In der Warteschlange nach oben schieben"
                      >
                        <ArrowUp size={11} />
                      </button>
                      <button
                        className="btn-icon-xs"
                        disabled={idx + 1 >= queueItems.length}
                        onClick={() =>
                          blackTeaStore.controlMusicBot(
                            activeBot.bot_id,
                            'move_down',
                            item.song_id
                          )
                        }
                        title="In der Warteschlange nach unten schieben"
                      >
                        <ArrowDown size={11} />
                      </button>
                      <button
                        className="btn-icon-xs danger"
                        onClick={() =>
                          blackTeaStore.controlMusicBot(
                            activeBot.bot_id,
                            'remove',
                            item.song_id
                          )
                        }
                        title="Aus der Warteschlange entfernen"
                      >
                        <X size={11} />
                      </button>
                    </div>
                  )}
                </div>
              ))}
            </div>
          )}
        </div>

        {/* RIGHT: LOKALE SERVER-AUDIODATEIEN + DRAG & DROP UPLOAD */}
        <div
          className={`bot-subpanel ${isDragOverUpload ? 'drag-over' : ''}`}
          onDragOver={(e) => {
            e.preventDefault();
            if (caps.canManageMusicBots) setIsDragOverUpload(true);
          }}
          onDragLeave={() => setIsDragOverUpload(false)}
          onDrop={(e) => {
            e.preventDefault();
            setIsDragOverUpload(false);
            if (caps.canManageMusicBots && e.dataTransfer.files?.length) {
              void handleFileUpload(e.dataTransfer.files, false);
            }
          }}
        >
          <div className="bot-subpanel-header">
            <span className="bot-subpanel-title">
              <FolderOpen size={13} />
              Lokale Server-Bibliothek ({localFiles.length})
            </span>

            {caps.canManageMusicBots && (
              <div className="bot-subpanel-actions">
                <input
                  ref={fileInputRef}
                  type="file"
                  accept=".mp3,.ogg,.opus,.wav,.flac,.m4a,.aac,.webm,.mp4,.mkv,.mov,.avi"
                  multiple
                  style={{ display: 'none' }}
                  onChange={(e) => {
                    void handleFileUpload(e.target.files, false);
                    e.target.value = '';
                  }}
                />
                <button
                  className="btn-secondary-xs"
                  onClick={() => fileInputRef.current?.click()}
                  title="Eigene Audio- oder Videodatei (.mp3, .mp4, .webm, .mkv, .mov, .ogg, .wav, .flac, .opus) auf den Server hochladen"
                >
                  <Upload size={11} /> Audio/Video hochladen
                </button>
              </div>
            )}
          </div>

          {localFiles.length > 2 && (
            <div className="local-files-search">
              <Search size={11} />
              <input
                type="text"
                placeholder="Lokale Audiodateien filtern..."
                value={localSearch}
                onChange={(e) => setLocalSearch(e.target.value)}
              />
              {localSearch && (
                <button
                  className="btn-icon-xs"
                  onClick={() => setLocalSearch('')}
                >
                  <X size={10} />
                </button>
              )}
            </div>
          )}

          {filteredLocalFiles.length === 0 ? (
            <div className="bot-subpanel-empty">
              {localFiles.length === 0
                ? 'Keine lokalen Audiodateien in data/music/ gefunden. Ziehe eine .mp3/.ogg/.wav Datei hierher oder klicke auf „Datei hochladen“.'
                : 'Keine Datei entspricht deinem Suchfilter.'}
            </div>
          ) : (
            <div className="bot-queue-list">
              {filteredLocalFiles.map((file) => {
                const isCurrentLocal = activeBot.song_url === file.url;
                return (
                  <div
                    key={file.url}
                    className={`bot-queue-item ${isCurrentLocal ? 'active' : ''}`}
                  >
                    <div
                      className="bot-queue-info"
                      title={`${file.name} (${file.source})`}
                    >
                      <span className="bot-queue-title">{file.name}</span>
                      <span className="bot-queue-meta">
                        {file.source} · {formatBytes(file.size)}
                      </span>
                    </div>
                    {caps.canManageMusicBots && (
                      <div className="bot-queue-actions">
                        <button
                          className="btn-primary-xs"
                          onClick={() =>
                            blackTeaStore.loadMusicBotSong(
                              activeBot.bot_id,
                              file.url,
                              true
                            )
                          }
                          title="Lokale Datei sofort abspielen"
                        >
                          <Play size={10} /> Play
                        </button>
                        <button
                          className="btn-secondary-xs"
                          onClick={() =>
                            blackTeaStore.loadMusicBotSong(
                              activeBot.bot_id,
                              file.url,
                              false
                            )
                          }
                          title="Lokale Datei in die Warteschlange legen"
                        >
                          + Queue
                        </button>
                      </div>
                    )}
                  </div>
                );
              })}
            </div>
          )}
        </div>
      </div>
    </>
  );
};
