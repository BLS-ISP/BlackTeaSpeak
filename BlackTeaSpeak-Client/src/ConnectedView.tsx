import { useState, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Identity } from './App';
import { Channel, Client, ChatMessage, FileEntry } from './types';
import { ChannelTree } from './ChannelTree';
import { InfoPane } from './InfoPane';
import { ChatPane } from './ChatPane';

import { useGlobalShortcuts } from './hooks/useGlobalShortcuts';
import { useServerEvents } from './hooks/useServerEvents';
import { useFileTransfers } from './hooks/useFileTransfers';
import { useClientActions } from './hooks/useClientActions';
import { useContextMenuActions } from './hooks/useContextMenuActions';
import { usePermissions } from './hooks/usePermissions';

import { ControlBar } from './ControlBar';
import { ModalsContainer } from './ModalsContainer';

interface ConnectedViewProps {
  onDisconnect: () => void;
  identity: Identity;
  onIdentityUpdated: (identity: Identity) => void;
}

export default function ConnectedView({ onDisconnect, identity, onIdentityUpdated }: ConnectedViewProps) {
  const [channels, setChannels] = useState<Channel[]>([]);
  const [clients, setClients] = useState<Client[]>([]);
  const [myClientId, setMyClientId] = useState<string>('');
  const [selectedChannel, setSelectedChannel] = useState<Channel | undefined>(undefined);
  const [selectedClient, setSelectedClient] = useState<Client | undefined>(undefined);
  const [chatMessages, setChatMessages] = useState<ChatMessage[]>([]);
  const [channelFiles, setChannelFiles] = useState<FileEntry[]>([]);
  const [avatarCache, setAvatarCache] = useState<Record<string, string>>({});

  const {
    isMicMuted, isSpeakerMuted,
    handleDisconnect, handleToggleMic, handleToggleSpeaker, handleSendMessage
  } = useClientActions(myClientId, identity, setChatMessages, onDisconnect);

  const {
    handleContextMenuAction,
    isSettingsOpen, setIsSettingsOpen,
    isGroupManagerOpen, setIsGroupManagerOpen,
    isBanManagerOpen, setIsBanManagerOpen,
    isTokenManagerOpen, setIsTokenManagerOpen,
    channelEditTarget, setChannelEditTarget,
    permissionTarget, setPermissionTarget
  } = useContextMenuActions();

  const currentChannelId = clients.find(c => c.clid === myClientId)?.cid || '0';
  const { hasPermission } = usePermissions(currentChannelId);

  const {
    pendingTransfers,
    executeFileTransfer,
    handleUploadFile,
    handleDownloadFile,
    handleDeleteFile,
    handleUploadAvatar,
    fetchAvatar,
    refreshFiles
  } = useFileTransfers(selectedChannel?.cid, setChannelFiles, setAvatarCache);

  useServerEvents({
    onDisconnect: handleDisconnect,
    setChannels, setClients, setMyClientId,
    setChatMessages, setChannelFiles,
    pendingTransfers, executeFileTransfer,
    currentChannelId
  });

  useGlobalShortcuts(identity);

  const handleChannelSelect = (channel: Channel) => {
    setSelectedChannel(channel);
    setSelectedClient(undefined);
    setChannelFiles([]);
    invoke('send_command', { command: `ftgetfilelist cid=${channel.cid} cpw= path=\\/` });
  };

  const handleClientClick = (client: Client) => {
    setSelectedChannel(undefined);
    setSelectedClient(client);
  };

  const handleChannelDoubleClick = (channel: Channel) => {
    if (!myClientId) return;
    invoke('send_command', { command: `clientmove cid=${channel.cid} clid=${myClientId}` }).catch(console.error);
  };

  return (
    <div className="connected-layout">
      <div className="main-area">
        <div className="content-area">
          <div className="tree-area">
            <h2>Server Channels</h2>
            <ChannelTree 
              channels={channels} clients={clients} myClientId={myClientId} 
              onChannelDoubleClick={handleChannelDoubleClick}
              onClientClick={handleClientClick}
              onChannelClick={handleChannelSelect}
              onContextMenuAction={handleContextMenuAction}
              hasPermission={hasPermission}
            />
            {channels.length === 0 && <p className="loading-text">Loading channels...</p>}
          </div>

          <div className="info-area">
            <InfoPane 
              selectedChannel={selectedChannel} selectedClient={selectedClient} 
              channelFiles={channelFiles} avatarCache={avatarCache}
              onUploadFile={handleUploadFile} onDownloadFile={handleDownloadFile} onDeleteFile={handleDeleteFile}
              onRefreshFiles={() => refreshFiles()} onUploadAvatar={handleUploadAvatar} fetchAvatar={(hash) => fetchAvatar(hash, avatarCache)}
            />
          </div>
        </div>

        <ChatPane 
          messages={chatMessages} myClientId={myClientId}
          currentChannelId={clients.find(c => c.clid === myClientId)?.cid || '0'}
          currentClientId={selectedClient?.clid}
          onSendMessage={handleSendMessage} onUploadFile={handleUploadFile}
        />
      </div>

      <ControlBar 
        isMicMuted={isMicMuted} isSpeakerMuted={isSpeakerMuted}
        handleToggleMic={handleToggleMic} handleToggleSpeaker={handleToggleSpeaker} handleDisconnect={handleDisconnect}
        setIsTokenManagerOpen={setIsTokenManagerOpen} setIsBanManagerOpen={setIsBanManagerOpen}
        setIsGroupManagerOpen={setIsGroupManagerOpen} setIsSettingsOpen={setIsSettingsOpen}
        hasPermission={hasPermission}
      />
      
      <ModalsContainer 
        identity={identity} onIdentityUpdated={onIdentityUpdated}
        isTokenManagerOpen={isTokenManagerOpen} setIsTokenManagerOpen={setIsTokenManagerOpen}
        channelEditTarget={channelEditTarget} setChannelEditTarget={setChannelEditTarget}
        isBanManagerOpen={isBanManagerOpen} setIsBanManagerOpen={setIsBanManagerOpen}
        isGroupManagerOpen={isGroupManagerOpen} setIsGroupManagerOpen={setIsGroupManagerOpen}
        isSettingsOpen={isSettingsOpen} setIsSettingsOpen={setIsSettingsOpen}
        permissionTarget={permissionTarget} setPermissionTarget={setPermissionTarget}
      />
    </div>
  );
}
