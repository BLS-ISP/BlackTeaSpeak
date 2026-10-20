import React, { useEffect } from 'react';
import { TopToolbar } from './components/layout/TopToolbar';
import { ConnectionBanner } from './components/layout/ConnectionBanner';
import { StatusBar } from './components/layout/StatusBar';
import { ServerChannelTree } from './components/tree/ServerChannelTree';
import { InspectorPanel } from './components/tree/InspectorPanel';
import { WebCodecsVideoStage } from './components/stage/WebCodecsVideoStage';
import { SpatialAudioRadar } from './components/spatial/SpatialAudioRadar';
import { MusicBotDeck } from './components/musicbot/MusicBotDeck';
import { RichChatDock } from './components/chat/RichChatDock';
import { ModalContainer } from './components/modals/ModalContainer';
import { isTauriRuntime } from './protocols/tauriBridge';
import { blackTeaStore, useBlackTeaStore } from './store/useBlackTeaStore';

function matchesConfiguredKey(e: KeyboardEvent, targetKey: string): boolean {
  if (!targetKey) return false;
  return (
    e.code === targetKey ||
    e.key === targetKey ||
    (targetKey.toLowerCase() === 'space' && e.code === 'Space')
  );
}

function toTauriShortcutAccelerator(rawKey: string): string | null {
  const trimmed = rawKey.trim();
  if (!trimmed) return null;
  if (trimmed.startsWith('Key') && trimmed.length === 4) {
    return `CommandOrControl+Shift+${trimmed.slice(3)}`;
  }
  if (trimmed.startsWith('F') && /^F([1-9]|1[0-2])$/i.test(trimmed)) {
    return trimmed.toUpperCase();
  }
  if (trimmed.includes('+')) {
    return trimmed;
  }
  return null;
}

export const App: React.FC = () => {
  const state = useBlackTeaStore();
  const activeIdentity =
    state.desktopIdentities.find((i) => i.id === state.activeIdentityId) ??
    state.desktopIdentities[0];
  const whisperHotkey = activeIdentity?.whisper_hotkey || 'AltLeft';

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      const tag = (e.target as HTMLElement)?.tagName;
      if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT') return;

      if (matchesConfiguredKey(e, state.pttKey || 'Space') && state.transmissionMode === 'ptt') {
        e.preventDefault();
        if (!e.repeat) {
          blackTeaStore.setPttPressed(true);
        }
      } else if (matchesConfiguredKey(e, whisperHotkey) && !e.repeat) {
        e.preventDefault();
        blackTeaStore.toggleWhisperMode();
      }
    };

    const handleKeyUp = (e: KeyboardEvent) => {
      if (matchesConfiguredKey(e, state.pttKey || 'Space') && state.transmissionMode === 'ptt') {
        blackTeaStore.setPttPressed(false);
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    window.addEventListener('keyup', handleKeyUp);
    return () => {
      window.removeEventListener('keydown', handleKeyDown);
      window.removeEventListener('keyup', handleKeyUp);
    };
  }, [state.transmissionMode, state.pttKey, whisperHotkey]);

  useEffect(() => {
    if (!isTauriRuntime()) return;
    let cancelled = false;
    const registeredAccelerators: string[] = [];

    void (async () => {
      try {
        const gs = await import('@tauri-apps/plugin-global-shortcut');
        if (cancelled) return;

        const pttAcc = toTauriShortcutAccelerator(state.pttKey || '');
        if (pttAcc && state.transmissionMode === 'ptt') {
          await gs.register(pttAcc, (event) => {
            blackTeaStore.setPttPressed(event.state === 'Pressed');
          });
          registeredAccelerators.push(pttAcc);
        }

        const whisperAcc = toTauriShortcutAccelerator(whisperHotkey);
        if (whisperAcc && whisperAcc !== pttAcc) {
          await gs.register(whisperAcc, (event) => {
            if (event.state === 'Pressed') {
              blackTeaStore.toggleWhisperMode();
            }
          });
          registeredAccelerators.push(whisperAcc);
        }
      } catch {
        // Ignore if global shortcut registration is unsupported for the chosen key
      }
    })();

    return () => {
      cancelled = true;
      if (registeredAccelerators.length > 0) {
        void import('@tauri-apps/plugin-global-shortcut')
          .then((gs) => gs.unregister(registeredAccelerators))
          .catch(() => {});
      }
    };
  }, [state.transmissionMode, state.pttKey, whisperHotkey]);

  return (
    <div className="app-shell">
      <TopToolbar />
      <ConnectionBanner />

      <main className="workspace-split">
        <ServerChannelTree />

        <div className="center-stage-column">
          <WebCodecsVideoStage />
          <SpatialAudioRadar />
          <MusicBotDeck />
          <RichChatDock />
        </div>

        <InspectorPanel />
      </main>

      <StatusBar />
      <ModalContainer />

      {state.statusToast && (
        <div className="toast-notification">{state.statusToast}</div>
      )}
    </div>
  );
};

export default App;
