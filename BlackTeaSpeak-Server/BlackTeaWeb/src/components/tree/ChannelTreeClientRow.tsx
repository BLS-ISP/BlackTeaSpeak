import React from 'react';
import {
  Clock,
  MessageSquare,
  MicOff,
  Music,
  Radio,
  ShieldCheck,
  VolumeX,
  Zap,
} from 'lucide-react';
import {
  blackTeaStore,
  BlackTeaState,
  ClientCapabilities,
} from '../../store/useBlackTeaStore';
import { ChannelInfo, ClientInfo } from '../../types/protocol';

export interface ChannelTreeClientRowProps {
  client: ClientInfo;
  channel: ChannelInfo;
  state: BlackTeaState;
  caps: ClientCapabilities;
}

export const ChannelTreeClientRow: React.FC<ChannelTreeClientRowProps> = ({
  client,
  channel,
  state,
  caps,
}) => {
  const isSelf = client.clid === state.selfClientId;
  const isSelected = state.selectedClientId === client.clid;
  const isMusicBot =
    client.client_type === 2 ||
    client.client_unique_identifier.startsWith('compat-musicbot-');

  const clientGroups = client.client_servergroups
    .map((sgid) => state.serverGroups.find((g) => g.sgid === sgid))
    .filter((g): g is NonNullable<typeof g> => Boolean(g));

  const elevatedGroups = clientGroups.filter((g) => {
    const lower = g.name.toLowerCase();
    return g.sgid !== 8 && lower !== 'normal' && lower !== 'guest';
  });

  const isClientAdmin =
    client.client_servergroups.includes(6) ||
    client.client_servergroups.includes(9) ||
    elevatedGroups.some((g) => g.name.toLowerCase().includes('admin'));

  const allGroupNames = clientGroups.map((g) => g.name).join(', ');
  let primaryBadgeLabel: string | null = null;
  let extraBadgeCount = 0;

  if (isMusicBot) {
    primaryBadgeLabel = 'Bot';
  } else if (isClientAdmin) {
    primaryBadgeLabel = 'Admin';
    const nonAdminElevated = elevatedGroups.filter(
      (g) => g.sgid !== 6 && g.sgid !== 9 && !g.name.toLowerCase().includes('admin')
    );
    extraBadgeCount = nonAdminElevated.length;
  } else if (elevatedGroups.length > 0) {
    const first = elevatedGroups[0].name;
    primaryBadgeLabel = first.length > 10 ? `${first.slice(0, 9)}…` : first;
    extraBadgeCount = elevatedGroups.length - 1;
  }

  const lacksTalkPowerInChannel =
    channel.channel_needed_talk_power > 0 &&
    client.client_talk_power < channel.channel_needed_talk_power &&
    !client.client_is_talker &&
    !isClientAdmin;

  return (
    <div
      draggable={caps.isConnected && (isSelf || caps.canModerate)}
      onDragStart={(e) => {
        e.dataTransfer.setData('text/plain', `client:${client.clid}`);
      }}
      className={`tree-client-row ${isSelected ? 'selected' : ''} ${
        client.isTalking ? 'talking' : ''
      }`}
      onClick={(e) => {
        e.stopPropagation();
        blackTeaStore.patchState({
          selectedClientId: client.clid,
          selectedChannelId: null,
        });
      }}
    >
      <span
        className={`talk-orb ${
          client.isTalking
            ? client.isWhispering
              ? 'whispering'
              : 'speaking'
            : client.client_output_muted
            ? 'deaf'
            : client.client_input_muted || lacksTalkPowerInChannel
            ? 'muted'
            : 'idle'
        }`}
        style={
          client.client_is_channel_commander && !client.isTalking
            ? { background: '#f97316', boxShadow: '0 0 6px rgba(249,115,22,0.7)' }
            : undefined
        }
        title={
          client.client_is_channel_commander
            ? 'Channel Commander aktiv'
            : lacksTalkPowerInChannel
            ? `Moderierter Channel (benötigt Talk-Power ${channel.channel_needed_talk_power})`
            : undefined
        }
      />

      {isMusicBot && <Music size={13} className="bot-icon" />}

      <span
        className={`client-name ${isSelf ? 'self-name' : ''}`}
        title={
          client.client_away && client.client_away_message
            ? `${client.client_nickname} — Abwesend: ${client.client_away_message}`
            : client.client_nickname
        }
      >
        {client.client_nickname}
        {client.client_away && (
          <em style={{ fontSize: 10, color: '#94a3b8', marginLeft: 4 }}>
            [{client.client_away_message || 'AFK'}]
          </em>
        )}
        {isSelf && <em className="you-tag"> (Du)</em>}
      </span>

      <div className="client-badges">
        {(client.client_talk_request ?? 0) > 0 && (
          <span
            title={`Talk-Power-Anfrage: ${client.client_talk_request_msg || 'Bittet um Sprachrecht'}`}
            style={{ fontSize: 11, cursor: 'help' }}
          >
            ✋
          </span>
        )}
        {client.client_away && (
          <span title={`Abwesend ${client.client_away_message || ''}`}>
            <Clock size={11} color="#94a3b8" />
          </span>
        )}
        {client.client_is_channel_commander && (
          <span title="Channel Commander">
            <Radio size={11} color="#f97316" />
          </span>
        )}
        {(client.client_input_muted || lacksTalkPowerInChannel) && (
          <MicOff size={12} className="muted-icon" />
        )}
        {client.client_output_muted && <VolumeX size={12} className="deaf-icon" />}
        {primaryBadgeLabel && (
          <span
            className={`sg-badge ${isClientAdmin ? 'sg-admin' : ''}`}
            title={allGroupNames ? `Servergruppen: ${allGroupNames}` : primaryBadgeLabel}
          >
            {isClientAdmin && <ShieldCheck size={10} />}
            <span>{primaryBadgeLabel}</span>
            {extraBadgeCount > 0 && <span className="sg-badge-more">+{extraBadgeCount}</span>}
          </span>
        )}
      </div>

      {!isSelf && (
        <div className="client-hover-actions" onClick={(e) => e.stopPropagation()}>
          <button
            title="Privatnachricht öffnen"
            onClick={() =>
              blackTeaStore.patchState({
                activeChatTab: 'private',
                privateChatTargetClientId: client.clid,
                selectedClientId: client.clid,
              })
            }
          >
            <MessageSquare size={12} />
          </button>
          <button
            title="Client anstupsen (Poke)"
            onClick={() => {
              const msg = window.prompt(
                `Poke-Nachricht an ${client.client_nickname}:`,
                'Hey!'
              );
              if (msg !== null) blackTeaStore.pokeClient(client.clid, msg);
            }}
          >
            <Zap size={12} />
          </button>
        </div>
      )}
    </div>
  );
};
