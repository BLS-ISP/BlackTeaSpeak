import { useState, useEffect } from 'react';
import { Channel, Client } from './types';
import { Avatar } from './Avatar';
import { FileBrowserModal } from './FileBrowserModal';
import { PermissionEditorModal } from './PermissionEditorModal';

interface ChannelTreeProps {
  channels: Channel[];
  clients: Client[];
  myClientId: string;
  onChannelDoubleClick: (channel: Channel) => void;
  onClientClick: (client: Client) => void;
  onChannelClick: (channel: Channel) => void;
  onContextMenuAction: (action: string, type: 'channel' | 'client' | 'server', target: any, extra?: any) => void;
  hasPermission: (permName: string) => boolean;
}

export function ChannelTree({ channels, clients, myClientId, onChannelDoubleClick, onClientClick, onChannelClick, onContextMenuAction, hasPermission }: ChannelTreeProps) {
  const [contextMenu, setContextMenu] = useState<{ x: number, y: number, type: 'channel' | 'client' | 'server', target: any } | null>(null);
  const [editingPermissions, setEditingPermissions] = useState<string | null>(null);
  const [browsingFiles, setBrowsingFiles] = useState<string | null>(null);
  const [clientVolumes, setClientVolumes] = useState<Record<string, number>>({});

  useEffect(() => {
    const handleClick = () => setContextMenu(null);
    window.addEventListener('click', handleClick);
    return () => window.removeEventListener('click', handleClick);
  }, []);

  const handleContextMenu = (e: React.MouseEvent, type: 'channel' | 'client' | 'server', target: any) => {
    e.preventDefault();
    setContextMenu({ x: e.clientX, y: e.clientY, type, target });
  };
  
  // Helper to render a single channel and its clients + subchannels
  const renderChannel = (channel: Channel, depth: number) => {
    const channelClients = clients.filter(c => c.cid === channel.cid);
    const subChannels = channels.filter(c => c.pid === channel.cid);
    
    return (
      <div key={channel.cid} className="tree-node" style={{ marginLeft: `${depth * 16}px` }}>
        <div 
          className="channel-item" 
          onClick={() => onChannelClick(channel)}
          onDoubleClick={() => onChannelDoubleClick(channel)}
          onContextMenu={(e) => handleContextMenu(e, 'channel', channel)}
        >
          <span className="icon">📁</span>
          <span className="name">{channel.channel_name}</span>
        </div>
        
        {channelClients.map(client => (
          <div 
            key={client.clid} 
            className={`client-item ${client.clid === myClientId ? 'my-client' : ''}`}
            onClick={() => onClientClick(client)}
            onContextMenu={(e) => handleContextMenu(e, 'client', client)}
            style={{ marginLeft: '16px', display: 'flex', alignItems: 'center', gap: '6px' }}
          >
            <div style={{ position: 'relative' }}>
              <Avatar name={client.client_nickname} size={24} />
              <span 
                className={`status-lamp ${client.is_talking ? (client.whisper_type === 'send' || client.whisper_type === 'receive' ? 'whisper' : 'talking') : ''}`} 
                style={{ position: 'absolute', bottom: '-2px', right: '-2px', width: '10px', height: '10px', border: '2px solid #1e1e1e' }}
              ></span>
            </div>
            <span className="name" style={{ flex: 1, whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>
              {client.client_nickname}
            </span>
            {client.client_input_muted && <span className="mute-icon" title="Microphone muted">🎤❌</span>}
            {client.client_output_muted && <span className="mute-icon" title="Speaker muted">🔊❌</span>}
          </div>
        ))}

        {subChannels.map(sub => renderChannel(sub, depth + 1))}
      </div>
    );
  };

  const rootChannels = channels.filter(c => c.pid === "0" || !c.pid);

  return (
    <div className="channel-tree" style={{ position: 'relative' }}>
      <div 
        className="tree-node server-root"
        onClick={() => onChannelClick({ cid: '0', channel_name: 'Server Root' } as any)}
        onContextMenu={(e) => handleContextMenu(e, 'server' as any, null)}
      >
        <div className="channel-item server-header">
          <span className="icon">🌍</span>
          <span className="name">BlackTeaSpeak Server</span>
        </div>
      </div>
      
      {rootChannels.map(c => renderChannel(c, 0))}
      
      {editingPermissions && (
        <PermissionEditorModal
          targetType="channel"
          targetId={editingPermissions}
          onClose={() => setEditingPermissions(null)}
        />
      )}
      
      {browsingFiles && (
        <FileBrowserModal
          channelId={browsingFiles}
          onClose={() => setBrowsingFiles(null)}
        />
      )}
      
      {contextMenu && (
        <div 
          className="context-menu" 
          style={{ 
            position: 'fixed', 
            top: contextMenu.y, 
            left: contextMenu.x, 
            zIndex: 1000
          }}
        >
          {contextMenu.type === 'server' && (
            <>
              <div className="context-menu-item" onClick={() => onContextMenuAction('channel_create_root', 'server', contextMenu.target)}>Create Root Channel</div>
              <div className="context-menu-divider"></div>
              {hasPermission('b_virtualserver_token_list') && (
                <div className="context-menu-item" onClick={() => onContextMenuAction('manage_tokens', 'server', contextMenu.target)}>Manage Tokens</div>
              )}
              {hasPermission('b_client_ban_list') && (
                <div className="context-menu-item" onClick={() => onContextMenuAction('manage_bans', 'server', contextMenu.target)}>Manage Bans</div>
              )}
              {hasPermission('b_virtualserver_servergroup_list') && (
                <div className="context-menu-item" onClick={() => onContextMenuAction('manage_groups', 'server', contextMenu.target)}>Manage Groups</div>
              )}
              {hasPermission('b_virtualserver_servergroup_permission_list') && (
                <div className="context-menu-item" onClick={() => onContextMenuAction('permissions_server', 'server', contextMenu.target)}>Server Permissions</div>
              )}
            </>
          )}
          {contextMenu.type === 'channel' && (
            <>
              {hasPermission('b_channel_create_child') && (
                <div className="context-menu-item" onClick={() => onContextMenuAction('channel_create', 'channel', contextMenu.target)}>Create Sub-Channel</div>
              )}
              {hasPermission('b_channel_modify_name') && (
                <div className="context-menu-item" onClick={() => onContextMenuAction('channel_edit', 'channel', contextMenu.target)}>Edit Channel</div>
              )}
              {hasPermission('b_channel_delete_flag') && (
                <div className="context-menu-item" onClick={() => onContextMenuAction('channel_delete', 'channel', contextMenu.target)}>Delete Channel</div>
              )}
              <div className="context-menu-divider"></div>
              {hasPermission('b_virtualserver_channelgroup_permission_list') && (
                <div className="context-menu-item" onClick={() => { setEditingPermissions(contextMenu.target.cid); setContextMenu(null); }}>Edit Permissions</div>
              )}
              <div className="context-menu-item" onClick={() => { setBrowsingFiles(contextMenu.target.cid); setContextMenu(null); }}>Open File Browser</div>
            </>
          )}
          {contextMenu.type === 'client' && (
            <>
              <div className="context-menu-item" onClick={() => onClientClick(contextMenu.target)}>Open Chat</div>
              
              <div className="context-menu-item" style={{ padding: '8px 12px', display: 'flex', flexDirection: 'column', gap: '4px' }} onClick={e => e.stopPropagation()}>
                <label style={{ fontSize: '11px', color: '#888' }}>Volume</label>
                <input 
                  type="range" 
                  min="0.0" 
                  max="3.0" 
                  step="0.1" 
                  value={clientVolumes[contextMenu.target.clid] ?? 1.0}
                  onChange={(e) => {
                    const vol = parseFloat(e.target.value);
                    setClientVolumes(prev => ({ ...prev, [contextMenu.target.clid]: vol }));
                    invoke('set_client_volume', { clientId: parseInt(contextMenu.target.clid, 10), volume: vol }).catch(console.error);
                  }}
                  style={{ width: '100px' }}
                />
              </div>

              {hasPermission('b_client_kick_from_channel') && (
                <div className="context-menu-item" onClick={() => onContextMenuAction('client_kick_channel', 'client', contextMenu.target)}>Kick from Channel</div>
              )}
              {hasPermission('b_client_kick_from_server') && (
                <div className="context-menu-item" onClick={() => onContextMenuAction('client_kick_server', 'client', contextMenu.target)}>Kick from Server</div>
              )}
              {hasPermission('b_client_ban_create') && (
                <div className="context-menu-item" onClick={() => onContextMenuAction('client_ban', 'client', contextMenu.target)}>Ban Client</div>
              )}
              <div className="context-menu-divider"></div>
              {hasPermission('b_virtualserver_client_permission_list') && (
                <div className="context-menu-item" onClick={() => onContextMenuAction('permissions_client', 'client', contextMenu.target)}>Client Permissions</div>
              )}
            </>
          )}
        </div>
      )}
    </div>
  );
}
