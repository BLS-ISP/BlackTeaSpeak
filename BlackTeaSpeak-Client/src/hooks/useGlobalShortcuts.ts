import { useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { register, unregister, isRegistered } from '@tauri-apps/plugin-global-shortcut';
import { Identity } from '../App';

export function useGlobalShortcuts(identity: Identity) {
  useEffect(() => {
    let activePttShortcut = '';
    let activeWhisperShortcut = '';

    async function setupPttHotkey() {
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
        activePttShortcut = shortcut;
      } catch (err) {
        console.error("Failed to register PTT hotkey:", err);
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

    setupPttHotkey();
    setupWhisperHotkey();

    return () => {
      if (activePttShortcut) {
        unregister(activePttShortcut).catch(console.error);
        invoke('set_ptt_state', { pressed: false }).catch(console.error);
      }
      if (activeWhisperShortcut) {
        unregister(activeWhisperShortcut).catch(console.error);
        invoke('set_whisper_state', { active: false }).catch(console.error);
      }
    };
  }, [identity.voice_transmission_mode, identity.ptt_hotkey, identity.whisper_hotkey, identity.whisper_targets]);
}
