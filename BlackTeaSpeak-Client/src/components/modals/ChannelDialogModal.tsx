import React, { useMemo, useState } from 'react';
import {
  FileText,
  FolderTree,
  Hash,
  Save,
  Shield,
  Volume2,
  X,
} from 'lucide-react';
import { blackTeaStore, useBlackTeaStore } from '../../store/useBlackTeaStore';
import { AUDIO_CODECS, CodecSpec, estimateCodecBitrate } from './channel-editor/codecSpecs';
import { ChannelGeneralTab } from './channel-editor/ChannelGeneralTab';
import { ChannelAudioQualityTab } from './channel-editor/ChannelAudioQualityTab';
import {
  ChannelHierarchyTab,
  ChannelTypeOption,
} from './channel-editor/ChannelHierarchyTab';
import { ChannelLimitsPermissionsTab } from './channel-editor/ChannelLimitsPermissionsTab';

export type { CodecSpec };
export { AUDIO_CODECS, estimateCodecBitrate };

type ChannelEditorTab = 'general' | 'audio' | 'hierarchy' | 'limits_permissions';

export const ChannelDialogModal: React.FC<{
  mode: 'create' | 'edit';
  onClose: () => void;
}> = ({ mode, onClose }) => {
  const state = useBlackTeaStore();
  const targetChannel =
    mode === 'edit'
      ? state.channels.find(
          (c) => c.cid === (state.selectedChannelId ?? state.currentChannelId)
        )
      : undefined;

  const [activeTab, setActiveTab] = useState<ChannelEditorTab>('general');
  const [submitting, setSubmitting] = useState(false);
  const [errorMsg, setErrorMsg] = useState<string | null>(null);

  // Tab 1: General
  const [name, setName] = useState(targetChannel?.channel_name ?? '');
  const [topic, setTopic] = useState(targetChannel?.channel_topic ?? '');
  const [description, setDescription] = useState(
    targetChannel?.channel_description ?? ''
  );
  const [password, setPassword] = useState('');
  const [clearPassword, setClearPassword] = useState(false);
  const [iconId, setIconId] = useState<number>(targetChannel?.channel_icon_id ?? 0);

  // Tab 2: Audio & Quality
  const [codec, setCodec] = useState<number>(targetChannel?.channel_codec ?? 4);
  const [codecQuality, setCodecQuality] = useState<number>(
    targetChannel?.channel_codec_quality ?? 6
  );

  // Tab 3: Type & Hierarchy
  const initialChannelType: ChannelTypeOption = targetChannel
    ? targetChannel.channel_flag_default
      ? 'default'
      : targetChannel.channel_flag_permanent
      ? 'permanent'
      : targetChannel.channel_flag_semi_permanent
      ? 'semi_permanent'
      : 'temporary'
    : 'permanent';

  const [channelType, setChannelType] = useState<ChannelTypeOption>(initialChannelType);
  const [pid, setPid] = useState<number>(targetChannel?.pid ?? 0);
  const [order, setOrder] = useState<number>(targetChannel?.channel_order ?? 0);

  // Tab 4: Limits & Permissions
  const [maxClientsUnlimited, setMaxClientsUnlimited] = useState<boolean>(
    targetChannel ? targetChannel.channel_maxclients < 0 : true
  );
  const [maxClientsValue, setMaxClientsValue] = useState<number>(
    targetChannel && targetChannel.channel_maxclients >= 0
      ? targetChannel.channel_maxclients
      : 16
  );

  const [maxFamilyUnlimited, setMaxFamilyUnlimited] = useState<boolean>(
    targetChannel ? targetChannel.channel_maxfamilyclients < 0 : true
  );
  const [maxFamilyValue, setMaxFamilyValue] = useState<number>(
    targetChannel && targetChannel.channel_maxfamilyclients >= 0
      ? targetChannel.channel_maxfamilyclients
      : 32
  );

  const [neededTalkPower, setNeededTalkPower] = useState<number>(
    targetChannel?.channel_needed_talk_power ?? 0
  );
  const [neededJoinPower, setNeededJoinPower] = useState<number>(
    targetChannel?.i_channel_needed_join_power ?? 0
  );
  const [neededSubscribePower, setNeededSubscribePower] = useState<number>(
    targetChannel?.i_channel_needed_subscribe_power ?? 0
  );
  const [neededDescViewPower, setNeededDescViewPower] = useState<number>(
    targetChannel?.i_channel_needed_description_view_power ?? 0
  );
  const [neededModifyPower, setNeededModifyPower] = useState<number>(
    targetChannel?.i_channel_needed_modify_power ?? 0
  );
  const [neededDeletePower, setNeededDeletePower] = useState<number>(
    targetChannel?.i_channel_needed_delete_power ?? 0
  );

  const forbiddenParentIds = useMemo(() => {
    const set = new Set<number>();
    if (mode !== 'edit' || !targetChannel) return set;
    set.add(targetChannel.cid);
    let added = true;
    while (added) {
      added = false;
      for (const ch of state.channels) {
        if (set.has(ch.pid) && !set.has(ch.cid)) {
          set.add(ch.cid);
          added = true;
        }
      }
    }
    return set;
  }, [mode, targetChannel, state.channels]);

  const availableParentChannels = useMemo(
    () => state.channels.filter((ch) => !forbiddenParentIds.has(ch.cid)),
    [state.channels, forbiddenParentIds]
  );

  const siblingChannels = useMemo(
    () =>
      state.channels
        .filter(
          (ch) =>
            ch.pid === pid && (mode === 'create' || ch.cid !== targetChannel?.cid)
        )
        .sort((a, b) => a.channel_order - b.channel_order || a.cid - b.cid),
    [state.channels, pid, mode, targetChannel]
  );

  const selectedCodecSpec =
    AUDIO_CODECS.find((c) => c.id === codec) ?? AUDIO_CODECS[4];
  const bitrateInfo = estimateCodecBitrate(codec, codecQuality);

  const handleParentChange = (nextPid: number) => {
    setPid(nextPid);
    setOrder(0);
  };

  const applyAudioPreset = (presetCodec: number, presetQuality: number) => {
    setCodec(presetCodec);
    setCodecQuality(presetQuality);
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!name.trim()) {
      setErrorMsg('Bitte gib einen gültigen Channel-Namen ein.');
      setActiveTab('general');
      return;
    }

    setSubmitting(true);
    setErrorMsg(null);

    const isDefault = channelType === 'default';
    const permanent = isDefault || channelType === 'permanent';
    const semiPermanent = channelType === 'semi_permanent';
    const finalMaxClients = maxClientsUnlimited ? -1 : Math.max(0, maxClientsValue);
    const finalMaxFamilyClients = maxFamilyUnlimited ? -1 : Math.max(0, maxFamilyValue);

    try {
      if (mode === 'create') {
        await blackTeaStore.createChannel({
          name: name.trim(),
          topic,
          description,
          pid,
          order,
          permanent,
          semiPermanent,
          isDefault,
          codec,
          codecQuality,
          maxClients: finalMaxClients,
          maxFamilyClients: finalMaxFamilyClients,
          neededTalkPower,
          neededJoinPower,
          neededSubscribePower,
          neededDescriptionViewPower: neededDescViewPower,
          neededModifyPower,
          neededDeletePower,
          iconId,
          password: password.trim(),
        });
      } else if (targetChannel) {
        await blackTeaStore.editChannel(targetChannel.cid, {
          name: name.trim(),
          topic,
          description,
          pid,
          order,
          permanent,
          semiPermanent,
          isDefault,
          codec,
          codecQuality,
          maxClients: finalMaxClients,
          maxFamilyClients: finalMaxFamilyClients,
          neededTalkPower,
          neededJoinPower,
          neededSubscribePower,
          neededDescriptionViewPower: neededDescViewPower,
          neededModifyPower,
          neededDeletePower,
          iconId,
          password: password.trim() || undefined,
          clearPassword,
        });
      }
      onClose();
    } catch (err: any) {
      setErrorMsg(err?.message || 'Speichern fehlgeschlagen.');
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div className="modal-card modal-wide">
      <div className="modal-header">
        <div className="modal-title">
          <Hash size={18} />
          <span>
            {mode === 'create'
              ? 'Neuen Channel erstellen'
              : `Channel bearbeiten — ${targetChannel?.channel_name ?? ''} (#${targetChannel?.cid ?? ''})`}
          </span>
        </div>
        <button className="btn-icon-sm" onClick={onClose} title="Schließen">
          <X size={16} />
        </button>
      </div>

      <div
        style={{
          display: 'flex',
          gap: '6px',
          padding: '10px 18px',
          borderBottom: '1px solid rgba(255,255,255,0.08)',
          background: 'rgba(0,0,0,0.2)',
          flexWrap: 'wrap',
        }}
      >
        <button
          type="button"
          className={activeTab === 'general' ? 'btn-primary-sm' : 'btn-secondary-sm'}
          onClick={() => setActiveTab('general')}
        >
          <FileText size={14} /> Allgemein
        </button>
        <button
          type="button"
          className={activeTab === 'audio' ? 'btn-primary-sm' : 'btn-secondary-sm'}
          onClick={() => setActiveTab('audio')}
        >
          <Volume2 size={14} /> Audio & Qualität ({selectedCodecSpec.shortName} · Q{codecQuality})
        </button>
        <button
          type="button"
          className={activeTab === 'hierarchy' ? 'btn-primary-sm' : 'btn-secondary-sm'}
          onClick={() => setActiveTab('hierarchy')}
        >
          <FolderTree size={14} /> Typ & Hierarchie
        </button>
        <button
          type="button"
          className={
            activeTab === 'limits_permissions' ? 'btn-primary-sm' : 'btn-secondary-sm'
          }
          onClick={() => setActiveTab('limits_permissions')}
        >
          <Shield size={14} /> Limits & Rechte
        </button>
      </div>

      <form className="modal-body stack-form" onSubmit={handleSubmit}>
        {errorMsg && (
          <div
            style={{
              padding: '10px 12px',
              borderRadius: '8px',
              background: 'rgba(239, 68, 68, 0.16)',
              border: '1px solid rgba(239, 68, 68, 0.4)',
              color: '#fca5a5',
              fontSize: '12px',
            }}
          >
            {errorMsg}
          </div>
        )}

        {activeTab === 'general' && (
          <ChannelGeneralTab
            mode={mode}
            targetChannel={targetChannel}
            name={name}
            setName={setName}
            topic={topic}
            setTopic={setTopic}
            description={description}
            setDescription={setDescription}
            password={password}
            setPassword={setPassword}
            clearPassword={clearPassword}
            setClearPassword={setClearPassword}
            iconId={iconId}
            setIconId={setIconId}
          />
        )}

        {activeTab === 'audio' && (
          <ChannelAudioQualityTab
            codec={codec}
            setCodec={setCodec}
            codecQuality={codecQuality}
            setCodecQuality={setCodecQuality}
            selectedCodecSpec={selectedCodecSpec}
            bitrateInfo={bitrateInfo}
            applyAudioPreset={applyAudioPreset}
          />
        )}

        {activeTab === 'hierarchy' && (
          <ChannelHierarchyTab
            channelType={channelType}
            setChannelType={setChannelType}
            pid={pid}
            handleParentChange={handleParentChange}
            order={order}
            setOrder={setOrder}
            availableParentChannels={availableParentChannels}
            siblingChannels={siblingChannels}
          />
        )}

        {activeTab === 'limits_permissions' && (
          <ChannelLimitsPermissionsTab
            maxClientsUnlimited={maxClientsUnlimited}
            setMaxClientsUnlimited={setMaxClientsUnlimited}
            maxClientsValue={maxClientsValue}
            setMaxClientsValue={setMaxClientsValue}
            maxFamilyUnlimited={maxFamilyUnlimited}
            setMaxFamilyUnlimited={setMaxFamilyUnlimited}
            maxFamilyValue={maxFamilyValue}
            setMaxFamilyValue={setMaxFamilyValue}
            neededTalkPower={neededTalkPower}
            setNeededTalkPower={setNeededTalkPower}
            neededJoinPower={neededJoinPower}
            setNeededJoinPower={setNeededJoinPower}
            neededSubscribePower={neededSubscribePower}
            setNeededSubscribePower={setNeededSubscribePower}
            neededDescViewPower={neededDescViewPower}
            setNeededDescViewPower={setNeededDescViewPower}
            neededModifyPower={neededModifyPower}
            setNeededModifyPower={setNeededModifyPower}
            neededDeletePower={neededDeletePower}
            setNeededDeletePower={setNeededDeletePower}
          />
        )}

        <div className="modal-footer-actions">
          <button type="button" className="btn-secondary-sm" onClick={onClose}>
            Abbrechen
          </button>
          <button type="submit" className="btn-primary-sm" disabled={submitting}>
            <Save size={14} />{' '}
            {submitting
              ? 'Speichere...'
              : mode === 'create'
              ? 'Channel erstellen'
              : 'Änderungen speichern'}
          </button>
        </div>
      </form>
    </div>
  );
};
