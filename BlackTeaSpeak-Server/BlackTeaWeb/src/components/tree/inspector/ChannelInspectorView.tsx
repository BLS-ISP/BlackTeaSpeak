import React from 'react';
import {
  Edit3,
  FolderOpen,
  Hash,
  Lock,
  LogIn,
  Mic,
  Shield,
  Trash2,
} from 'lucide-react';
import {
  blackTeaStore,
  BlackTeaState,
  ClientCapabilities,
} from '../../../store/useBlackTeaStore';
import { ChannelInfo } from '../../../types/protocol';
import {
  AUDIO_CODECS,
  estimateCodecBitrate,
} from '../../modals/channel-editor/codecSpecs';

export interface ChannelInspectorViewProps {
  state: BlackTeaState;
  caps: ClientCapabilities;
  selectedChannel: ChannelInfo;
}

export const ChannelInspectorView: React.FC<ChannelInspectorViewProps> = ({
  state,
  caps,
  selectedChannel,
}) => {
  const isCurrent = selectedChannel.cid === state.currentChannelId;
  const codecSpec =
    AUDIO_CODECS.find((c) => c.id === selectedChannel.channel_codec) ?? AUDIO_CODECS[4];
  const bitrate = estimateCodecBitrate(
    selectedChannel.channel_codec,
    selectedChannel.channel_codec_quality
  );
  const selfClient = state.clients.find((c) => c.clid === state.selfClientId);
  const isModeratedForSelf =
    isCurrent &&
    selectedChannel.channel_needed_talk_power > 0 &&
    (selfClient?.client_talk_power ?? 0) < selectedChannel.channel_needed_talk_power &&
    !selfClient?.client_is_talker;

  return (
    <aside className="inspector-panel">
      <div className="inspector-header">
        <div className="inspector-avatar channel">
          <Hash size={20} />
        </div>
        <div>
          <h3 className="inspector-title">
            {selectedChannel.channel_name}
            {selectedChannel.channel_flag_password && (
              <Lock
                size={13}
                style={{ marginLeft: 6, color: '#fbbf24', verticalAlign: 'middle' }}
              />
            )}
          </h3>
          <span className="inspector-subtitle">
            {selectedChannel.channel_flag_default
              ? 'Standard-Channel (Permanent)'
              : selectedChannel.channel_flag_permanent
              ? 'Permanenter Channel'
              : selectedChannel.channel_flag_semi_permanent
              ? 'Semi-Permanenter Channel'
              : 'Temporärer Channel'}
            {' • '}#{selectedChannel.cid}
          </span>
        </div>
      </div>

      {selectedChannel.channel_topic && (
        <div className="inspector-section">
          <h4>Thema</h4>
          <p className="inspector-text">{selectedChannel.channel_topic}</p>
        </div>
      )}

      <div className="inspector-section">
        <h4>Beschreibung</h4>
        <div className="inspector-description-box">
          {selectedChannel.channel_description || 'Keine Channel-Beschreibung vorhanden.'}
        </div>
      </div>

      <div className="inspector-section">
        <h4>Audio-Codec &amp; Sprachqualität</h4>
        <div className="kv-list">
          <div className="kv-row">
            <span>Audio-Codec</span>
            <span>{codecSpec.shortName}</span>
          </div>
          <div className="kv-row">
            <span>Qualitätsstufe</span>
            <span>
              Stufe {selectedChannel.channel_codec_quality} / 10 ({bitrate.kbps} kbit/s)
            </span>
          </div>
          <div className="kv-row">
            <span>Netzwerk-Bandbreite</span>
            <span>{bitrate.kibPerSec} KiB/s</span>
          </div>
        </div>
      </div>

      <div className="inspector-section">
        <h4>Limits &amp; Berechtigungen</h4>
        <div className="kv-list">
          <div className="kv-row">
            <span>Max. Clients</span>
            <span>
              {selectedChannel.channel_maxclients < 0
                ? 'Unbegrenzt'
                : selectedChannel.channel_maxclients}
            </span>
          </div>
          <div className="kv-row">
            <span>Max. Family-Clients</span>
            <span>
              {selectedChannel.channel_maxfamilyclients < 0
                ? 'Unbegrenzt'
                : selectedChannel.channel_maxfamilyclients}
            </span>
          </div>
          <div className="kv-row">
            <span>Benötigte Talk-Power</span>
            <span>
              {selectedChannel.channel_needed_talk_power > 0
                ? `${selectedChannel.channel_needed_talk_power} (Moderiert)`
                : '0 (Offen)'}
            </span>
          </div>
          {(selectedChannel.i_channel_needed_join_power ?? 0) > 0 && (
            <div className="kv-row">
              <span>Benötigte Join-Power</span>
              <span>{selectedChannel.i_channel_needed_join_power}</span>
            </div>
          )}
          {(selectedChannel.i_channel_needed_subscribe_power ?? 0) > 0 && (
            <div className="kv-row">
              <span>Benötigte Subscribe-Power</span>
              <span>{selectedChannel.i_channel_needed_subscribe_power}</span>
            </div>
          )}
          {(selectedChannel.i_channel_needed_modify_power ?? 0) > 0 && (
            <div className="kv-row">
              <span>Benötigte Modify-Power</span>
              <span>{selectedChannel.i_channel_needed_modify_power}</span>
            </div>
          )}
        </div>
      </div>

      <div className="inspector-section">
        <h4>Channel-Aktionen</h4>
        <div className="inspector-action-grid">
          {!isCurrent && (
            <button
              className="btn-primary-sm"
              disabled={!caps.isConnected}
              onClick={() => {
                if (selectedChannel.channel_flag_password) {
                  const pw = window.prompt(
                    `Passwort für Channel '${selectedChannel.channel_name}':`,
                    ''
                  );
                  if (pw !== null) blackTeaStore.moveToChannel(selectedChannel.cid, pw);
                } else {
                  blackTeaStore.moveToChannel(selectedChannel.cid);
                }
              }}
            >
              <LogIn size={13} /> Beitreten
            </button>
          )}

          {isModeratedForSelf && (
            <button
              className="btn-primary-sm full-span"
              onClick={() => {
                const msg = window.prompt(
                  'Nachricht für Talk-Power-Anfrage:',
                  'Ich möchte bitte sprechen'
                );
                if (msg !== null) void blackTeaStore.requestTalkPower(msg);
              }}
            >
              <Mic size={13} /> Talk-Power anfordern
            </button>
          )}

          <button
            className="btn-secondary-sm"
            disabled={!caps.isConnected}
            onClick={() => {
              blackTeaStore.patchState({ activeModal: 'files' });
              blackTeaStore.fetchChannelFiles(selectedChannel.cid, '/');
            }}
          >
            <FolderOpen size={13} /> Dateien
          </button>

          {caps.canEditChannel && (
            <button
              className="btn-secondary-sm"
              onClick={() => blackTeaStore.patchState({ activeModal: 'channel_edit' })}
            >
              <Edit3 size={13} /> Bearbeiten
            </button>
          )}

          {caps.canManagePermissions && (
            <button
              className="btn-secondary-sm"
              onClick={() => blackTeaStore.patchState({ activeModal: 'permissions' })}
            >
              <Shield size={13} /> Channel-Rechte
            </button>
          )}

          {caps.canDeleteChannel && !selectedChannel.channel_flag_default && (
            <button
              className="btn-danger-sm"
              onClick={() => {
                if (
                  window.confirm(
                    `Channel '${selectedChannel.channel_name}' wirklich löschen?`
                  )
                ) {
                  void blackTeaStore.deleteChannel(selectedChannel.cid);
                }
              }}
            >
              <Trash2 size={13} /> Löschen
            </button>
          )}
        </div>
      </div>
    </aside>
  );
};
