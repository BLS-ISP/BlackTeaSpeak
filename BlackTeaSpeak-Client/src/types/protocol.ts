export type ConnectionState = 'disconnected' | 'connecting' | 'handshaking' | 'connected' | 'error';

export interface ServerInfo {
  virtualserver_id: number;
  virtualserver_name: string;
  virtualserver_welcomemessage: string;
  virtualserver_maxclients: number;
  virtualserver_clientsonline: number;
  virtualserver_channelsonline: number;
  virtualserver_uptime: number;
  virtualserver_platform: string;
  virtualserver_version: string;
  virtualserver_hostbanner_url: string;
  virtualserver_hostbanner_gfx_url: string;
  virtualserver_hostbutton_tooltip: string;
  virtualserver_hostbutton_url: string;
  virtualserver_hostmessage?: string;
  virtualserver_hostmessage_mode?: number;
  virtualserver_codec_encryption_mode: number;
  virtualserver_default_server_group: number;
  virtualserver_default_channel_group: number;
  virtualserver_antiflood_points_tick_reduce?: number;
  virtualserver_antiflood_points_needed_command_block?: number;
  virtualserver_antiflood_points_needed_ip_block?: number;
}

export interface ChannelInfo {
  cid: number;
  pid: number;
  channel_order: number;
  channel_name: string;
  channel_topic: string;
  channel_description: string;
  channel_codec: number;
  channel_codec_quality: number;
  channel_maxclients: number;
  channel_maxfamilyclients: number;
  channel_flag_permanent: boolean;
  channel_flag_semi_permanent: boolean;
  channel_flag_default: boolean;
  channel_flag_password: boolean;
  channel_flag_maxclients_unlimited?: boolean;
  channel_flag_maxfamilyclients_unlimited?: boolean;
  channel_flag_maxfamilyclients_inherited?: boolean;
  channel_needed_talk_power: number;
  i_channel_needed_join_power?: number;
  i_channel_needed_subscribe_power?: number;
  i_channel_needed_description_view_power?: number;
  i_channel_needed_modify_power?: number;
  i_channel_needed_delete_power?: number;
  channel_icon_id: number;
  total_clients: number;
  total_clients_family?: number;
}

export interface ChannelFormPayload {
  name: string;
  topic: string;
  description: string;
  password?: string;
  clearPassword?: boolean;
  pid: number;
  order: number;
  codec: number;
  codecQuality: number;
  maxClients: number;
  maxFamilyClients: number;
  permanent: boolean;
  semiPermanent: boolean;
  isDefault: boolean;
  neededTalkPower: number;
  neededJoinPower: number;
  neededSubscribePower: number;
  neededDescriptionViewPower: number;
  neededModifyPower: number;
  neededDeletePower: number;
  iconId: number;
}

export interface SpatialPosition {
  x: number;
  y: number;
  z: number;
}

export interface ClientInfo {
  clid: number;
  cid: number;
  client_database_id: number;
  client_nickname: string;
  client_unique_identifier: string;
  client_type: number; // 0 = regular/web, 1 = query, 2 = musicbot
  client_away: boolean;
  client_away_message: string;
  client_input_muted: boolean;
  client_output_muted: boolean;
  client_input_hardware: boolean;
  client_output_hardware: boolean;
  client_talk_power: number;
  client_is_talker: boolean;
  client_is_priority_speaker: boolean;
  client_is_recording: boolean;
  client_is_channel_commander: boolean;
  client_talk_request?: number;
  client_talk_request_msg?: string;
  client_servergroups: number[];
  client_channel_group_id: number;
  client_version: string;
  client_platform: string;
  client_country: string;
  client_description: string;
  client_avatar?: string;
  isTalking?: boolean;
  isWhispering?: boolean;
  volumeGain?: number;
  localMuted?: boolean;
  spatialPos?: SpatialPosition;
}

export interface ChatMessage {
  id: string;
  timestamp: number;
  targetmode: 1 | 2 | 3; // 1 = Private, 2 = Channel, 3 = Server
  targetClientId?: number;
  invokerid: number;
  invokername: string;
  invokeruid: string;
  msg: string;
  isSystem?: boolean;
}

export interface PermissionEntry {
  permid: number;
  permsid: string;
  permname?: string;
  permdesc?: string;
  permvalue: number;
  permnegated: boolean;
  permskip: boolean;
  grant?: number;
}

export interface ServerGroup {
  sgid: number;
  name: string;
  type: number;
  iconid: number;
  savedb: boolean;
  sortid: number;
  namemode: number;
  n_modifyp: number;
  n_member_addp: number;
  n_member_removep: number;
}

export interface ChannelGroup {
  cgid: number;
  name: string;
  type: number;
  iconid: number;
  savedb: boolean;
  sortid: number;
  namemode: number;
}

export interface PrivilegeKeyEntry {
  token: string;
  token_type: number;
  token_id1: number;
  token_id2: number;
  token_created: number;
  token_description: string;
}

export interface BanEntry {
  banid: number;
  ip: string;
  name: string;
  uid: string;
  mytsid: string;
  created: number;
  duration: number;
  invokername: string;
  invokercldbid: number;
  invokeruid: string;
  reason: string;
  enforcements: number;
}

export interface ComplaintEntry {
  tcldbid: number;
  tname: string;
  fcldbid: number;
  fname: string;
  message: string;
  timestamp: number;
}

export interface OfflineMessageEntry {
  msgid: number;
  cluid: string;
  subject: string;
  message?: string;
  timestamp: number;
  flag_read: boolean;
}

export interface FileEntry {
  name: string;
  size: number;
  datetime: number;
  type: number; // 0 = directory, 1 = file
}

export interface MusicBotQueueItem {
  song_id: number;
  title: string;
  url: string;
  loader: string;
  duration_secs: number;
  is_current: boolean;
  live_stream: boolean;
  thumbnail?: string;
}

export interface LocalMusicFileItem {
  name: string;
  url: string;
  source: string;
  size: number;
}

export interface MusicBotInfo {
  bot_id: number;
  client_id: number;
  cid: number;
  bot_name: string;
  player_state: number; // 0 = stopped, 1/2 = playing, 3 = paused, 4 = stopped
  song_title: string;
  song_url: string;
  song_thumbnail?: string;
  position_ms: number;
  duration_ms: number;
  volume: number;
  playlist_id: number;
  replay_mode?: number; // 0 = off, 1 = repeat all, 2 = repeat one
  queue: MusicBotQueueItem[];
  local_files: LocalMusicFileItem[];
}

export interface PlaylistSongEntry {
  song_id: number;
  url: string;
  title: string;
  invoker: string;
}

export interface TsdnsRecord {
  domain: string;
  target: string;
}

export interface VirtualServerSummary {
  virtualserver_id: number;
  virtualserver_port: number;
  virtualserver_status: string;
  virtualserver_clientsonline: number;
  virtualserver_maxclients: number;
  virtualserver_name: string;
  virtualserver_autostart: boolean;
}

export interface ActiveVideoStream {
  senderClientId: number;
  senderName: string;
  streamKind: 'camera' | 'screen';
  width: number;
  height: number;
  fps: number;
  codec: string;
  lastFrameAt: number;
  framesDecoded: number;
  bytesReceived: number;
}

export interface WebTransportFrame {
  type?: string;
  command: string;
  code?: string;
  data: Record<string, string | number | undefined>[];
  options?: string[];
}

export interface MediaTelemetry {
  audioCodec: string;
  videoCodec: string;
  voicePacketsSent: number;
  voicePacketsReceived: number;
  videoFramesSent: number;
  videoFramesReceived: number;
  datagramsDropped: number;
  inputLevelDb: number;
  vadActive: boolean;
  spatial3DEnabled: boolean;
  rttMs: number;
}

export interface DesktopIdentity {
  id: string;
  name: string;
  private_key: string;
  public_key: string;
  uid: string;
  default_nickname: string;
  audio_input_device?: string | null;
  audio_output_device?: string | null;
  input_amplification?: number | null;
  output_amplification?: number | null;
  voice_transmission_mode?: string | null;
  voice_activation_threshold?: number | null;
  ptt_hotkey?: string | null;
  whisper_hotkey?: string | null;
  noise_suppression?: boolean | null;
  auto_gain_control?: boolean | null;
  echo_cancellation?: boolean | null;
}

export interface DesktopFavorite {
  id: string;
  name: string;
  address: string;
  password?: string | null;
  nickname: string;
  identity_id: string;
}

export interface DesktopAppConfig {
  identities: DesktopIdentity[];
  favorites: DesktopFavorite[];
}

export interface DesktopAudioDevices {
  inputs: string[];
  outputs: string[];
}

export interface DesktopPluginSdkStatus {
  enabled: boolean;
  running: boolean;
  listen_addr: string;
  connected_plugins: number;
  last_error?: string | null;
}

export type ActiveModalType =
  | null
  | 'desktop_client'
  | 'redeem_key'
  | 'permissions'
  | 'groups_tokens'
  | 'moderation'
  | 'files'
  | 'server_admin'
  | 'channel_create'
  | 'channel_edit';

