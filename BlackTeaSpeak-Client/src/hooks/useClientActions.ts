import { useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { ChatMessage } from '../types';
import { escapeTs3String } from '../ts3parser';
import { Identity } from '../App';
import { AudioService } from '../services/AudioService';

export function useClientActions(
  myClientId: string, 
  identity: Identity, 
  setChatMessages: React.Dispatch<React.SetStateAction<ChatMessage[]>>,
  onDisconnect: () => void
) {
  const [isMicMuted, setIsMicMuted] = useState(false);
  const [isSpeakerMuted, setIsSpeakerMuted] = useState(false);

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
      if (newMuted) AudioService.playMicMuted(); else AudioService.playMicActivated();
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
      if (newMuted) AudioService.playSpeakerMuted(); else AudioService.playSpeakerActivated();
    } catch (e) {
      console.error(e);
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

  return {
    isMicMuted,
    isSpeakerMuted,
    handleDisconnect,
    handleToggleMic,
    handleToggleSpeaker,
    handleSendMessage
  };
}
