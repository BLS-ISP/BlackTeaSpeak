import { useEffect, useRef } from 'react';
import { listen } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';
import { eventBus } from '../EventBus';
import { Channel, Client, ChatMessage, FileEntry } from '../types';

interface UseServerEventsProps {
  onDisconnect: () => void;
  setChannels: React.Dispatch<React.SetStateAction<Channel[]>>;
  setClients: React.Dispatch<React.SetStateAction<Client[]>>;
  setMyClientId: (id: string) => void;
  setChatMessages: React.Dispatch<React.SetStateAction<ChatMessage[]>>;
  setChannelFiles: React.Dispatch<React.SetStateAction<FileEntry[]>>;
  pendingTransfers: React.MutableRefObject<Map<string, any>>;
  executeFileTransfer: (transfer: any, ftkey: string, port: number) => void;
}

export function useServerEvents({
  onDisconnect,
  setChannels,
  setClients,
  setMyClientId,
  setChatMessages,
  setChannelFiles,
  pendingTransfers,
  executeFileTransfer
}: UseServerEventsProps) {
  const myClientIdRef = useRef<string>('');

  useEffect(() => {
    let unlistenDisconnect: (() => void) | undefined;
    let unlistenTransmit: (() => void) | undefined;

    const unsubscribe = eventBus.subscribe((rows) => {
      for (const row of rows) {
        if (row.command === 'initserver') {
          if (row.args.client_id) {
            setMyClientId(row.args.client_id);
            myClientIdRef.current = row.args.client_id;
          }
        } else if (row.args.client_id && row.command === 'unknown') {
          setMyClientId(row.args.client_id);
          myClientIdRef.current = row.args.client_id;
        } else if (row.command === 'channellist' || row.command === 'notifychannelcreated') {
          setChannels(prev => {
            const existing = prev.find(c => c.cid === row.args.cid);
            if (existing) {
              return prev.map(c => c.cid === row.args.cid ? { ...c, ...row.args } as any as Channel : c);
            }
            return [...prev, row.args as any as Channel];
          });
        } else if (row.command === 'notifychanneledited') {
          setChannels(prev => prev.map(c => 
            c.cid === row.args.cid ? { ...c, ...row.args } as any as Channel : c
          ));
        } else if (row.command === 'notifychanneldeleted') {
          setChannels(prev => prev.filter(c => c.cid !== row.args.cid));
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
              client_flag_avatar: row.args.client_flag_avatar,
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
                client_flag_avatar: row.args.client_flag_avatar !== undefined ? row.args.client_flag_avatar : c.client_flag_avatar,
              };
            }
            return c;
          }));
        } else if (row.command === 'notifytalkstatus') {
          const isTalking = row.args.status === '1';
          const isWhisper = row.args.isreceivedwhisper === '1';
          const clid = parseInt(row.args.clid || '0');
          setClients(prev => prev.map(c => {
            if (c.clid === clid.toString()) {
               return { ...c, is_talking: isTalking, whisper_type: isWhisper ? (c.clid === myClientIdRef.current ? 'send' : 'receive') : undefined };
            }
            return c;
          }));
        } else if (row.command === 'notifytextmessage') {
          if (row.args.invokerid !== myClientIdRef.current) {
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

    const setup = async () => {
      unlistenDisconnect = await listen('server_disconnect', () => {
        onDisconnect();
      });
      
      unlistenTransmit = await listen<boolean>('is_transmitting', (e) => {
        const isTransmitting = e.payload;
        setClients(prev => prev.map(c => c.clid === myClientIdRef.current ? { ...c, is_talking: isTransmitting } : c));
      });

      await eventBus.waitForReady();

      invoke('send_command', { command: 'servernotifyregister event=server' }).catch(console.error);
      invoke('send_command', { command: 'servernotifyregister event=channel id=0' }).catch(console.error);
      invoke('send_command', { command: 'servernotifyregister event=textserver' }).catch(console.error);
      invoke('send_command', { command: 'servernotifyregister event=textchannel' }).catch(console.error);
      invoke('send_command', { command: 'servernotifyregister event=textprivate' }).catch(console.error);
      invoke('send_command', { command: 'whoami' }).catch(console.error);
      invoke('send_command', { command: 'channellist' }).catch(console.error);
      invoke('send_command', { command: 'clientlist' }).catch(console.error);
    };

    setup();

    return () => {
      unsubscribe();
      if (unlistenDisconnect) unlistenDisconnect();
      if (unlistenTransmit) unlistenTransmit();
    };
  }, []);
}
