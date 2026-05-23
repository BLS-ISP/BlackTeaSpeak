import { Channel, Client } from './types';

interface InfoPaneProps {
  selectedChannel?: Channel;
  selectedClient?: Client;
}

export function InfoPane({ selectedChannel, selectedClient }: InfoPaneProps) {
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
    return (
      <div className="info-pane">
        <div className="info-header">
          <h2>{selectedChannel.channel_name}</h2>
          <p>Channel ID: {selectedChannel.cid}</p>
        </div>
        <div className="info-body">
          <div className="info-row">
            <span>Topic:</span>
            <span>{selectedChannel.channel_topic || 'No topic set'}</span>
          </div>
          {/* Additional details like description can be shown here */}
        </div>
      </div>
    );
  }

  return (
    <div className="info-pane empty">
      <p>Select a channel or client to view information.</p>
    </div>
  );
}
