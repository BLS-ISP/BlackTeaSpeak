import React from 'react';
import {
  getClientCapabilities,
  useBlackTeaStore,
} from '../../store/useBlackTeaStore';
import { MusicBotInspectorView } from './inspector/MusicBotInspectorView';
import { ClientInspectorView } from './inspector/ClientInspectorView';
import { ChannelInspectorView } from './inspector/ChannelInspectorView';
import { ServerInspectorView } from './inspector/ServerInspectorView';

export const InspectorPanel: React.FC = () => {
  const state = useBlackTeaStore();
  const caps = getClientCapabilities(state);

  const selectedClient =
    state.selectedClientId !== null
      ? state.clients.find((c) => c.clid === state.selectedClientId) ?? null
      : null;
  const selectedChannel =
    state.selectedChannelId !== null
      ? state.channels.find((c) => c.cid === state.selectedChannelId) ?? null
      : null;

  if (selectedClient) {
    const selectedBot =
      state.musicBots.find((b) => b.client_id === selectedClient.clid) ?? null;
    const isMusicBot =
      selectedBot !== null ||
      selectedClient.client_type === 2 ||
      selectedClient.client_unique_identifier.startsWith('compat-musicbot-');

    if (isMusicBot && selectedBot) {
      return (
        <MusicBotInspectorView
          state={state}
          caps={caps}
          selectedClient={selectedClient}
          selectedBot={selectedBot}
        />
      );
    }

    return (
      <ClientInspectorView
        state={state}
        caps={caps}
        selectedClient={selectedClient}
      />
    );
  }

  if (selectedChannel) {
    return (
      <ChannelInspectorView
        state={state}
        caps={caps}
        selectedChannel={selectedChannel}
      />
    );
  }

  return <ServerInspectorView state={state} caps={caps} />;
};
