import React from 'react';
import {
  Edit3,
  Hash,
  Lock,
  LogIn,
  MicOff,
  Music,
  Plus,
  Server,
  Trash2,
  Volume2,
} from 'lucide-react';
import {
  blackTeaStore,
  getClientCapabilities,
  useBlackTeaStore,
} from '../../store/useBlackTeaStore';
import { ChannelInfo } from '../../types/protocol';
import { AUDIO_CODECS } from '../modals/channel-editor/codecSpecs';
import { ChannelTreeClientRow } from './ChannelTreeClientRow';

export const ServerChannelTree: React.FC = () => {
  const state = useBlackTeaStore();
  const caps = getClientCapabilities(state);

  const renderChannelNode = (channel: ChannelInfo, depth = 0): React.ReactNode => {
    const childChannels = state.channels
      .filter((c) => c.pid === channel.cid)
      .sort((a, b) => a.channel_order - b.channel_order || a.cid - b.cid);
    const channelClients = state.clients.filter((cl) => cl.cid === channel.cid);
    const isCurrent = state.currentChannelId === channel.cid;
    const isSelected =
      state.selectedChannelId === channel.cid && state.selectedClientId === null;
    const codecSpec =
      AUDIO_CODECS.find((c) => c.id === channel.channel_codec) ?? AUDIO_CODECS[4];

    return (
      <div key={channel.cid} className="tree-channel-block">
        <div
          className={`tree-channel-row ${isCurrent ? 'current-channel' : ''} ${
            isSelected ? 'selected' : ''
          }`}
          style={{ paddingLeft: `${12 + depth * 16}px` }}
          onDragOver={(e) => {
            e.preventDefault();
          }}
          onDrop={(e) => {
            e.preventDefault();
            const raw = e.dataTransfer.getData('text/plain');
            if (raw.startsWith('client:')) {
              const draggedClid = Number(raw.slice(7));
              if (draggedClid > 0) {
                void blackTeaStore.moveClientToChannel(draggedClid, channel.cid);
              }
            }
          }}
          onClick={() =>
            blackTeaStore.patchState({
              selectedChannelId: channel.cid,
              selectedClientId: null,
            })
          }
          onDoubleClick={() => {
            if (!isCurrent) {
              if (channel.channel_flag_password) {
                const pw = window.prompt(
                  `Passwort für Channel '${channel.channel_name}':`,
                  ''
                );
                if (pw !== null) blackTeaStore.moveToChannel(channel.cid, pw);
              } else {
                blackTeaStore.moveToChannel(channel.cid);
              }
            }
          }}
        >
          <span className="channel-icon">
            {channel.channel_flag_password ? <Lock size={13} /> : <Hash size={14} />}
          </span>

          <span className="channel-name" title={channel.channel_topic || channel.channel_name}>
            {channel.channel_name}
            {channel.channel_flag_default && (
              <span
                title="Standard-Channel (Default)"
                style={{ fontSize: 10, color: '#34d399', marginLeft: 5 }}
              >
                ★
              </span>
            )}
          </span>

          <div className="channel-meta">
            {channel.channel_needed_talk_power > 0 && (
              <span
                className="codec-tag"
                title={`Moderierter Channel (Benötigte Talk-Power: ${channel.channel_needed_talk_power})`}
                style={{ color: '#fbbf24' }}
              >
                <MicOff size={11} />
              </span>
            )}
            {channel.channel_codec === 5 ? (
              <span
                className="codec-tag"
                title={`${codecSpec.shortName} • Qualitätsstufe ${channel.channel_codec_quality}/10`}
                style={{ color: '#c084fc' }}
              >
                <Music size={11} />
              </span>
            ) : (
              <span
                className="codec-tag"
                title={`${codecSpec.shortName} • Qualitätsstufe ${channel.channel_codec_quality}/10`}
              >
                <Volume2 size={11} />
              </span>
            )}
            {(channelClients.length > 0 || channel.channel_maxclients >= 0) && (
              <span className="client-count-pill">
                {channel.channel_maxclients >= 0
                  ? `${channelClients.length}/${channel.channel_maxclients}`
                  : channelClients.length}
              </span>
            )}
          </div>

          <div className="channel-hover-actions" onClick={(e) => e.stopPropagation()}>
            {!isCurrent && (
              <button
                title="In diesen Channel wechseln"
                onClick={() => {
                  if (channel.channel_flag_password) {
                    const pw = window.prompt(
                      `Passwort für Channel '${channel.channel_name}':`,
                      ''
                    );
                    if (pw !== null) blackTeaStore.moveToChannel(channel.cid, pw);
                  } else {
                    blackTeaStore.moveToChannel(channel.cid);
                  }
                }}
              >
                <LogIn size={12} />
              </button>
            )}
            {caps.canEditChannel && (
              <button
                title="Channel bearbeiten (Qualität, Typ, Rechte)"
                onClick={() => {
                  blackTeaStore.patchState({
                    selectedChannelId: channel.cid,
                    selectedClientId: null,
                    activeModal: 'channel_edit',
                  });
                }}
              >
                <Edit3 size={12} />
              </button>
            )}
            {caps.canDeleteChannel && !channel.channel_flag_default && (
              <button
                title="Channel löschen"
                onClick={() => {
                  if (
                    window.confirm(`Channel '${channel.channel_name}' wirklich löschen?`)
                  ) {
                    blackTeaStore.deleteChannel(channel.cid);
                  }
                }}
              >
                <Trash2 size={12} />
              </button>
            )}
          </div>
        </div>

        {channelClients.length > 0 && (
          <div
            className="tree-channel-clients"
            style={{ paddingLeft: `${24 + depth * 16}px` }}
          >
            {channelClients.map((cl) => (
              <ChannelTreeClientRow
                key={cl.clid}
                client={cl}
                channel={channel}
                state={state}
                caps={caps}
              />
            ))}
          </div>
        )}

        {childChannels.map((sub) => renderChannelNode(sub, depth + 1))}
      </div>
    );
  };

  const rootChannels = state.channels
    .filter((c) => c.pid === 0)
    .sort((a, b) => a.channel_order - b.channel_order || a.cid - b.cid);

  return (
    <aside className="server-channel-tree">
      <div className="tree-header">
        <div
          className="tree-server-root"
          onClick={() =>
            blackTeaStore.patchState({
              selectedChannelId: null,
              selectedClientId: null,
            })
          }
        >
          <Server size={15} className="server-icon" />
          <span className="server-root-name">{state.serverInfo.virtualserver_name}</span>
          <span className="server-count-badge">{state.clients.length} Online</span>
        </div>

        {caps.canCreateChannel && (
          <button
            className="btn-icon-sm"
            disabled={state.connectionState !== 'connected'}
            onClick={() => blackTeaStore.patchState({ activeModal: 'channel_create' })}
            title="Neuen Channel erstellen"
          >
            <Plus size={15} />
          </button>
        )}
      </div>

      <div className="tree-scroll-area">
        {state.connectionState !== 'connected' ? (
          <div className="tree-empty-hint">
            Klicke oben auf <strong>Verbinden</strong>, um dem Voice-Server beizutreten.
          </div>
        ) : rootChannels.length === 0 ? (
          <div className="tree-empty-hint">Lade Channels...</div>
        ) : (
          rootChannels.map((ch) => renderChannelNode(ch, 0))
        )}
      </div>
    </aside>
  );
};
