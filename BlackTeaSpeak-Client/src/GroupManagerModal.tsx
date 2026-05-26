import { useState, useEffect } from 'react';

import { invoke } from '@tauri-apps/api/core';
import { ServerGroup, ChannelGroup } from './types';
import { escapeTs3String } from './ts3parser';
import { PermissionEditor } from './components/permissions/PermissionEditor';
import { Dialogs } from './ui/Dialogs';
import { Toast } from './ui/Toast';
import { eventBus } from './EventBus';
import { Trash as TrashIcon, X as XIcon } from 'lucide-react';


interface GroupManagerModalProps {
  onClose: () => void;
}

export function GroupManagerModal({ onClose }: GroupManagerModalProps) {
  const [activeTab, setActiveTab] = useState<'server' | 'channel'>('server');
  const [serverGroups, setServerGroups] = useState<ServerGroup[]>([]);
  const [channelGroups, setChannelGroups] = useState<ChannelGroup[]>([]);
  const [editingTarget, setEditingTarget] = useState<{type: 'servergroup'|'channelgroup', id: string} | null>(null);

  const [, setWaitingForRefresh] = useState(false);

  useEffect(() => {
    const unsubscribe = eventBus.subscribe((rows) => {
      for (const row of rows) {
        if (row.command === 'servergrouplist') {
          setServerGroups(prev => {
            if (prev.find(g => g.sgid === row.args.sgid)) return prev;
            return [...prev, row.args as any as ServerGroup];
          });
        } else if (row.command === 'channelgrouplist') {
          setChannelGroups(prev => {
            if (prev.find(g => g.cgid === row.args.cgid)) return prev;
            return [...prev, row.args as any as ChannelGroup];
          });
        } else if (row.command === 'error' && row.args.msg === 'ok') {
          setWaitingForRefresh(waiting => {
            if (waiting) {
              refreshGroups();
              return false;
            }
            return waiting;
          });
        }
      }
    });

    refreshGroups();

    return () => {
      unsubscribe();
    };
  }, []);

  const refreshGroups = () => {
    setServerGroups([]);
    setChannelGroups([]);
    invoke('send_command', { command: 'servergrouplist' }).catch(console.error);
    invoke('send_command', { command: 'channelgrouplist' }).catch(console.error);
  };

  const handleAddServerGroup = async () => {
    const name = await Dialogs.prompt("Add Server Group", "Enter Server Group Name:");
    if (name) {
      invoke('send_command', { command: `servergroupadd name=${escapeTs3String(name)} type=1` });
      setWaitingForRefresh(true);
      Toast.success("Server Group added");
    }
  };

  const handleAddChannelGroup = async () => {
    const name = await Dialogs.prompt("Add Channel Group", "Enter Channel Group Name:");
    if (name) {
      invoke('send_command', { command: `channelgroupadd name=${escapeTs3String(name)} type=1` });
      setWaitingForRefresh(true);
      Toast.success("Channel Group added");
    }
  };

  const handleDeleteServerGroup = async (sgid: string) => {
    if (await Dialogs.confirm("Delete Group", "Are you sure you want to delete this Server Group?")) {
      invoke('send_command', { command: `servergroupdel sgid=${sgid} force=1` });
      setWaitingForRefresh(true);
      Toast.success("Server Group deleted");
    }
  };

  const handleDeleteChannelGroup = async (cgid: string) => {
    if (await Dialogs.confirm("Delete Group", "Are you sure you want to delete this Channel Group?")) {
      invoke('send_command', { command: `channelgroupdel cgid=${cgid} force=1` });
      setWaitingForRefresh(true);
      Toast.success("Channel Group deleted");
    }
  };


  return (
    <div className="modal-overlay">
      <div className="modal-content" style={{ width: '1200px', maxWidth: '95vw', height: '85vh', padding: '0', display: 'flex', flexDirection: 'column' }}>
        
        <div style={{ padding: '24px 32px', borderBottom: '1px solid var(--glass-border)', display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
          <h2 style={{ margin: 0 }}>Server Overview - Groups & Permissions</h2>
          <button className="btn-icon" onClick={onClose} style={{ padding: '8px', fontSize: '18px' }}><XIcon /></button>
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
                        <button className="btn-icon muted" onClick={(e) => { e.stopPropagation(); handleDeleteServerGroup(g.sgid); }} title="Delete"><TrashIcon /></button>
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
                        <button className="btn-icon muted" onClick={(e) => { e.stopPropagation(); handleDeleteChannelGroup(g.cgid); }} title="Delete"><TrashIcon /></button>
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
