import React from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Dialogs } from './ui/Dialogs';
import { Toast } from './ui/Toast';
import { Shield as ShieldIcon, Headphones as HeadphoneIcon, Users as UsersIcon, MicOff as MicOffIcon, Mic as MicIcon, Settings as SettingsIcon, Key as KeyIcon, Hammer as HammerIcon } from 'lucide-react';


interface ControlBarProps {
  isMicMuted: boolean;
  isSpeakerMuted: boolean;
  handleToggleMic: () => void;
  handleToggleSpeaker: () => void;
  handleDisconnect: () => void;
  setIsTokenManagerOpen: (v: boolean) => void;
  setIsBanManagerOpen: (v: boolean) => void;
  setIsGroupManagerOpen: (v: boolean) => void;
  setIsSettingsOpen: (v: boolean) => void;
  hasPermission: (permName: string) => boolean;
}

export function ControlBar({
  isMicMuted, isSpeakerMuted,
  handleToggleMic, handleToggleSpeaker, handleDisconnect,
  setIsTokenManagerOpen, setIsBanManagerOpen, setIsGroupManagerOpen, setIsSettingsOpen,
  hasPermission
}: ControlBarProps) {
  
  const handleUseToken = async () => {
    const key = await Dialogs.prompt("Use Privilege Key", "Enter Privilege Key to use:");
    if (key) {
      invoke('send_command', { command: `tokenuse token=${key}` });
      Toast.info("Token use requested");
    }
  };

  return (
    <div className="control-bar">
      <button className={`btn-icon ${isMicMuted ? 'muted' : ''}`} onClick={handleToggleMic}>
        {isMicMuted ? '<MicOffIcon /> Mic Muted' : '<MicIcon /> Mic Active'}
      </button>
      <button className={`btn-icon ${isSpeakerMuted ? 'muted' : ''}`} onClick={handleToggleSpeaker}>
        {isSpeakerMuted ? '<HeadphoneIcon /> Speaker Muted' : '<HeadphoneIcon /> Speaker Active'}
      </button>
      <button className="btn-icon" onClick={handleUseToken}>
        <KeyIcon /> Use Token
      </button>
      {hasPermission('b_virtualserver_token_list') && (
        <button className="btn-icon" onClick={() => setIsTokenManagerOpen(true)}>
          <ShieldIcon /> Manage Tokens
        </button>
      )}
      {hasPermission('b_client_ban_list') && (
        <button className="btn-icon" onClick={() => setIsBanManagerOpen(true)}>
          <HammerIcon /> Bans
        </button>
      )}
      {hasPermission('b_virtualserver_servergroup_list') && (
        <button className="btn-icon" onClick={() => setIsGroupManagerOpen(true)}>
          <UsersIcon /> Groups
        </button>
      )}
      <button className="btn-icon" onClick={() => setIsSettingsOpen(true)}>
        <SettingsIcon /> Settings
      </button>
      <button className="btn-danger" onClick={handleDisconnect}>
        Disconnect
      </button>
    </div>
  );
}
