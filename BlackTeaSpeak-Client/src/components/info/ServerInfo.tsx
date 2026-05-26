import React, { useState, useEffect } from 'react';
import { eventBus } from '../../EventBus';
import { Dialogs } from '../../ui/Dialogs';
import { Trash as TrashIcon, Pause as PauseIcon } from 'lucide-react';


export function ServerInfo() {
  const [bots, setBots] = useState<any[]>([]);
  const [playlists, setPlaylists] = useState<any[]>([]);

  useEffect(() => {
    const unsubscribe = eventBus.subscribe((rows) => {
      for (const row of rows) {
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

    refreshBots();
    return () => { unsubscribe(); };
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
    Dialogs.confirm('Delete Music Bot', 'Delete Music Bot?').then((confirmed) => {
      if (confirmed) {
        import('@tauri-apps/api/core').then(({ invoke }) => {
          invoke('send_command', { command: `musicbotdelete bot_id=${botId}` });
          setTimeout(refreshBots, 500);
        });
      }
    });
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
              <button className="btn-icon" style={{ padding: '8px 12px' }} title="Play/Pause"><PauseIcon /></button>
              <button className="btn-icon muted" style={{ padding: '8px 12px' }} onClick={() => handleDeleteBot(b.bot_id)} title="Delete"><TrashIcon /></button>
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
