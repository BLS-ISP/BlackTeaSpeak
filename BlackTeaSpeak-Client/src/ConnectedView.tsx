import { useState, useEffect } from 'react';
import { listen } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';
import { parseTs3Response } from './ts3parser';
import { Identity } from './App';
import { SettingsModal } from './SettingsModal';
import { register, unregister, isRegistered } from '@tauri-apps/plugin-global-shortcut';

import { Channel, Client, ChatMessage } from './types';
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

  useEffect(() => {
    let unlisten: () => void;
    let unlistenDisconnect: () => void;

    async function setup() {
      unlisten = await listen<string>('server_event', (event) => {
        console.log('Raw event:', event.payload);
        const parsed = parseTs3Response(event.payload);
        console.log('Parsed:', parsed);
        
        for (const row of parsed) {
          if (row.command === 'initserver') {
            if (row.args.client_id) {
              setMyClientId(row.args.client_id);
            }
          } else if (row.args.client_id && row.command === 'unknown') {
            // "whoami" returns a raw row with client_id
            setMyClientId(row.args.client_id);
          } else if (row.command === 'channellist') {
            console.log('Adding channel:', row.args);
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
          }
        }
      });

      unlistenDisconnect = await listen('server_disconnect', () => {
        console.log('Server disconnected');
        onDisconnect();
      });

      // Now that listeners are registered, send commands!
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

  useEffect(() => {
    let activeShortcut = '';

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
        console.log(`Registered PTT hotkey: ${shortcut}`);
      } catch (err) {
        console.error("Failed to register global hotkey:", err);
      }
    }

    setupHotkey();

    return () => {
      if (activeShortcut) {
        unregister(activeShortcut).catch(console.error);
      }
    };
  }, [identity.voice_transmission_mode, identity.ptt_hotkey]);

  const handleDisconnect = async () => {
    try {
      await invoke("disconnect");
      onDisconnect();
    } catch (e) {
      console.error(e);
      onDisconnect(); // Disconnect UI anyway
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

  const handleSendMessage = (targetMode: number, target: string, message: string) => {
    const escapedMsg = escapeTs3String(message);
    invoke('send_command', { command: `sendtextmessage targetmode=${targetMode} target=${target} msg=${escapedMsg}` })
      .catch(console.error);
      
    // Optimistically add own message
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

  const handleChannelClick = (channel: Channel) => {
    setSelectedClient(undefined);
    setSelectedChannel(channel);
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
              onChannelClick={handleChannelClick}
              onClientClick={handleClientClick}
            />
            {channels.length === 0 && <p className="loading-text">Loading channels...</p>}
          </div>

          <div className="info-area">
            <InfoPane selectedChannel={selectedChannel} selectedClient={selectedClient} />
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
        <button className="btn-icon" onClick={() => setIsSettingsOpen(true)}>
          ⚙️ Settings
        </button>
        <button className="btn-danger" onClick={handleDisconnect}>
          Disconnect
        </button>
      </div>
      
      {isSettingsOpen && (
        <SettingsModal 
          onClose={() => setIsSettingsOpen(false)} 
          identity={identity} 
          onIdentityUpdated={onIdentityUpdated} 
        />
      )}
    </div>
  );
}
