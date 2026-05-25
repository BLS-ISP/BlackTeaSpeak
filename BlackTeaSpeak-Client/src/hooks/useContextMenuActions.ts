import { useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Channel, Client } from '../types';
import { escapeTs3String } from '../ts3parser';
import { Toast } from '../ui/Toast';
import { Dialogs } from '../ui/Dialogs';

export function useContextMenuActions() {
  const [isSettingsOpen, setIsSettingsOpen] = useState(false);
  const [isGroupManagerOpen, setIsGroupManagerOpen] = useState(false);
  const [isBanManagerOpen, setIsBanManagerOpen] = useState(false);
  const [isTokenManagerOpen, setIsTokenManagerOpen] = useState(false);
  const [channelEditTarget, setChannelEditTarget] = useState<{cid?: string, cpid?: string} | null>(null);
  const [permissionTarget, setPermissionTarget] = useState<{type: 'servergroup' | 'channelgroup' | 'client' | 'channel', targetId: string} | null>(null);

  const handleContextMenuAction = async (action: string, type: 'channel' | 'client' | 'server', target: any) => {
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
        setPermissionTarget({ type: 'servergroup', targetId: '0' });
      }
    } else if (type === 'channel') {
      const channel = target as Channel;
      if (action === 'channel_create') {
        setChannelEditTarget({ cpid: channel.cid });
      } else if (action === 'channel_edit') {
        setChannelEditTarget({ cid: channel.cid });
      } else if (action === 'channel_delete') {
        if (await Dialogs.confirm('Delete Channel', `Are you sure you want to delete ${channel.channel_name}?`)) {
          invoke('send_command', { command: `channeldelete cid=${channel.cid} force=1` });
          Toast.success("Channel deleted");
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
        const time = await Dialogs.prompt("Ban Time", "Enter ban time in seconds (0 for permanent):", "0");
        if (time === null) return;
        const reason = await Dialogs.prompt("Ban Reason", "Enter ban reason:");
        if (reason === null) return;
        invoke('send_command', { command: `banclient clid=${client.clid} time=${time} banreason=${escapeTs3String(reason)}` });
        Toast.success("Client banned");
      } else if (action === 'permissions_client') {
        setPermissionTarget({ type: 'client', targetId: client.clid });
      }
    }
  };

  return {
    handleContextMenuAction,
    isSettingsOpen, setIsSettingsOpen,
    isGroupManagerOpen, setIsGroupManagerOpen,
    isBanManagerOpen, setIsBanManagerOpen,
    isTokenManagerOpen, setIsTokenManagerOpen,
    channelEditTarget, setChannelEditTarget,
    permissionTarget, setPermissionTarget
  };
}
