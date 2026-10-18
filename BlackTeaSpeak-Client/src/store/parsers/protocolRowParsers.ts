import { ChannelInfo, ClientInfo, ServerInfo } from '../../types/protocol';

export function parseServerInitRow(r: Record<string, string | number | undefined>): ServerInfo {
  return {
    virtualserver_id: Number(r.virtualserver_id ?? 1),
    virtualserver_name: String(r.virtualserver_name ?? 'BlackTeaSpeak Server'),
    virtualserver_welcomemessage: String(r.virtualserver_welcomemessage ?? ''),
    virtualserver_maxclients: Number(r.virtualserver_maxclients ?? 64),
    virtualserver_clientsonline: Number(r.virtualserver_clientsonline ?? 1),
    virtualserver_channelsonline: Number(r.virtualserver_channelsonline ?? 1),
    virtualserver_uptime: Number(r.virtualserver_uptime ?? 0),
    virtualserver_platform: String(r.virtualserver_platform ?? 'Rust / HTTP3'),
    virtualserver_version: String(r.virtualserver_version ?? 'BlackTeaSpeak'),
    virtualserver_hostbanner_url: String(r.virtualserver_hostbanner_url ?? ''),
    virtualserver_hostbanner_gfx_url: String(r.virtualserver_hostbanner_gfx_url ?? ''),
    virtualserver_hostbutton_tooltip: String(r.virtualserver_hostbutton_tooltip ?? ''),
    virtualserver_hostbutton_url: String(r.virtualserver_hostbutton_url ?? ''),
    virtualserver_hostmessage: String(r.virtualserver_hostmessage ?? ''),
    virtualserver_hostmessage_mode: Number(r.virtualserver_hostmessage_mode ?? 0),
    virtualserver_codec_encryption_mode: Number(r.virtualserver_codec_encryption_mode ?? 2),
    virtualserver_default_server_group: Number(r.virtualserver_default_server_group ?? 8),
    virtualserver_default_channel_group: Number(r.virtualserver_default_channel_group ?? 8),
    virtualserver_antiflood_points_tick_reduce: Number(
      r.virtualserver_antiflood_points_tick_reduce ?? 5
    ),
    virtualserver_antiflood_points_needed_command_block: Number(
      r.virtualserver_antiflood_points_needed_command_block ?? 150
    ),
    virtualserver_antiflood_points_needed_ip_block: Number(
      r.virtualserver_antiflood_points_needed_ip_block ?? 250
    ),
  };
}

export function parseChannelRow(r: Record<string, string | number | undefined>): ChannelInfo {
  const maxClients = Number(r.channel_maxclients ?? -1);
  const maxFamilyClients = Number(r.channel_maxfamilyclients ?? -1);
  return {
    cid: Number(r.cid ?? 0),
    pid: Number(r.pid ?? r.cpid ?? 0),
    channel_order: Number(r.channel_order ?? r.order ?? 0),
    channel_name: String(r.channel_name ?? 'Channel'),
    channel_topic: String(r.channel_topic ?? ''),
    channel_description: String(r.channel_description ?? ''),
    channel_codec: Number(r.channel_codec ?? 4),
    channel_codec_quality: Number(r.channel_codec_quality ?? 6),
    channel_maxclients: maxClients,
    channel_maxfamilyclients: maxFamilyClients,
    channel_flag_permanent: Number(r.channel_flag_permanent ?? 1) === 1,
    channel_flag_semi_permanent: Number(r.channel_flag_semi_permanent ?? 0) === 1,
    channel_flag_default: Number(r.channel_flag_default ?? 0) === 1,
    channel_flag_password: Number(r.channel_flag_password ?? 0) === 1,
    channel_flag_maxclients_unlimited:
      r.channel_flag_maxclients_unlimited !== undefined
        ? Number(r.channel_flag_maxclients_unlimited) === 1
        : maxClients < 0,
    channel_flag_maxfamilyclients_unlimited:
      r.channel_flag_maxfamilyclients_unlimited !== undefined
        ? Number(r.channel_flag_maxfamilyclients_unlimited) === 1
        : maxFamilyClients < 0,
    channel_flag_maxfamilyclients_inherited:
      r.channel_flag_maxfamilyclients_inherited !== undefined
        ? Number(r.channel_flag_maxfamilyclients_inherited) === 1
        : maxFamilyClients < 0,
    channel_needed_talk_power: Number(r.channel_needed_talk_power ?? 0),
    i_channel_needed_join_power: Number(r.i_channel_needed_join_power ?? 0),
    i_channel_needed_subscribe_power: Number(r.i_channel_needed_subscribe_power ?? 0),
    i_channel_needed_description_view_power: Number(
      r.i_channel_needed_description_view_power ?? 0
    ),
    i_channel_needed_modify_power: Number(r.i_channel_needed_modify_power ?? 0),
    i_channel_needed_delete_power: Number(r.i_channel_needed_delete_power ?? 0),
    channel_icon_id: Number(r.channel_icon_id ?? 0),
    total_clients: Number(r.total_clients ?? 0),
    total_clients_family: Number(r.total_clients_family ?? r.total_clients ?? 0),
  };
}

export function parseClientRow(
  r: Record<string, string | number | undefined>,
  existing?: ClientInfo
): ClientInfo {
  const sgRaw = r.client_servergroups !== undefined ? String(r.client_servergroups) : undefined;
  const serverGroups = sgRaw
    ? sgRaw
        .split(',')
        .map((s) => Number(s.trim()))
        .filter((n) => !Number.isNaN(n) && n > 0)
    : (existing?.client_servergroups ?? [8]);

  return {
    clid: Number(r.clid ?? existing?.clid ?? 0),
    cid: Number(r.cid ?? r.ctid ?? existing?.cid ?? 1),
    client_database_id: Number(r.client_database_id ?? existing?.client_database_id ?? 0),
    client_nickname: String(r.client_nickname ?? existing?.client_nickname ?? 'User'),
    client_unique_identifier: String(
      r.client_unique_identifier ?? existing?.client_unique_identifier ?? ''
    ),
    client_type: Number(r.client_type ?? existing?.client_type ?? 0),
    client_away:
      r.client_away !== undefined ? Number(r.client_away) === 1 : (existing?.client_away ?? false),
    client_away_message: String(r.client_away_message ?? existing?.client_away_message ?? ''),
    client_input_muted:
      r.client_input_muted !== undefined
        ? Number(r.client_input_muted) === 1
        : (existing?.client_input_muted ?? false),
    client_output_muted:
      r.client_output_muted !== undefined
        ? Number(r.client_output_muted) === 1
        : (existing?.client_output_muted ?? false),
    client_input_hardware:
      r.client_input_hardware !== undefined
        ? Number(r.client_input_hardware) === 1
        : (existing?.client_input_hardware ?? true),
    client_output_hardware:
      r.client_output_hardware !== undefined
        ? Number(r.client_output_hardware) === 1
        : (existing?.client_output_hardware ?? true),
    client_talk_power: Number(r.client_talk_power ?? existing?.client_talk_power ?? 0),
    client_is_talker:
      r.client_is_talker !== undefined
        ? Number(r.client_is_talker) === 1
        : (existing?.client_is_talker ?? false),
    client_is_priority_speaker:
      r.client_is_priority_speaker !== undefined
        ? Number(r.client_is_priority_speaker) === 1
        : (existing?.client_is_priority_speaker ?? false),
    client_is_recording:
      r.client_is_recording !== undefined
        ? Number(r.client_is_recording) === 1
        : (existing?.client_is_recording ?? false),
    client_is_channel_commander:
      r.client_is_channel_commander !== undefined
        ? Number(r.client_is_channel_commander) === 1
        : (existing?.client_is_channel_commander ?? false),
    client_talk_request:
      r.client_talk_request !== undefined
        ? Number(r.client_talk_request)
        : (existing?.client_talk_request ?? 0),
    client_talk_request_msg: String(
      r.client_talk_request_msg ?? existing?.client_talk_request_msg ?? ''
    ),
    client_servergroups: serverGroups,
    client_channel_group_id: Number(
      r.client_channel_group_id ?? existing?.client_channel_group_id ?? 8
    ),
    client_version: String(r.client_version ?? existing?.client_version ?? 'BlackTeaWeb 2.0'),
    client_platform: String(r.client_platform ?? existing?.client_platform ?? 'Web (HTTP/3)'),
    client_country: String(r.client_country ?? existing?.client_country ?? 'DE'),
    client_description: String(r.client_description ?? existing?.client_description ?? ''),
    isTalking: existing?.isTalking ?? false,
    isWhispering: existing?.isWhispering ?? false,
    volumeGain: existing?.volumeGain ?? 1.0,
    localMuted: existing?.localMuted ?? false,
    spatialPos: existing?.spatialPos ?? {
      x: Math.cos((Number(r.clid ?? 1) * 1.3) % (Math.PI * 2)) * 2.5,
      y: 0,
      z: Math.sin((Number(r.clid ?? 1) * 1.3) % (Math.PI * 2)) * 2.5,
    },
  };
}
