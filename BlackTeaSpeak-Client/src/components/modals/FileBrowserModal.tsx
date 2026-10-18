import React, { useRef, useState } from 'react';
import {
  Edit3,
  FileText,
  Folder,
  FolderOpen,
  FolderPlus,
  RefreshCw,
  Trash2,
  Upload,
  X,
} from 'lucide-react';
import { blackTeaStore, useBlackTeaStore } from '../../store/useBlackTeaStore';

export const FileBrowserModal: React.FC<{ onClose: () => void }> = ({ onClose }) => {
  const state = useBlackTeaStore();
  const [targetCid, setTargetCid] = useState<number>(
    state.selectedChannelId ?? state.currentChannelId
  );
  const channel = state.channels.find((c) => c.cid === targetCid);
  const [newFolderName, setNewFolderName] = useState('');
  const uploadInputRef = useRef<HTMLInputElement | null>(null);

  const handleChannelChange = (nextCid: number) => {
    setTargetCid(nextCid);
    void blackTeaStore.fetchChannelFiles(nextCid, '/');
  };

  return (
    <div className="modal-card wide">
      <div className="modal-header">
        <div className="modal-title">
          <FolderOpen size={18} />
          <span>
            Channel-Dateibrowser — {channel?.channel_name ?? `Channel #${targetCid}`} (
            <code>{state.currentFilePath}</code>)
          </span>
        </div>
        <button className="btn-icon-sm" onClick={onClose}>
          <X size={16} />
        </button>
      </div>

      <div className="modal-body">
        <div className="inline-form" style={{ marginBottom: '10px' }}>
          <select
            value={targetCid}
            onChange={(e) => handleChannelChange(Number(e.target.value))}
          >
            {state.channels.map((ch) => (
              <option key={ch.cid} value={ch.cid}>
                Channel: {ch.channel_name} (#{ch.cid})
              </option>
            ))}
          </select>

          <input
            type="text"
            placeholder="Neuer Ordnername..."
            value={newFolderName}
            onChange={(e) => setNewFolderName(e.target.value)}
          />
          <button
            className="btn-primary-sm"
            onClick={() => {
              if (newFolderName.trim()) {
                void blackTeaStore.createChannelDirectory(
                  targetCid,
                  newFolderName.trim()
                );
                setNewFolderName('');
              }
            }}
          >
            <FolderPlus size={14} /> Ordner anlegen
          </button>

          <input
            ref={uploadInputRef}
            type="file"
            style={{ display: 'none' }}
            onChange={(e) => {
              const file = e.target.files?.[0];
              if (file) {
                void blackTeaStore.uploadLocalMusicFile(file);
                e.target.value = '';
              }
            }}
          />
          <button
            className="btn-secondary-sm"
            onClick={() => uploadInputRef.current?.click()}
            title="Audio-/Mediendatei auf den Server hochladen"
          >
            <Upload size={14} /> Datei-Upload
          </button>

          <button
            className="btn-secondary-sm"
            onClick={() =>
              void blackTeaStore.fetchChannelFiles(targetCid, state.currentFilePath)
            }
          >
            <RefreshCw size={14} /> Aktualisieren
          </button>
        </div>

        <div className="list-box">
          {state.currentFilePath !== '/' && (
            <div
              className="list-item-row clickable"
              onClick={() => {
                const parts = state.currentFilePath
                  .split('/')
                  .filter((p) => p.length > 0);
                parts.pop();
                const parentPath = parts.length === 0 ? '/' : `/${parts.join('/')}`;
                void blackTeaStore.fetchChannelFiles(targetCid, parentPath);
              }}
            >
              <div>
                <Folder size={14} /> <strong>.. (Übergeordnetes Verzeichnis)</strong>
              </div>
            </div>
          )}

          {state.files.length === 0 ? (
            <div className="muted-note">Keine Dateien in diesem Channel-Verzeichnis.</div>
          ) : (
            state.files.map((f) => (
              <div
                key={f.name}
                className="list-item-row clickable"
                onClick={() => {
                  if (f.type === 0) {
                    const nextPath = `${state.currentFilePath.replace(/\/$/, '')}/${f.name}`;
                    void blackTeaStore.fetchChannelFiles(targetCid, nextPath);
                  }
                }}
              >
                <div className="file-row-left">
                  {f.type === 0 ? <Folder size={15} /> : <FileText size={15} />}
                  <strong>{f.name}</strong>
                  {f.type === 1 && (
                    <span className="sub-id">{(f.size / 1024).toFixed(1)} KB</span>
                  )}
                </div>
                <div
                  style={{ display: 'flex', gap: '6px' }}
                  onClick={(e) => e.stopPropagation()}
                >
                  <button
                    className="btn-icon-xs"
                    onClick={() => {
                      const nextName = window.prompt(
                        `Neuer Name für '${f.name}':`,
                        f.name
                      );
                      if (nextName && nextName.trim() && nextName.trim() !== f.name) {
                        void blackTeaStore.renameChannelFile(
                          targetCid,
                          f.name,
                          nextName.trim()
                        );
                      }
                    }}
                    title="Umbenennen"
                  >
                    <Edit3 size={13} />
                  </button>
                  <button
                    className="btn-icon-xs danger"
                    onClick={() => {
                      if (window.confirm(`'${f.name}' wirklich löschen?`)) {
                        void blackTeaStore.deleteChannelFile(targetCid, f.name);
                      }
                    }}
                    title="Löschen"
                  >
                    <Trash2 size={13} />
                  </button>
                </div>
              </div>
            ))
          )}
        </div>
      </div>
    </div>
  );
};
