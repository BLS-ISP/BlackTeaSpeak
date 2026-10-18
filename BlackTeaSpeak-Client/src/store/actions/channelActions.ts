import { soundCues } from '../../media/SoundCueEngine';
import { ChannelFormPayload } from '../../types/protocol';
import { StoreContext } from '../state/blackTeaStateTypes';

export interface CreateChannelParams {
  name: string;
  topic?: string;
  description?: string;
  pid?: number;
  order?: number;
  permanent?: boolean;
  semiPermanent?: boolean;
  isDefault?: boolean;
  codec?: number;
  codecQuality?: number;
  maxclients?: number;
  maxClients?: number;
  maxFamilyClients?: number;
  neededTalkPower?: number;
  neededJoinPower?: number;
  neededSubscribePower?: number;
  neededDescriptionViewPower?: number;
  neededModifyPower?: number;
  neededDeletePower?: number;
  iconId?: number;
  password?: string;
}

export async function createChannelAction(
  ctx: StoreContext,
  params: CreateChannelParams
): Promise<void> {
  const maxClients = params.maxClients ?? params.maxclients ?? -1;
  const maxFamilyClients = params.maxFamilyClients ?? -1;
  const isDefault = Boolean(params.isDefault);
  const permanent = isDefault ? true : Boolean(params.permanent);
  const semiPermanent = !permanent && Boolean(params.semiPermanent);

  const payload: Record<string, string> = {
    channel_name: params.name,
    channel_topic: params.topic ?? '',
    channel_description: params.description ?? '',
    cpid: String(params.pid ?? 0),
    channel_order: String(params.order ?? 0),
    channel_flag_permanent: permanent ? '1' : '0',
    channel_flag_semi_permanent: semiPermanent ? '1' : '0',
    channel_flag_default: isDefault ? '1' : '0',
    channel_maxclients: String(maxClients),
    channel_maxfamilyclients: String(maxFamilyClients),
    channel_flag_maxclients_unlimited: maxClients < 0 ? '1' : '0',
    channel_flag_maxfamilyclients_unlimited: maxFamilyClients < 0 ? '1' : '0',
    channel_needed_talk_power: String(params.neededTalkPower ?? 0),
    i_channel_needed_join_power: String(params.neededJoinPower ?? 0),
    i_channel_needed_subscribe_power: String(params.neededSubscribePower ?? 0),
    i_channel_needed_description_view_power: String(params.neededDescriptionViewPower ?? 0),
    i_channel_needed_modify_power: String(params.neededModifyPower ?? 0),
    i_channel_needed_delete_power: String(params.neededDeletePower ?? 0),
    channel_icon_id: String(params.iconId ?? 0),
    channel_password: params.password ?? '',
    channel_codec: String(params.codec ?? 4),
    channel_codec_quality: String(params.codecQuality ?? 10),
  };

  await ctx.protocol.sendCommand('channelcreate', [payload]);
  await ctx.protocol.sendCommand('channellist').catch(() => {});
  if (!ctx.getState().outputDeafened) soundCues.play('channel_created');
  ctx.showToast(`Channel '${params.name}' erstellt`);
}

export async function editChannelAction(
  ctx: StoreContext,
  cid: number,
  params: Partial<ChannelFormPayload> & {
    maxclients?: number;
  }
): Promise<void> {
  const state = ctx.getState();
  const existing = state.channels.find((c) => c.cid === cid);
  const maxClients =
    params.maxClients !== undefined
      ? params.maxClients
      : params.maxclients !== undefined
      ? params.maxclients
      : (existing?.channel_maxclients ?? -1);
  const maxFamilyClients =
    params.maxFamilyClients !== undefined
      ? params.maxFamilyClients
      : (existing?.channel_maxfamilyclients ?? -1);

  const payload: Record<string, string> = {
    cid: String(cid),
  };

  if (params.name !== undefined) payload.channel_name = params.name;
  if (params.topic !== undefined) payload.channel_topic = params.topic;
  if (params.description !== undefined) payload.channel_description = params.description;
  if (params.pid !== undefined) payload.cpid = String(params.pid);
  if (params.order !== undefined) payload.channel_order = String(params.order);
  if (params.codec !== undefined) payload.channel_codec = String(params.codec);
  if (params.codecQuality !== undefined) {
    payload.channel_codec_quality = String(params.codecQuality);
  }

  payload.channel_maxclients = String(maxClients);
  payload.channel_maxfamilyclients = String(maxFamilyClients);
  payload.channel_flag_maxclients_unlimited = maxClients < 0 ? '1' : '0';
  payload.channel_flag_maxfamilyclients_unlimited = maxFamilyClients < 0 ? '1' : '0';

  if (
    params.isDefault !== undefined ||
    params.permanent !== undefined ||
    params.semiPermanent !== undefined
  ) {
    const isDef = params.isDefault ?? existing?.channel_flag_default ?? false;
    const isPerm = isDef ? true : (params.permanent ?? existing?.channel_flag_permanent ?? true);
    const isSemi =
      !isPerm && (params.semiPermanent ?? existing?.channel_flag_semi_permanent ?? false);
    payload.channel_flag_permanent = isPerm ? '1' : '0';
    payload.channel_flag_semi_permanent = isSemi ? '1' : '0';
    if (isDef) {
      payload.channel_flag_default = '1';
    }
  }

  if (params.neededTalkPower !== undefined) {
    payload.channel_needed_talk_power = String(params.neededTalkPower);
  }
  if (params.neededJoinPower !== undefined) {
    payload.i_channel_needed_join_power = String(params.neededJoinPower);
  }
  if (params.neededSubscribePower !== undefined) {
    payload.i_channel_needed_subscribe_power = String(params.neededSubscribePower);
  }
  if (params.neededDescriptionViewPower !== undefined) {
    payload.i_channel_needed_description_view_power = String(
      params.neededDescriptionViewPower
    );
  }
  if (params.neededModifyPower !== undefined) {
    payload.i_channel_needed_modify_power = String(params.neededModifyPower);
  }
  if (params.neededDeletePower !== undefined) {
    payload.i_channel_needed_delete_power = String(params.neededDeletePower);
  }
  if (params.iconId !== undefined) {
    payload.channel_icon_id = String(params.iconId);
  }

  if (params.clearPassword) {
    payload.channel_password = '';
    payload.channel_flag_password = '0';
  } else if (params.password && params.password.trim().length > 0) {
    payload.channel_password = params.password;
    payload.channel_flag_password = '1';
  }

  await ctx.protocol.sendCommand('channeledit', [payload]);
  await ctx.protocol.sendCommand('channellist').catch(() => {});
  if (!ctx.getState().outputDeafened) soundCues.play('channel_edited');
  ctx.showToast(`Channel '${params.name ?? existing?.channel_name ?? cid}' aktualisiert`);
}

export async function moveChannelAction(
  ctx: StoreContext,
  cid: number,
  cpid: number,
  order = 0
): Promise<void> {
  await ctx.protocol.sendCommand('channelmove', [
    {
      cid: String(cid),
      cpid: String(cpid),
      order: String(order),
    },
  ]);
  await ctx.protocol.sendCommand('channellist').catch(() => {});
  if (!ctx.getState().outputDeafened) soundCues.play('channel_edited');
  ctx.showToast(`Channel #${cid} verschoben`);
}

export async function deleteChannelAction(ctx: StoreContext, cid: number): Promise<void> {
  await ctx.protocol.sendCommand('channeldelete', [{ cid: String(cid), force: '1' }]);
  if (!ctx.getState().outputDeafened) soundCues.play('channel_deleted');
  ctx.showToast(`Channel #${cid} gelöscht`);
}
