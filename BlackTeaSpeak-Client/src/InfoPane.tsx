import { useState, useEffect } from 'react';
import { Channel, Client, FileEntry } from './types';

interface InfoPaneProps {
  selectedChannel?: Channel;
  selectedClient?: Client;
  channelFiles: FileEntry[];
  onUploadFile: (file: File) => void;
  onDownloadFile: (entry: FileEntry) => void;
  onDeleteFile: (entry: FileEntry) => void;
  onRefreshFiles: () => void;
}

export function InfoPane({ selectedChannel, selectedClient, channelFiles, onUploadFile, onDownloadFile, onDeleteFile, onRefreshFiles }: InfoPaneProps) {
  const [activeTab, setActiveTab] = useState<'details'|'files'>('details');
  if (selectedClient) {
    return (
      <div className="info-pane">
        <div className="info-header">
          <h2>{selectedClient.client_nickname}</h2>
          <p>Client ID: {selectedClient.clid}</p>
        </div>
        <div className="info-body">
          <div className="info-row">
            <span>Type:</span>
            <span>{selectedClient.client_type === '0' ? 'Normal Client' : 'Server Query'}</span>
          </div>
          {/* We will add more details here later when clientinfo response is parsed */}
        </div>
      </div>
    );
  }

  if (selectedChannel) {
    if (selectedChannel.cid === '0') {
      return (
        <div className="info-pane">
          <MusicBotsPanel />
        </div>
      );
    }

    return (
      <div className="info-pane">
        <div className="info-header">
          <h2>{selectedChannel.channel_name}</h2>
          <p>Channel ID: {selectedChannel.cid}</p>
        </div>
        
        <div className="info-tabs">
          <button className={`tab-btn ${activeTab === 'details' ? 'active' : ''}`} onClick={() => setActiveTab('details')}>Details</button>
          <button className={`tab-btn ${activeTab === 'files' ? 'active' : ''}`} onClick={() => { setActiveTab('files'); onRefreshFiles(); }}>Files</button>
        </div>

        <div className="info-body">
          {activeTab === 'details' && (
            <div className="info-row">
              <span>Topic:</span>
              <span>{selectedChannel.channel_topic || 'No topic set'}</span>
            </div>
          )}

          {activeTab === 'files' && (
            <div className="file-browser">
              <div className="file-actions">
                <input type="file" id="upload-input" style={{display: 'none'}} onChange={(e) => {
                  if (e.target.files && e.target.files[0]) {
                    onUploadFile(e.target.files[0]);
                    e.target.value = ''; // Reset
                  }
                }} />
                <button className="btn-secondary" onClick={() => document.getElementById('upload-input')?.click()}>Upload File</button>
                <button className="btn-secondary" onClick={onRefreshFiles}>Refresh</button>
              </div>
              
              <ul className="file-list">
                {channelFiles.map(file => (
                  <li key={file.name} className="file-item">
                    <span className="file-icon">{file.type === 0 ? '📁' : '📄'}</span>
                    <span className="file-name">{file.name}</span>
                    {file.type === 1 && <span className="file-size">{(file.size / 1024).toFixed(1)} KB</span>}
                    
                    {file.type === 1 && (
                      <div className="file-item-actions">
                        <button onClick={() => onDownloadFile(file)} title="Download">⬇️</button>
                        <button onClick={() => onDeleteFile(file)} title="Delete">🗑️</button>
                      </div>
                    )}
                  </li>
                ))}
                {channelFiles.length === 0 && <p>No files found.</p>}
              </ul>
            </div>
          )}
        </div>
      </div>
    );
  }

  return (
    <div className="info-pane">
      <MusicBotsPanel />
    </div>
  );
}

function MusicBotsPanel() {
  const [bots, setBots] = useState<any[]>([]);
  const [playlists, setPlaylists] = useState<any[]>([]);

  useEffect(() => {
    let unlisten: () => void;
    import('@tauri-apps/api/event').then(({ listen }) => {
      listen<string>('server_event', (event) => {
        import('./ts3parser').then(({ parseTs3Response }) => {
          const parsed = parseTs3Response(event.payload);
          for (const row of parsed) {
            if (row.command === 'musicbotlist') {
              setBots(prev => {
                if (prev.find(b => b.bot_id === row.args.bot_id)) return prev;
                return [...prev, row.args];
              });
            } else if (row.command === 'playlistlist') {
              setPlaylists(prev => {
                if (prev.find(p => p.id === row.args.id)) return prev;
                return [...prev, row.args];
              });
            }
          }
        });
      }).then(u => unlisten = u);
    });

    refreshBots();
    return () => { if (unlisten) unlisten(); };
  }, []);

  const refreshBots = () => {
    setBots([]);
    setPlaylists([]);
    import('@tauri-apps/api/core').then(({ invoke }) => {
      invoke('send_command', { command: 'musicbotlist' }).catch(console.error);
      invoke('send_command', { command: 'playlistlist' }).catch(console.error);
    });
  };

  const handleCreateBot = () => {
    import('@tauri-apps/api/core').then(({ invoke }) => {
      invoke('send_command', { command: 'musicbotcreate' });
      setTimeout(refreshBots, 500);
    });
  };

  const handleDeleteBot = (botId: string) => {
    if(confirm('Delete Music Bot?')) {
      import('@tauri-apps/api/core').then(({ invoke }) => {
        invoke('send_command', { command: `musicbotdelete bot_id=${botId}` });
        setTimeout(refreshBots, 500);
      });
    }
  };

  return (
    <div style={{ padding: '0 10px', height: '100%', display: 'flex', flexDirection: 'column' }}>
      <div className="info-header" style={{ marginBottom: '24px' }}>
        <h2>Server Overview</h2>
        <p>Select a channel or client to view details, or manage server resources below.</p>
      </div>

      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '16px' }}>
        <h3 style={{ margin: 0 }}>Music Bots</h3>
        <button className="btn-secondary" style={{ padding: '8px 16px', fontSize: '13px' }} onClick={handleCreateBot}>
          + Create Bot
        </button>
      </div>
      
      <div className="list-view" style={{ flexGrow: 1, overflowY: 'auto', marginBottom: '24px' }}>
        {bots.map((b, i) => (
          <div key={i} className="list-item">
            <div className="list-info">
              <h4>{b.name || `Bot ${b.bot_id}`}</h4>
              <p style={{ margin: 0, fontSize: '12px', color: 'var(--text-secondary)' }}>Status: Playing</p>
            </div>
            <div className="card-actions" style={{ marginTop: 0 }}>
              <button className="btn-icon" style={{ padding: '8px 12px' }} title="Play/Pause">⏸️</button>
              <button className="btn-icon muted" style={{ padding: '8px 12px' }} onClick={() => handleDeleteBot(b.bot_id)} title="Delete">🗑️</button>
            </div>
          </div>
        ))}
        {bots.length === 0 && <p className="empty-state">No music bots found on this server.</p>}
      </div>
      
      <h3 style={{ marginBottom: '16px' }}>Playlists</h3>
      <div className="list-view" style={{ flexGrow: 1, overflowY: 'auto' }}>
        {playlists.map((p, i) => (
          <div key={i} className="list-item">
            <div className="list-info">
              <h4>{p.name || `Playlist ${p.id}`}</h4>
            </div>
          </div>
        ))}
        {playlists.length === 0 && <p className="empty-state">No playlists available.</p>}
      </div>
    </div>
  );
}
