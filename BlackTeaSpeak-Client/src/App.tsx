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
import { blackTeaStore, useBlackTeaStore } from './store/useBlackTeaStore';

export const App: React.FC = () => {
  const state = useBlackTeaStore();

  useEffect(() => {
    const isPttKeyMatch = (e: KeyboardEvent) => {
      const targetKey = state.pttKey || 'Space';
      return (
        e.code === targetKey ||
        e.key === targetKey ||
        (targetKey.toLowerCase() === 'space' && e.code === 'Space')
      );
    };

    const handleKeyDown = (e: KeyboardEvent) => {
      const tag = (e.target as HTMLElement)?.tagName;
      if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT') return;
      if (isPttKeyMatch(e) && state.transmissionMode === 'ptt') {
        e.preventDefault();
        blackTeaStore.setPttPressed(true);
      }
    };
    const handleKeyUp = (e: KeyboardEvent) => {
      if (isPttKeyMatch(e) && state.transmissionMode === 'ptt') {
        blackTeaStore.setPttPressed(false);
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    window.addEventListener('keyup', handleKeyUp);
    return () => {
      window.removeEventListener('keydown', handleKeyDown);
      window.removeEventListener('keyup', handleKeyUp);
    };
  }, [state.transmissionMode, state.pttKey]);

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

