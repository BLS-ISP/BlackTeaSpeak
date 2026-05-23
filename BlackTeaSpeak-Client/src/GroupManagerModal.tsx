import { useState, useEffect } from 'react';
import { listen } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';
import { ServerGroup, ChannelGroup } from './types';
import { parseTs3Response, escapeTs3String } from './ts3parser';
import { PermissionEditor } from './PermissionEditorModal';

interface GroupManagerModalProps {
  onClose: () => void;
}

export function GroupManagerModal({ onClose }: GroupManagerModalProps) {
  const [activeTab, setActiveTab] = useState<'server' | 'channel'>('server');
  const [serverGroups, setServerGroups] = useState<ServerGroup[]>([]);
  const [channelGroups, setChannelGroups] = useState<ChannelGroup[]>([]);
  const [editingTarget, setEditingTarget] = useState<{type: 'servergroup'|'channelgroup', id: string} | null>(null);

  useEffect(() => {
    let unlisten: () => void;

    async function setup() {
      unlisten = await listen<string>('server_event', (event) => {
        const parsed = parseTs3Response(event.payload);
        for (const row of parsed) {
          if (row.command === 'servergrouplist') {
            setServerGroups(prev => {
              if (prev.find(g => g.sgid === row.args.sgid)) return prev;
              return [...prev, row.args as unknown as ServerGroup];
            });
          } else if (row.command === 'channelgrouplist') {
            setChannelGroups(prev => {
              if (prev.find(g => g.cgid === row.args.cgid)) return prev;
              return [...prev, row.args as unknown as ChannelGroup];
            });
          }
        }
      });

      refreshGroups();
    }
    setup();

    return () => {
      if (unlisten) unlisten();
    };
  }, []);

  const refreshGroups = () => {
    setServerGroups([]);
    setChannelGroups([]);
    invoke('send_command', { command: 'servergrouplist' }).catch(console.error);
    invoke('send_command', { command: 'channelgrouplist' }).catch(console.error);
  };

  const handleAddServerGroup = () => {
    const name = prompt("Enter Server Group Name:");
    if (name) {
      invoke('send_command', { command: `servergroupadd name=${escapeTs3String(name)} type=1` });
      setTimeout(refreshGroups, 500);
    }
  };

  const handleAddChannelGroup = () => {
    const name = prompt("Enter Channel Group Name:");
    if (name) {
      invoke('send_command', { command: `channelgroupadd name=${escapeTs3String(name)} type=1` });
      setTimeout(refreshGroups, 500);
    }
  };

  const handleDeleteServerGroup = (sgid: string) => {
    if (confirm("Are you sure you want to delete this Server Group?")) {
      invoke('send_command', { command: `servergroupdel sgid=${sgid} force=1` });
      setTimeout(refreshGroups, 500);
    }
  };

  const handleDeleteChannelGroup = (cgid: string) => {
    if (confirm("Are you sure you want to delete this Channel Group?")) {
      invoke('send_command', { command: `channelgroupdel cgid=${cgid} force=1` });
      setTimeout(refreshGroups, 500);
    }
  };

  const handleRenameServerGroup = (sgid: string, oldName: string) => {
    const name = prompt("Enter new Server Group Name:", oldName);
    if (name && name !== oldName) {
      invoke('send_command', { command: `servergrouprename sgid=${sgid} name=${escapeTs3String(name)}` });
      setTimeout(refreshGroups, 500);
    }
  };

  const handleRenameChannelGroup = (cgid: string, oldName: string) => {
    const name = prompt("Enter new Channel Group Name:", oldName);
    if (name && name !== oldName) {
      invoke('send_command', { command: `channelgrouprename cgid=${cgid} name=${escapeTs3String(name)}` });
      setTimeout(refreshGroups, 500);
    }
  };

  return (
    <div className="modal-overlay">
      <div className="modal-content" style={{ width: '1200px', maxWidth: '95vw', height: '85vh', padding: '0', display: 'flex', flexDirection: 'column' }}>
        
        <div style={{ padding: '24px 32px', borderBottom: '1px solid var(--glass-border)', display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
          <h2 style={{ margin: 0 }}>Server Overview - Groups & Permissions</h2>
          <button className="btn-icon" onClick={onClose} style={{ padding: '8px', fontSize: '18px' }}>❌</button>
        </div>

        <div style={{ display: 'flex', flexGrow: 1, overflow: 'hidden' }}>
          {/* Left Panel: Groups List */}
          <div style={{ width: '400px', borderRight: '1px solid var(--glass-border)', display: 'flex', flexDirection: 'column', padding: '24px' }}>
            <div className="tabs" style={{ display: 'flex', gap: '8px', marginBottom: '16px' }}>
              <button 
                className={activeTab === 'server' ? 'btn-primary' : 'btn-secondary'} 
                onClick={() => { setActiveTab('server'); setEditingTarget(null); }}
                style={{ flex: 1 }}
              >
                Server Groups
              </button>
              <button 
                className={activeTab === 'channel' ? 'btn-primary' : 'btn-secondary'} 
                onClick={() => { setActiveTab('channel'); setEditingTarget(null); }}
                style={{ flex: 1 }}
              >
                Channel Groups
              </button>
            </div>

            <div className="list-view" style={{ flexGrow: 1, overflowY: 'auto' }}>
              {activeTab === 'server' && (
                <>
                  {serverGroups.map(g => (
                    <div 
                      className="list-item" 
                      key={g.sgid} 
                      style={{ cursor: 'pointer', border: editingTarget?.id === g.sgid ? '1px solid var(--accent-color)' : '' }}
                      onClick={() => setEditingTarget({ type: 'servergroup', id: g.sgid })}
                    >
                      <div className="list-info">
                        <h4>{g.name}</h4>
                        <span className="mono-text">ID: {g.sgid} | Type: {g.type}</span>
                      </div>
                      <div className="card-actions">
                        <button className="btn-icon muted" onClick={(e) => { e.stopPropagation(); handleDeleteServerGroup(g.sgid); }} title="Delete">🗑️</button>
                      </div>
                    </div>
                  ))}
                  <button className="btn-secondary" style={{ marginTop: '16px' }} onClick={handleAddServerGroup}>+ Add Server Group</button>
                </>
              )}

              {activeTab === 'channel' && (
                <>
                  {channelGroups.map(g => (
                    <div 
                      className="list-item" 
                      key={g.cgid} 
                      style={{ cursor: 'pointer', border: editingTarget?.id === g.cgid ? '1px solid var(--accent-color)' : '' }}
                      onClick={() => setEditingTarget({ type: 'channelgroup', id: g.cgid })}
                    >
                      <div className="list-info">
                        <h4>{g.name}</h4>
                        <span className="mono-text">ID: {g.cgid} | Type: {g.type}</span>
                      </div>
                      <div className="card-actions">
                        <button className="btn-icon muted" onClick={(e) => { e.stopPropagation(); handleDeleteChannelGroup(g.cgid); }} title="Delete">🗑️</button>
                      </div>
                    </div>
                  ))}
                  <button className="btn-secondary" style={{ marginTop: '16px' }} onClick={handleAddChannelGroup}>+ Add Channel Group</button>
                </>
              )}
            </div>
          </div>

          {/* Right Panel: Permissions Editor */}
          <div style={{ flexGrow: 1, padding: '24px', display: 'flex', flexDirection: 'column', backgroundColor: 'rgba(0,0,0,0.2)' }}>
            {editingTarget ? (
              <PermissionEditor 
                targetType={editingTarget.type} 
                targetId={editingTarget.id} 
              />
            ) : (
              <div style={{ display: 'flex', flexGrow: 1, alignItems: 'center', justifyContent: 'center', color: 'var(--text-secondary)' }}>
                <p>Select a group on the left to edit its permissions.</p>
              </div>
            )}
          </div>
        </div>

      </div>
    </div>
  );
}
