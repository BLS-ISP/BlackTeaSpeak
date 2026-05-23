import { useState, useEffect, useRef } from 'react';
import { listen } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';
import { fetch as tauriFetch } from '@tauri-apps/plugin-http';
import { parseTs3Response } from './ts3parser';
import { Identity } from './App';
import { SettingsModal } from './SettingsModal';
import { GroupManagerModal } from './GroupManagerModal';
import { BanManagerModal } from './BanManagerModal';
import { TokenManagerModal } from './TokenManagerModal';
import { ChannelEditModal } from './ChannelEditModal';
import { PermissionEditorModal } from './PermissionEditorModal';
import { register, unregister, isRegistered } from '@tauri-apps/plugin-global-shortcut';

import { Channel, Client, ChatMessage, FileEntry } from './types';
import { ChannelTree } from './ChannelTree';
import { InfoPane } from './InfoPane';
import { ChatPane } from './ChatPane';
import { escapeTs3String } from './ts3parser';

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
  
  const [isMicMuted, setIsMicMuted] = useState(false);
  const [isSpeakerMuted, setIsSpeakerMuted] = useState(false);
  const [isSettingsOpen, setIsSettingsOpen] = useState(false);
  const [isGroupManagerOpen, setIsGroupManagerOpen] = useState(false);
  const [isBanManagerOpen, setIsBanManagerOpen] = useState(false);
  const [isTokenManagerOpen, setIsTokenManagerOpen] = useState(false);
  const [channelEditTarget, setChannelEditTarget] = useState<{cid?: string, cpid?: string} | null>(null);
  const [permissionTarget, setPermissionTarget] = useState<{type: 'server' | 'channel' | 'client' | 'group', targetId: string} | null>(null);

  const [channelFiles, setChannelFiles] = useState<FileEntry[]>([]);
  const pendingTransfers = useRef<Map<string, { type: 'upload' | 'download', file?: File, fileEntry?: FileEntry }>>(new Map());

  useEffect(() => {
    let unlisten: () => void;
    let unlistenDisconnect: () => void;

    async function setup() {
      unlisten = await listen<string>('server_event', (event) => {
        console.log('Raw event:', event.payload);
        const parsed = parseTs3Response(event.payload);
        
        for (const row of parsed) {
          if (row.command === 'initserver') {
            if (row.args.client_id) {
              setMyClientId(row.args.client_id);
            }
          } else if (row.args.client_id && row.command === 'unknown') {
            setMyClientId(row.args.client_id);
          } else if (row.command === 'channellist') {
            setChannels(prev => {
              const existing = prev.find(c => c.cid === row.args.cid);
              if (existing) return prev;
              return [...prev, row.args as unknown as Channel];
            });
          } else if (row.command === 'clientlist' || row.command === 'notifycliententerview') {
            setClients(prev => {
              const cid = row.args.cid || row.args.ctid;
              const clid = row.args.clid;
              const existing = prev.findIndex(c => c.clid === clid);
              const newClient = {
                clid,
                cid,
                client_nickname: row.args.client_nickname,
                client_type: row.args.client_type,
                client_input_muted: row.args.client_input_muted === '1',
                client_output_muted: row.args.client_output_muted === '1',
              } as Client;

              if (existing !== -1) {
                const newClients = [...prev];
                newClients[existing] = newClient;
                return newClients;
              }
              return [...prev, newClient];
            });
          } else if (row.command === 'notifyclientleftview') {
            setClients(prev => prev.filter(c => c.clid !== row.args.clid));
          } else if (row.command === 'notifyclientmoved') {
            setClients(prev => prev.map(c => 
              c.clid === row.args.clid ? { ...c, cid: row.args.ctid } : c
            ));
          } else if (row.command === 'notifyclientupdated') {
            setClients(prev => prev.map(c => {
              if (c.clid === row.args.clid) {
                return {
                  ...c,
                  client_nickname: row.args.client_nickname || c.client_nickname,
                  client_input_muted: row.args.client_input_muted !== undefined ? row.args.client_input_muted === '1' : c.client_input_muted,
                  client_output_muted: row.args.client_output_muted !== undefined ? row.args.client_output_muted === '1' : c.client_output_muted,
                  is_talking: row.args.client_is_talking !== undefined ? row.args.client_is_talking === '1' : c.is_talking,
                };
              }
              return c;
            }));
          } else if (row.command === 'notifytalkstatus') {
            const isTalking = row.args.status === '1';
            setClients(prev => prev.map(c => c.clid === row.args.clid ? { ...c, is_talking: isTalking } : c));
          } else if (row.command === 'notifytextmessage') {
            const newMsg: ChatMessage = {
              id: Math.random().toString(),
              timestamp: Date.now(),
              senderName: row.args.invokername || 'Unknown',
              senderId: row.args.invokerid,
              targetMode: parseInt(row.args.targetmode) || 3,
              message: row.args.msg || ''
            };
            setChatMessages(prev => [...prev, newMsg]);
          } else if ((row.command === 'unknown' || row.command === 'ftgetfilelist') && row.args.name && row.args.size && row.args.datetime && row.args.type) {
            setChannelFiles(prev => {
              const file: FileEntry = {
                name: row.args.name,
                size: parseInt(row.args.size),
                datetime: parseInt(row.args.datetime),
                type: parseInt(row.args.type),
                empty: row.args.empty === '1'
              };
              if (prev.find(f => f.name === file.name)) return prev;
              return [...prev, file];
            });
          } else if (row.args.ftkey && row.args.port && row.args.clientftfid) {
            const transfer = pendingTransfers.current.get(row.args.clientftfid);
            if (transfer) {
              pendingTransfers.current.delete(row.args.clientftfid);
              executeFileTransfer(transfer, row.args.ftkey, parseInt(row.args.port));
            }
          }
        }
      });

      unlistenDisconnect = await listen('server_disconnect', () => {
        onDisconnect();
      });

      invoke('send_command', { command: 'servernotifyregister event=server' }).catch(console.error);
      invoke('send_command', { command: 'servernotifyregister event=channel id=0' }).catch(console.error);
      invoke('send_command', { command: 'servernotifyregister event=textserver' }).catch(console.error);
      invoke('send_command', { command: 'servernotifyregister event=textchannel' }).catch(console.error);
      invoke('send_command', { command: 'servernotifyregister event=textprivate' }).catch(console.error);
      invoke('send_command', { command: 'whoami' }).catch(console.error);
      invoke('send_command', { command: 'channellist' }).catch(console.error);
      invoke('send_command', { command: 'clientlist' }).catch(console.error);
    }

    setup();

    return () => {
      if (unlisten) unlisten();
      if (unlistenDisconnect) unlistenDisconnect();
    };
  }, []);

  const executeFileTransfer = async (transfer: any, ftkey: string, port: number) => {
    const baseUrl = `https://127.0.0.1:${port}`;
    try {
      if (transfer.type === 'upload' && transfer.file) {
        await tauriFetch(`${baseUrl}/upload?transfer-key=${ftkey}`, {
          method: 'POST',
          body: transfer.file,
          headers: { 'Content-Type': 'application/octet-stream' },
          danger: { acceptInvalidCerts: true }
        });
        refreshFiles();
      } else if (transfer.type === 'download' && transfer.fileEntry) {
        const resp = await tauriFetch(`${baseUrl}/download?transfer-key=${ftkey}`, {
          danger: { acceptInvalidCerts: true }
        });
        const blob = await resp.blob();
        const url = URL.createObjectURL(blob);
        const a = document.createElement('a');
        a.href = url;
        a.download = transfer.fileEntry.name;
        a.click();
        URL.revokeObjectURL(url);
      }
    } catch (e) {
      console.error("File transfer failed:", e);
      alert("File transfer failed: " + e);
    }
  };

  const handleUploadFile = (file: File) => {
    if (!selectedChannel) return;
    const clientftfid = Math.floor(Math.random() * 10000).toString();
    pendingTransfers.current.set(clientftfid, { type: 'upload', file });
    invoke('send_command', { command: `ftinitupload clientftfid=${clientftfid} name=\\/${escapeTs3String(file.name)} cid=${selectedChannel.cid} size=${file.size} overwrite=1 resume=0` });
  };

  const handleDownloadFile = (entry: FileEntry) => {
    if (!selectedChannel) return;
    const clientftfid = Math.floor(Math.random() * 10000).toString();
    pendingTransfers.current.set(clientftfid, { type: 'download', fileEntry: entry });
    invoke('send_command', { command: `ftinitdownload clientftfid=${clientftfid} name=\\/${escapeTs3String(entry.name)} cid=${selectedChannel.cid} cpw= seekpos=0 proto=0` });
  };

  const handleDeleteFile = (entry: FileEntry) => {
    if (!selectedChannel) return;
    if (confirm(`Delete file ${entry.name}?`)) {
      invoke('send_command', { command: `ftdeletefile cid=${selectedChannel.cid} cpw= name=\\/${escapeTs3String(entry.name)}` });
      setTimeout(refreshFiles, 500);
    }
  };

  const refreshFiles = () => {
    if (selectedChannel) {
      setChannelFiles([]);
      invoke('send_command', { command: `ftgetfilelist cid=${selectedChannel.cid} cpw= path=\\/` });
    }
  };

  const handleChannelSelect = (channel: Channel) => {
    setSelectedChannel(channel);
    setSelectedClient(undefined);
    setChannelFiles([]);
    invoke('send_command', { command: `ftgetfilelist cid=${channel.cid} cpw= path=\\/` });
  };

  useEffect(() => {
    let activeShortcut = '';
    let activeWhisperShortcut = '';

    async function setupHotkey() {
      if (identity.voice_transmission_mode !== 'push_to_talk' || !identity.ptt_hotkey) {
        return;
      }

      try {
        const shortcut = identity.ptt_hotkey;
        const registered = await isRegistered(shortcut);
        if (registered) {
          await unregister(shortcut);
        }
        
        await register(shortcut, async (event) => {
          if (event.state === 'Pressed') {
            await invoke('set_ptt_state', { pressed: true });
          } else if (event.state === 'Released') {
            await invoke('set_ptt_state', { pressed: false });
          }
        });
        activeShortcut = shortcut;
      } catch (err) {
        console.error("Failed to register global hotkey:", err);
      }
    }

    async function setupWhisperHotkey() {
      if (!identity.whisper_hotkey) return;
      try {
        const shortcut = identity.whisper_hotkey;
        const registered = await isRegistered(shortcut);
        if (registered) await unregister(shortcut);
        await register(shortcut, async (event) => {
          if (event.state === 'Pressed') {
            await invoke('set_whisper_state', { active: true });
          } else if (event.state === 'Released') {
            await invoke('set_whisper_state', { active: false });
          }
        });
        activeWhisperShortcut = shortcut;
        
        if (identity.whisper_targets) {
          const clientTargetStr = identity.whisper_targets.client_ids.length > 0 
            ? `target=client target_id=${identity.whisper_targets.client_ids.join(',')}`
            : '';
          const channelTargetStr = identity.whisper_targets.channel_ids.length > 0
            ? `target=channel target_id=${identity.whisper_targets.channel_ids.join(',')}`
            : '';
          if (clientTargetStr) {
            invoke('send_command', { command: `desktopwhisperset ${clientTargetStr}` }).catch(console.error);
          } else if (channelTargetStr) {
            invoke('send_command', { command: `desktopwhisperset ${channelTargetStr}` }).catch(console.error);
          }
        }
      } catch (err) {
        console.error("Failed to register whisper hotkey:", err);
      }
    }

    setupHotkey();
    setupWhisperHotkey();

    return () => {
      if (activeShortcut) {
        unregister(activeShortcut).catch(console.error);
      }
      if (activeWhisperShortcut) {
        unregister(activeWhisperShortcut).catch(console.error);
      }
    };
  }, [identity.voice_transmission_mode, identity.ptt_hotkey, identity.whisper_hotkey, identity.whisper_targets]);

  const handleDisconnect = async () => {
    try {
      await invoke("disconnect");
      onDisconnect();
    } catch (e) {
      console.error(e);
      onDisconnect();
    }
  };

  const handleToggleMic = async () => {
    try {
      const newMuted = !isMicMuted;
      await invoke("toggle_microphone", { muted: newMuted });
      await invoke('send_command', { command: `clientupdate client_input_muted=${newMuted ? 1 : 0}` });
      setIsMicMuted(newMuted);
    } catch (e) {
      console.error(e);
    }
  };

  const handleToggleSpeaker = async () => {
    try {
      const newMuted = !isSpeakerMuted;
      await invoke("toggle_speaker", { muted: newMuted });
      await invoke('send_command', { command: `clientupdate client_output_muted=${newMuted ? 1 : 0}` });
      setIsSpeakerMuted(newMuted);
    } catch (e) {
      console.error(e);
    }
  };

  const handleChannelDoubleClick = (channel: Channel) => {
    if (!myClientId) return;
    invoke('send_command', { command: `clientmove cid=${channel.cid} clid=${myClientId}` })
      .catch(console.error);
  };

  const handleContextMenuAction = (action: string, type: 'channel' | 'client' | 'server', target: any) => {
    if (type === 'server') {
      if (action === 'channel_create_root') {
        setChannelEditTarget({ cpid: '0' });
      } else if (action === 'manage_tokens') {
        setIsTokenManagerOpen(true);
      } else if (action === 'manage_bans') {
        setIsBanManagerOpen(true);
      } else if (action === 'manage_groups') {
        setIsGroupManagerOpen(true);
      } else if (action === 'permissions_server') {
        setPermissionTarget({ type: 'server', targetId: '0' });
      }
    } else if (type === 'channel') {
      const channel = target as Channel;
      if (action === 'channel_create') {
        setChannelEditTarget({ cpid: channel.cid });
      } else if (action === 'channel_edit') {
        setChannelEditTarget({ cid: channel.cid });
      } else if (action === 'channel_delete') {
        if (confirm(`Delete channel ${channel.channel_name}?`)) {
          invoke('send_command', { command: `channeldelete cid=${channel.cid} force=1` });
        }
      } else if (action === 'permissions_channel') {
        setPermissionTarget({ type: 'channel', targetId: channel.cid });
      }
    } else if (type === 'client') {
      const client = target as Client;
      if (action === 'client_kick_channel') {
        invoke('send_command', { command: `clientkick reasonid=4 clid=${client.clid}` });
      } else if (action === 'client_kick_server') {
        invoke('send_command', { command: `clientkick reasonid=5 clid=${client.clid}` });
      } else if (action === 'client_ban') {
        const time = prompt("Enter ban time in seconds (0 for permanent):", "0");
        const reason = prompt("Enter ban reason:");
        if (time !== null) {
          invoke('send_command', { command: `banclient clid=${client.clid} time=${time} banreason=${escapeTs3String(reason || '')}` });
        }
      } else if (action === 'permissions_client') {
        setPermissionTarget({ type: 'client', targetId: client.clid });
      }
    }
  };

  const handleSendMessage = (targetMode: number, target: string, message: string) => {
    const escapedMsg = escapeTs3String(message);
    invoke('send_command', { command: `sendtextmessage targetmode=${targetMode} target=${target} msg=${escapedMsg}` })
      .catch(console.error);
      
    const newMsg: ChatMessage = {
      id: Math.random().toString(),
      timestamp: Date.now(),
      senderName: identity.name,
      senderId: myClientId,
      targetMode,
      message: message
    };
    setChatMessages(prev => [...prev, newMsg]);
  };

  const handleClientClick = (client: Client) => {
    setSelectedChannel(undefined);
    setSelectedClient(client);
  };

  return (
    <div className="connected-layout">
      <div className="main-area">
        <div className="content-area">
          <div className="tree-area">
            <h2>Server Channels</h2>
            <ChannelTree 
              channels={channels} 
              clients={clients} 
              myClientId={myClientId} 
              onChannelDoubleClick={handleChannelDoubleClick}
              onClientClick={handleClientClick}
              onChannelClick={handleChannelSelect}
              onContextMenuAction={handleContextMenuAction}
            />
            {channels.length === 0 && <p className="loading-text">Loading channels...</p>}
          </div>

          <div className="info-area">
            <InfoPane 
            selectedChannel={selectedChannel} 
            selectedClient={selectedClient} 
            channelFiles={channelFiles}
            onUploadFile={handleUploadFile}
            onDownloadFile={handleDownloadFile}
            onDeleteFile={handleDeleteFile}
            onRefreshFiles={refreshFiles}
          />
          </div>
        </div>

        <ChatPane 
          messages={chatMessages} 
          onSendMessage={handleSendMessage} 
          myClientId={myClientId}
          currentChannelId={clients.find(c => c.clid === myClientId)?.cid || '0'}
        />
      </div>

      <div className="control-bar">
        <button className={`btn-icon ${isMicMuted ? 'muted' : ''}`} onClick={handleToggleMic}>
          {isMicMuted ? '🔇 Mic Muted' : '🎙️ Mic Active'}
        </button>
        <button className={`btn-icon ${isSpeakerMuted ? 'muted' : ''}`} onClick={handleToggleSpeaker}>
          {isSpeakerMuted ? '🔈 Speaker Muted' : '🔊 Speaker Active'}
        </button>
        <button className="btn-icon" onClick={() => {
          const key = prompt("Enter Privilege Key to use:");
          if (key) invoke('send_command', { command: `tokenuse token=${key}` });
        }}>
          🔑 Use Token
        </button>
        <button className="btn-icon" onClick={() => setIsTokenManagerOpen(true)}>
          🛡️ Manage Tokens
        </button>
        <button className="btn-icon" onClick={() => setIsBanManagerOpen(true)}>
          🔨 Bans
        </button>
        <button className="btn-icon" onClick={() => setIsGroupManagerOpen(true)}>
          👥 Groups
        </button>
        <button className="btn-icon" onClick={() => setIsSettingsOpen(true)}>
          ⚙️ Settings
        </button>
        <button className="btn-danger" onClick={handleDisconnect}>
          Disconnect
        </button>
      </div>
      
      {isTokenManagerOpen && (
        <TokenManagerModal onClose={() => setIsTokenManagerOpen(false)} />
      )}

      {channelEditTarget && (
        <ChannelEditModal 
          cid={channelEditTarget.cid} 
          cpid={channelEditTarget.cpid} 
          onClose={() => setChannelEditTarget(null)} 
        />
      )}

      {isBanManagerOpen && (
        <BanManagerModal onClose={() => setIsBanManagerOpen(false)} />
      )}

      {isGroupManagerOpen && (
        <GroupManagerModal onClose={() => setIsGroupManagerOpen(false)} />
      )}

      {isSettingsOpen && (
        <SettingsModal 
          onClose={() => setIsSettingsOpen(false)} 
          identity={identity} 
          onIdentityUpdated={onIdentityUpdated} 
        />
      )}

      {permissionTarget && (
        <PermissionEditorModal 
          targetType={permissionTarget.type} 
          targetId={permissionTarget.targetId} 
          onClose={() => setPermissionTarget(null)} 
        />
      )}
    </div>
  );
}
