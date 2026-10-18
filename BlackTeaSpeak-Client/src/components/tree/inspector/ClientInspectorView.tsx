import React from 'react';
import {
  AlertTriangle,
  ArrowRightLeft,
  Ban,
  Clock,
  Edit3,
  KeyRound,
  MessageSquare,
  Mic,
  Radio,
  ShieldCheck,
  User,
  UserMinus,
  Volume2,
  VolumeX,
  Zap,
} from 'lucide-react';
import {
  blackTeaStore,
  BlackTeaState,
  ClientCapabilities,
} from '../../../store/useBlackTeaStore';
import { ClientInfo } from '../../../types/protocol';

export interface ClientInspectorViewProps {
  state: BlackTeaState;
  caps: ClientCapabilities;
  selectedClient: ClientInfo;
}

export const ClientInspectorView: React.FC<ClientInspectorViewProps> = ({
  state,
  caps,
  selectedClient,
}) => {
  const isSelf = selectedClient.clid === state.selfClientId;
  const gainPercent = Math.round((selectedClient.volumeGain ?? 1.0) * 100);
  const assignedGroups = state.serverGroups.filter((sg) =>
    selectedClient.client_servergroups.includes(sg.sgid)
  );

  return (
    <aside className="inspector-panel">
      <div className="inspector-header">
        <div className="inspector-avatar">
          <User size={20} />
        </div>
        <div>
          <h3 className="inspector-title">
            {selectedClient.client_nickname}
            {isSelf && <span className="you-pill">Du</span>}
          </h3>
          <span className="inspector-subtitle">
            {selectedClient.client_platform} • {selectedClient.client_version}
          </span>
        </div>
      </div>

      <div className="inspector-section">
        <h4>Client-Details</h4>
        <div className="kv-list">
          <div className="kv-row">
            <span>Status</span>
            <span>
              {selectedClient.client_away
                ? `🌙 Abwesend${selectedClient.client_away_message ? ` (${selectedClient.client_away_message})` : ''}`
                : selectedClient.isTalking
                ? '🎙️ Spricht gerade'
                : selectedClient.client_output_muted
                ? '🔇 Sound stummgeschaltet'
                : selectedClient.client_input_muted
                ? '🎤 Mikrofon stumm'
                : '🟢 Verbunden'}
            </span>
          </div>
          <div className="kv-row">
            <span>Talk Power</span>
            <span>
              {selectedClient.client_talk_power}
              {selectedClient.client_is_talker && ' (Talker erteilt)'}
              {selectedClient.client_is_channel_commander && ' • Commander'}
            </span>
          </div>
          {(selectedClient.client_talk_request ?? 0) > 0 && (
            <div className="kv-row">
              <span>Talk-Anfrage</span>
              <span style={{ color: '#fbbf24', fontWeight: 600 }}>
                ✋ {selectedClient.client_talk_request_msg || 'Bittet um Sprachrecht'}
              </span>
            </div>
          )}
          {caps.isAdmin && (
            <>
              <div className="kv-row">
                <span>Datenbank-ID</span>
                <span>#{selectedClient.client_database_id}</span>
              </div>
              <div className="kv-row">
                <span>Unique ID</span>
                <code className="uid-code">
                  {selectedClient.client_unique_identifier || 'Web-Ed25519'}
                </code>
              </div>
            </>
          )}
        </div>
      </div>

      <div className="inspector-section">
        <div
          style={{
            display: 'flex',
            justifyContent: 'space-between',
            alignItems: 'center',
            marginBottom: '6px',
          }}
        >
          <h4 style={{ margin: 0 }}>Client-Beschreibung</h4>
          {(isSelf || caps.canModerate) && (
            <button
              className="btn-xs"
              onClick={() => {
                const nextDesc = window.prompt(
                  `Beschreibung für ${selectedClient.client_nickname}:`,
                  selectedClient.client_description || ''
                );
                if (nextDesc !== null) {
                  void blackTeaStore.editClientDescription(
                    selectedClient.clid,
                    nextDesc
                  );
                }
              }}
            >
              <Edit3 size={11} /> Bearbeiten
            </button>
          )}
        </div>
        <div className="inspector-description-box">
          {selectedClient.client_description || 'Keine Beschreibung hinterlegt.'}
        </div>
      </div>

      {isSelf && (
        <div className="inspector-section">
          <h4>Eigene Status-Funktionen</h4>
          <div className="inspector-action-grid">
            <button
              className={
                selectedClient.client_away ? 'btn-primary-sm' : 'btn-secondary-sm'
              }
              onClick={() => {
                if (selectedClient.client_away) {
                  void blackTeaStore.toggleAway(false, '');
                } else {
                  const msg = window.prompt(
                    'Abwesenheitsnotiz (optional):',
                    selectedClient.client_away_message || 'Kurz weg (AFK)'
                  );
                  if (msg !== null) {
                    void blackTeaStore.toggleAway(true, msg);
                  }
                }
              }}
            >
              <Clock size={13} />{' '}
              {selectedClient.client_away ? 'AFK beenden' : 'AFK setzen'}
            </button>

            <button
              className={
                selectedClient.client_is_channel_commander
                  ? 'btn-primary-sm'
                  : 'btn-secondary-sm'
              }
              onClick={() => void blackTeaStore.toggleChannelCommander()}
            >
              <Radio size={13} /> Commander
            </button>

            {(selectedClient.client_talk_request ?? 0) > 0 ? (
              <button
                className="btn-danger-sm full-span"
                onClick={() => void blackTeaStore.cancelTalkPowerRequest()}
              >
                <Mic size={13} /> Talk-Power-Anfrage zurückziehen
              </button>
            ) : (
              <button
                className="btn-secondary-sm full-span"
                onClick={() => {
                  const msg = window.prompt(
                    'Grund für Talk-Power-Anfrage:',
                    'Ich möchte bitte sprechen'
                  );
                  if (msg !== null) {
                    void blackTeaStore.requestTalkPower(msg);
                  }
                }}
              >
                <Mic size={13} /> Talk-Power anfordern
              </button>
            )}
          </div>
        </div>
      )}

      {!isSelf && (
        <div className="inspector-section">
          <h4>Lokale Lautstärke</h4>
          <div className="volume-control-box">
            <div className="volume-label-row">
              <span>Pegel ({gainPercent}%)</span>
              <button
                className={`btn-xs ${selectedClient.localMuted ? 'danger' : ''}`}
                onClick={() => blackTeaStore.togglePeerMute(selectedClient.clid)}
              >
                {selectedClient.localMuted ? (
                  <>
                    <VolumeX size={12} /> Stumm
                  </>
                ) : (
                  <>
                    <Volume2 size={12} /> Aktiv
                  </>
                )}
              </button>
            </div>
            <input
              type="range"
              min={0}
              max={200}
              value={gainPercent}
              onChange={(e) =>
                blackTeaStore.setPeerVolume(
                  selectedClient.clid,
                  Number(e.target.value) / 100
                )
              }
            />
          </div>

          {selectedClient.spatialPos && (
            <div className="spatial-coords-pill">
              3D Position: X={selectedClient.spatialPos.x.toFixed(1)}m, Z=
              {selectedClient.spatialPos.z.toFixed(1)}m
            </div>
          )}
        </div>
      )}

      <div className="inspector-section">
        <h4>Servergruppen</h4>
        {caps.canAssignGroups ? (
          <div className="group-chips">
            {state.serverGroups.map((sg) => {
              const hasGroup = selectedClient.client_servergroups.includes(sg.sgid);
              return (
                <button
                  key={sg.sgid}
                  className={`group-chip ${hasGroup ? 'assigned' : ''}`}
                  onClick={() =>
                    blackTeaStore.toggleClientServerGroup(
                      selectedClient.client_database_id,
                      sg.sgid,
                      hasGroup
                    )
                  }
                  title="Klicken, um Servergruppe zuzuweisen oder zu entfernen"
                >
                  {sg.name}
                </button>
              );
            })}
          </div>
        ) : (
          <div className="group-chips">
            {assignedGroups.length === 0 ? (
              <span className="readonly-group-pill">Normal</span>
            ) : (
              assignedGroups.map((sg) => (
                <span
                  key={sg.sgid}
                  className={`readonly-group-pill ${
                    sg.sgid === 6 || sg.sgid === 9 ? 'admin-pill' : ''
                  }`}
                >
                  {(sg.sgid === 6 || sg.sgid === 9) && <ShieldCheck size={12} />}
                  {sg.name}
                </span>
              ))
            )}
          </div>
        )}

        {isSelf && !caps.isAdmin && caps.isConnected && (
          <button
            className="btn-key-inline"
            onClick={() => blackTeaStore.patchState({ activeModal: 'redeem_key' })}
          >
            <KeyRound size={13} /> Berechtigungsschlüssel einlösen
          </button>
        )}
      </div>

      <div className="inspector-section">
        <h4>Channelgruppe</h4>
        {caps.canAssignGroups ? (
          <div className="group-chips">
            {state.channelGroups.map((cg) => {
              const isActive = selectedClient.client_channel_group_id === cg.cgid;
              return (
                <button
                  key={cg.cgid}
                  className={`group-chip ${isActive ? 'assigned' : ''}`}
                  onClick={() =>
                    blackTeaStore.setClientChannelGroup(
                      cg.cgid,
                      selectedClient.cid,
                      selectedClient.client_database_id
                    )
                  }
                  title="Klicken, um Channelgruppe im aktuellen Channel zu setzen"
                >
                  {cg.name}
                </button>
              );
            })}
          </div>
        ) : (
          <div className="group-chips">
            <span className="readonly-group-pill">
              {state.channelGroups.find(
                (cg) => cg.cgid === selectedClient.client_channel_group_id
              )?.name ?? 'Guest'}
            </span>
          </div>
        )}
      </div>

      {!isSelf && (
        <div className="inspector-section">
          <h4>Interaktion{caps.canModerate ? ' & Moderation' : ''}</h4>
          <div className="inspector-action-grid">
            <button
              className="btn-secondary-sm"
              onClick={() =>
                blackTeaStore.patchState({
                  activeChatTab: 'private',
                  privateChatTargetClientId: selectedClient.clid,
                })
              }
            >
              <MessageSquare size={13} /> Nachricht
            </button>

            <button
              className="btn-secondary-sm"
              onClick={() => {
                const msg = window.prompt('Poke-Nachricht:', 'Hey!');
                if (msg !== null) blackTeaStore.pokeClient(selectedClient.clid, msg);
              }}
            >
              <Zap size={13} /> Anstupsen
            </button>

            <button
              className="btn-secondary-sm full-span"
              onClick={() => {
                const msg = window.prompt(
                  `Beschwerde über ${selectedClient.client_nickname} einreichen:`,
                  ''
                );
                if (msg && msg.trim()) {
                  void blackTeaStore.submitComplaint(
                    selectedClient.client_database_id,
                    msg.trim()
                  );
                }
              }}
            >
              <AlertTriangle size={13} /> Beschwerde einreichen
            </button>

            {caps.canModerate && (
              <>
                <button
                  className={
                    selectedClient.client_is_talker
                      ? 'btn-primary-sm full-span'
                      : 'btn-secondary-sm full-span'
                  }
                  onClick={() =>
                    void blackTeaStore.setClientIsTalker(
                      selectedClient.clid,
                      !selectedClient.client_is_talker
                    )
                  }
                >
                  <Mic size={13} />{' '}
                  {selectedClient.client_is_talker
                    ? 'Talk-Power entziehen'
                    : 'Talk-Power erteilen (Talker)'}
                </button>

                <div className="full-span" style={{ marginTop: 2, marginBottom: 2 }}>
                  <label
                    style={{
                      display: 'block',
                      fontSize: '11px',
                      color: 'rgba(255,255,255,0.6)',
                      marginBottom: 4,
                    }}
                  >
                    <ArrowRightLeft
                      size={11}
                      style={{ verticalAlign: 'middle', marginRight: 4 }}
                    />
                    Client in Channel verschieben:
                  </label>
                  <select
                    value={selectedClient.cid}
                    onChange={(e) =>
                      void blackTeaStore.moveClientToChannel(
                        selectedClient.clid,
                        Number(e.target.value)
                      )
                    }
                    style={{ width: '100%' }}
                  >
                    {state.channels.map((ch) => (
                      <option key={ch.cid} value={ch.cid}>
                        {ch.channel_name} (#{ch.cid})
                      </option>
                    ))}
                  </select>
                </div>

                <button
                  className="btn-secondary-sm"
                  onClick={() => {
                    const reason = window.prompt('Grund für Channel-Kick:', '');
                    if (reason !== null)
                      blackTeaStore.kickClient(selectedClient.clid, false, reason);
                  }}
                >
                  <UserMinus size={13} /> Channel-Kick
                </button>

                <button
                  className="btn-danger-sm"
                  onClick={() => {
                    const reason = window.prompt('Grund für Server-Kick:', '');
                    if (reason !== null)
                      blackTeaStore.kickClient(selectedClient.clid, true, reason);
                  }}
                >
                  <UserMinus size={13} /> Server-Kick
                </button>

                <button
                  className="btn-danger-sm full-span"
                  onClick={() => {
                    const reason = window.prompt('Ban-Grund:', 'Regelverstoß');
                    if (reason !== null)
                      blackTeaStore.banClient(selectedClient.clid, 3600, reason);
                  }}
                >
                  <Ban size={13} /> 1 Stunde bannen
                </button>
              </>
            )}
          </div>
        </div>
      )}
    </aside>
  );
};
