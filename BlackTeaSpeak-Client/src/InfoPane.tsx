import { Channel, Client, FileEntry } from './types';
import { ServerInfo } from './components/info/ServerInfo';
import { ChannelInfo } from './components/info/ChannelInfo';
import { ClientInfo } from './components/info/ClientInfo';

interface InfoPaneProps {
  selectedChannel?: Channel;
  selectedClient?: Client;
  myClientId?: string;
  channelFiles: FileEntry[];
  onUploadFile: (file: File) => void;
  onDownloadFile: (entry: FileEntry) => void;
  onDeleteFile: (entry: FileEntry) => void;
  onRefreshFiles: () => void;
  avatarCache?: Record<string, string>;
  onUploadAvatar?: (file: File) => void;
  fetchAvatar?: (hash: string) => void;
}

export function InfoPane({ selectedChannel, selectedClient, myClientId, channelFiles, onUploadFile, onDownloadFile, onDeleteFile, onRefreshFiles, avatarCache, onUploadAvatar, fetchAvatar }: InfoPaneProps) {
  if (selectedClient) {
    return (
      <ClientInfo 
        selectedClient={selectedClient} 
        myClientId={myClientId} 
        avatarCache={avatarCache} 
        onUploadAvatar={onUploadAvatar} 
        fetchAvatar={fetchAvatar} 
      />
    );
  }

  if (selectedChannel) {
    if (selectedChannel.cid === '0') {
      return (
        <div className="info-pane">
          <ServerInfo />
        </div>
      );
    }

    return (
      <ChannelInfo 
        selectedChannel={selectedChannel} 
        channelFiles={channelFiles} 
        onUploadFile={onUploadFile} 
        onDownloadFile={onDownloadFile} 
        onDeleteFile={onDeleteFile} 
        onRefreshFiles={onRefreshFiles} 
      />
    );
  }

  return (
    <div className="info-pane">
      <ServerInfo />
    </div>
  );
}
