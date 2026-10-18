import { soundCues } from '../../media/SoundCueEngine';
import {
  parseChannelRow,
  parseServerInitRow,
} from '../parsers/protocolRowParsers';
import { StoreContext } from '../state/blackTeaStateTypes';
import { handleClientAndChatFrame } from './clientAndChatFrameHandlers';

export function handleChannelAndClientFrame(
  ctx: StoreContext,
  cmd: string,
  rows: Record<string, string | number | undefined>[]
): boolean {
  const state = ctx.getState();

  switch (cmd) {
    case 'initserver': {
      const r = rows[0];
      const selfClid = Number(r.aclid ?? r.client_id ?? state.selfClientId);
      const info = parseServerInitRow(r);
      ctx.patchState({
        serverInfo: info,
        selfClientId: selfClid || state.selfClientId,
      });
      if (info.virtualserver_welcomemessage) {
        ctx.appendSystemChat(info.virtualserver_welcomemessage);
      }
      return true;
    }

    case 'notifyserveredited': {
      const r = rows[0];
      if (r) {
        ctx.patchState({
          serverInfo: {
            ...state.serverInfo,
            ...(r.virtualserver_name !== undefined
              ? { virtualserver_name: String(r.virtualserver_name) }
              : {}),
            ...(r.virtualserver_welcomemessage !== undefined
              ? { virtualserver_welcomemessage: String(r.virtualserver_welcomemessage) }
              : {}),
            ...(r.virtualserver_maxclients !== undefined
              ? { virtualserver_maxclients: Number(r.virtualserver_maxclients) }
              : {}),
            ...(r.virtualserver_hostbanner_url !== undefined
              ? { virtualserver_hostbanner_url: String(r.virtualserver_hostbanner_url) }
              : {}),
            ...(r.virtualserver_hostbanner_gfx_url !== undefined
              ? { virtualserver_hostbanner_gfx_url: String(r.virtualserver_hostbanner_gfx_url) }
              : {}),
            ...(r.virtualserver_hostbutton_tooltip !== undefined
              ? { virtualserver_hostbutton_tooltip: String(r.virtualserver_hostbutton_tooltip) }
              : {}),
            ...(r.virtualserver_hostbutton_url !== undefined
              ? { virtualserver_hostbutton_url: String(r.virtualserver_hostbutton_url) }
              : {}),
            ...(r.virtualserver_hostmessage !== undefined
              ? { virtualserver_hostmessage: String(r.virtualserver_hostmessage) }
              : {}),
            ...(r.virtualserver_hostmessage_mode !== undefined
              ? { virtualserver_hostmessage_mode: Number(r.virtualserver_hostmessage_mode) }
              : {}),
            ...(r.virtualserver_codec_encryption_mode !== undefined
              ? {
                  virtualserver_codec_encryption_mode: Number(
                    r.virtualserver_codec_encryption_mode
                  ),
                }
              : {}),
            ...(r.virtualserver_default_server_group !== undefined
              ? {
                  virtualserver_default_server_group: Number(
                    r.virtualserver_default_server_group
                  ),
                }
              : {}),
            ...(r.virtualserver_default_channel_group !== undefined
              ? {
                  virtualserver_default_channel_group: Number(
                    r.virtualserver_default_channel_group
                  ),
                }
              : {}),
          },
        });
      }
      return true;
    }

    case 'channellist':
    case 'notifychannellist':
    case 'notifychannelcreated': {
      const incoming = rows.map((r) => parseChannelRow(r));
      const map = new Map(state.channels.map((c) => [c.cid, c]));
      for (const ch of incoming) {
        map.set(ch.cid, ch);
      }
      ctx.patchState({
        channels: Array.from(map.values()).sort(
          (a, b) => a.pid - b.pid || a.channel_order - b.channel_order || a.cid - b.cid
        ),
      });
      return true;
    }

    case 'notifychanneledited':
    case 'notifychannelmoved': {
      const map = new Map(state.channels.map((c) => [c.cid, c]));
      let currentChannelEditedByPeer = false;
      let newDefaultChannelId: number | null = null;
      for (const r of rows) {
        const cid = Number(r.cid ?? 0);
        const invokerid = Number(r.invokerid ?? 0);
        if (
          cid === state.currentChannelId &&
          invokerid > 0 &&
          invokerid !== state.selfClientId
        ) {
          currentChannelEditedByPeer = true;
        }
        const existing = map.get(cid);
        if (existing) {
          const isNewDefault =
            r.channel_flag_default !== undefined
              ? Number(r.channel_flag_default) === 1
              : existing.channel_flag_default;
          if (isNewDefault) {
            newDefaultChannelId = cid;
          }
          map.set(cid, {
            ...existing,
            pid:
              r.pid !== undefined
                ? Number(r.pid)
                : r.cpid !== undefined
                ? Number(r.cpid)
                : existing.pid,
            channel_order:
              r.channel_order !== undefined
                ? Number(r.channel_order)
                : r.order !== undefined
                ? Number(r.order)
                : existing.channel_order,
            channel_name:
              r.channel_name !== undefined ? String(r.channel_name) : existing.channel_name,
            channel_topic:
              r.channel_topic !== undefined ? String(r.channel_topic) : existing.channel_topic,
            channel_description:
              r.channel_description !== undefined
                ? String(r.channel_description)
                : existing.channel_description,
            channel_codec:
              r.channel_codec !== undefined
                ? Number(r.channel_codec)
                : existing.channel_codec,
            channel_codec_quality:
              r.channel_codec_quality !== undefined
                ? Number(r.channel_codec_quality)
                : existing.channel_codec_quality,
            channel_maxclients:
              r.channel_maxclients !== undefined
                ? Number(r.channel_maxclients)
                : existing.channel_maxclients,
            channel_maxfamilyclients:
              r.channel_maxfamilyclients !== undefined
                ? Number(r.channel_maxfamilyclients)
                : existing.channel_maxfamilyclients,
            channel_flag_permanent:
              r.channel_flag_permanent !== undefined
                ? Number(r.channel_flag_permanent) === 1
                : existing.channel_flag_permanent,
            channel_flag_semi_permanent:
              r.channel_flag_semi_permanent !== undefined
                ? Number(r.channel_flag_semi_permanent) === 1
                : existing.channel_flag_semi_permanent,
            channel_flag_default: isNewDefault,
            channel_flag_password:
              r.channel_flag_password !== undefined
                ? Number(r.channel_flag_password) === 1
                : existing.channel_flag_password,
            channel_needed_talk_power:
              r.channel_needed_talk_power !== undefined
                ? Number(r.channel_needed_talk_power)
                : existing.channel_needed_talk_power,
            i_channel_needed_join_power:
              r.i_channel_needed_join_power !== undefined
                ? Number(r.i_channel_needed_join_power)
                : existing.i_channel_needed_join_power,
            i_channel_needed_subscribe_power:
              r.i_channel_needed_subscribe_power !== undefined
                ? Number(r.i_channel_needed_subscribe_power)
                : existing.i_channel_needed_subscribe_power,
            i_channel_needed_description_view_power:
              r.i_channel_needed_description_view_power !== undefined
                ? Number(r.i_channel_needed_description_view_power)
                : existing.i_channel_needed_description_view_power,
            i_channel_needed_modify_power:
              r.i_channel_needed_modify_power !== undefined
                ? Number(r.i_channel_needed_modify_power)
                : existing.i_channel_needed_modify_power,
            i_channel_needed_delete_power:
              r.i_channel_needed_delete_power !== undefined
                ? Number(r.i_channel_needed_delete_power)
                : existing.i_channel_needed_delete_power,
            channel_icon_id:
              r.channel_icon_id !== undefined
                ? Number(r.channel_icon_id)
                : existing.channel_icon_id,
          });
        }
      }
      if (newDefaultChannelId !== null) {
        for (const [k, ch] of map.entries()) {
          if (k !== newDefaultChannelId && ch.channel_flag_default) {
            map.set(k, { ...ch, channel_flag_default: false });
          }
        }
      }
      ctx.patchState({
        channels: Array.from(map.values()).sort(
          (a, b) => a.pid - b.pid || a.channel_order - b.channel_order || a.cid - b.cid
        ),
      });
      if (currentChannelEditedByPeer && !state.outputDeafened) {
        soundCues.play('your_channel_was_edited');
      }
      return true;
    }

    case 'notifychanneldeleted': {
      const deletedIds = new Set(rows.map((r) => Number(r.cid ?? 0)));
      ctx.patchState({
        channels: state.channels.filter((c) => !deletedIds.has(c.cid)),
      });
      return true;
    }

    default:
      return handleClientAndChatFrame(ctx, cmd, rows);
  }
}
