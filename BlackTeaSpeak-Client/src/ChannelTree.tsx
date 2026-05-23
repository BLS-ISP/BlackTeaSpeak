import { Channel, Client } from './types';

interface ChannelTreeProps {
  channels: Channel[];
  clients: Client[];
  myClientId: string;
  onChannelDoubleClick: (channel: Channel) => void;
  onClientClick: (client: Client) => void;
  onChannelClick: (channel: Channel) => void;
}

export function ChannelTree({ channels, clients, myClientId, onChannelDoubleClick, onClientClick, onChannelClick }: ChannelTreeProps) {
  
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
        >
          <span className="icon">📁</span>
          <span className="name">{channel.channel_name}</span>
        </div>
        
        {channelClients.map(client => (
          <div 
            key={client.clid} 
            className={`client-item ${client.clid === myClientId ? 'my-client' : ''}`}
            onClick={() => onClientClick(client)}
            style={{ marginLeft: '16px' }}
          >
            <span className={`status-lamp ${client.is_talking ? 'talking' : ''}`}></span>
            <span className="name">{client.client_nickname}</span>
            {client.client_input_muted && <span className="mute-icon" title="Microphone muted">🎤❌</span>}
            {client.client_output_muted && <span className="mute-icon" title="Speaker muted">🔊❌</span>}
          </div>
        ))}

        {subChannels.map(sub => renderChannel(sub, 1))}
      </div>
    );
  };

  const rootChannels = channels.filter(c => c.pid === "0" || !c.pid);

  return (
    <div className="channel-tree">
      {rootChannels.map(c => renderChannel(c, 0))}
    </div>
  );
}
