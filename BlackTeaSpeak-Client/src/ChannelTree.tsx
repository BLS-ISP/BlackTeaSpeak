import { useState, useEffect } from 'react';
import { Channel, Client } from './types';

interface ChannelTreeProps {
  channels: Channel[];
  clients: Client[];
  myClientId: string;
  onChannelDoubleClick: (channel: Channel) => void;
  onClientClick: (client: Client) => void;
  onChannelClick: (channel: Channel) => void;
  onContextMenuAction: (action: string, type: 'channel' | 'client' | 'server', target: any, extra?: any) => void;
}

export function ChannelTree({ channels, clients, myClientId, onChannelDoubleClick, onClientClick, onChannelClick, onContextMenuAction }: ChannelTreeProps) {
  const [contextMenu, setContextMenu] = useState<{ x: number, y: number, type: 'channel' | 'client' | 'server', target: any } | null>(null);

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
            style={{ marginLeft: '16px' }}
          >
            <span className={`status-lamp ${client.is_talking ? 'talking' : ''}`}></span>
            <span className="name">{client.client_nickname}</span>
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
              <div className="context-menu-item" onClick={() => onContextMenuAction('manage_tokens', 'server', contextMenu.target)}>Manage Tokens</div>
              <div className="context-menu-item" onClick={() => onContextMenuAction('manage_bans', 'server', contextMenu.target)}>Manage Bans</div>
              <div className="context-menu-item" onClick={() => onContextMenuAction('manage_groups', 'server', contextMenu.target)}>Manage Groups</div>
              <div className="context-menu-item" onClick={() => onContextMenuAction('permissions_server', 'server', contextMenu.target)}>Server Permissions</div>
            </>
          )}
          {contextMenu.type === 'channel' && (
            <>
              <div className="context-menu-item" onClick={() => onContextMenuAction('channel_create', 'channel', contextMenu.target)}>Create Sub-Channel</div>
              <div className="context-menu-item" onClick={() => onContextMenuAction('channel_edit', 'channel', contextMenu.target)}>Edit Channel</div>
              <div className="context-menu-item" onClick={() => onContextMenuAction('channel_delete', 'channel', contextMenu.target)}>Delete Channel</div>
              <div className="context-menu-divider"></div>
              <div className="context-menu-item" onClick={() => onContextMenuAction('permissions_channel', 'channel', contextMenu.target)}>Channel Permissions</div>
            </>
          )}
          {contextMenu.type === 'client' && (
            <>
              <div className="context-menu-item" onClick={() => onContextMenuAction('client_kick_channel', 'client', contextMenu.target)}>Kick from Channel</div>
              <div className="context-menu-item" onClick={() => onContextMenuAction('client_kick_server', 'client', contextMenu.target)}>Kick from Server</div>
              <div className="context-menu-item" onClick={() => onContextMenuAction('client_ban', 'client', contextMenu.target)}>Ban Client</div>
              <div className="context-menu-divider"></div>
              <div className="context-menu-item" onClick={() => onContextMenuAction('permissions_client', 'client', contextMenu.target)}>Client Permissions</div>
            </>
          )}
        </div>
      )}
    </div>
  );
}
