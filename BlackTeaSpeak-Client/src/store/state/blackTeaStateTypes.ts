import {
  ActiveModalType,
  ActiveVideoStream,
  BanEntry,
  ChannelGroup,
  ChannelInfo,
  ChatMessage,
  ClientInfo,
  ComplaintEntry,
  ConnectionState,
  DesktopAudioDevices,
  DesktopFavorite,
  DesktopIdentity,
  DesktopPluginSdkStatus,
  FileEntry,
  MediaTelemetry,
  MusicBotInfo,
  OfflineMessageEntry,
  PermissionEntry,
  PlaylistSongEntry,
  PrivilegeKeyEntry,
  ServerGroup,
  ServerInfo,
  SpatialPosition,
  TsdnsRecord,
  VirtualServerSummary,
} from '../../types/protocol';
import { BlackTeaProtocolClient } from '../../protocols/webtransport/BlackTeaProtocolClient';
import { OpusVoiceEngine } from '../../media/webcodecs/OpusVoiceEngine';
import { VideoCodecsEngine } from '../../media/webcodecs/VideoCodecsEngine';
import { soundCues, SoundPackId } from '../../media/SoundCueEngine';

export interface BlackTeaState {
  connectionState: ConnectionState;
  connectionError: string | null;
  host: string;
  port: number;
  certHash: string;
  nickname: string;
  serverPassword: string;
  identityPublicKey: string;

  desktopIdentities: DesktopIdentity[];
  activeIdentityId: string;
  desktopFavorites: DesktopFavorite[];
  desktopAudioDevices: DesktopAudioDevices;
  pluginSdkStatus: DesktopPluginSdkStatus;

  selfClientId: number;
  selfDatabaseId: number;
  selfUniqueId: string;
  currentChannelId: number;

  serverInfo: ServerInfo;
  channels: ChannelInfo[];
  clients: ClientInfo[];
  selectedChannelId: number | null;
  selectedClientId: number | null;

  chatMessages: ChatMessage[];
  activeChatTab: 'server' | 'channel' | 'private';
  privateChatTargetClientId: number | null;
  pokeAlert: { invokerName: string; message: string; at: number } | null;

  inputMuted: boolean;
  outputDeafened: boolean;
  soundPack: SoundPackId;
  transmissionMode: 'vad' | 'ptt';
  vadThresholdDb: number;
  pttKey: string;
  pttPressed: boolean;
  whisperMode: boolean;
  cameraSharing: boolean;
  screenSharing: boolean;
  showSpatialRadar: boolean;
  showMusicBotDeck: boolean;
  selfSpatialPos: SpatialPosition;

  activeVideoStreams: Record<string, ActiveVideoStream>;
  telemetry: MediaTelemetry;

  serverGroups: ServerGroup[];
  channelGroups: ChannelGroup[];
  permissionCatalog: PermissionEntry[];
  inspectedPermissions: PermissionEntry[];
  selfNeededPermissions: Record<number, number>;
  tokens: PrivilegeKeyEntry[];
  bans: BanEntry[];
  complaints: ComplaintEntry[];
  offlineMessages: OfflineMessageEntry[];
  files: FileEntry[];
  currentFilePath: string;
  musicBots: MusicBotInfo[];
  playlistSongs: PlaylistSongEntry[];
  tsdnsRecords: TsdnsRecord[];
  virtualServers: VirtualServerSummary[];

  activeModal: ActiveModalType;
  statusToast: string | null;
}

export interface StoreContext {
  readonly protocol: BlackTeaProtocolClient;
  readonly voiceEngine: OpusVoiceEngine;
  readonly videoEngine: VideoCodecsEngine;
  readonly peerTalkTimers: Map<number, number>;
  readonly botVolumeTimers: Map<number, number>;
  getState(): BlackTeaState;
  patchState(partial: Partial<BlackTeaState>): void;
  showToast(msg: string): void;
  appendSystemChat(msg: string): void;
  setClientTalkingState(clid: number, isTalking: boolean, isWhispering?: boolean): void;
}

export function getOrCreateIdentityKey(): string {
  const key = 'blackteaweb_ed25519_pub';
  const stored = localStorage.getItem(key);
  if (stored && stored.length > 8) return stored;
  const randomBytes = new Uint8Array(24);
  crypto.getRandomValues(randomBytes);
  const generated = btoa(String.fromCharCode(...randomBytes));
  localStorage.setItem(key, generated);
  return generated;
}

export const DEFAULT_SERVER_INFO: ServerInfo = {
  virtualserver_id: 1,
  virtualserver_name: 'BlackTeaSpeak Server',
  virtualserver_welcomemessage: 'Willkommen im BlackTeaSpeak Desktop Client v2.0!',
  virtualserver_maxclients: 64,
  virtualserver_clientsonline: 0,
  virtualserver_channelsonline: 0,
  virtualserver_uptime: 0,
  virtualserver_platform: 'Rust / Tokio / HTTP3-QUIC',
  virtualserver_version: 'BlackTeaSpeak 2.0-Desktop',
  virtualserver_hostbanner_url: '',
  virtualserver_hostbanner_gfx_url: '',
  virtualserver_hostbutton_tooltip: 'BlackTeaSpeak',
  virtualserver_hostbutton_url: '',
  virtualserver_codec_encryption_mode: 2,
  virtualserver_default_server_group: 8,
  virtualserver_default_channel_group: 8,
};

export function createInitialBlackTeaState(): BlackTeaState {
  const defaultPubKey = getOrCreateIdentityKey();
  const initialIdentity: DesktopIdentity = {
    id: 'id_default',
    name: 'Standard-Identität',
    private_key: defaultPubKey,
    public_key: defaultPubKey,
    uid: defaultPubKey.slice(0, 28),
    default_nickname: localStorage.getItem('blackteaweb_nick') || 'BlackTeaUser',
    audio_input_device: null,
    audio_output_device: null,
    input_amplification: 1.0,
    output_amplification: 1.0,
    voice_transmission_mode: 'voice_activation',
    voice_activation_threshold: 0.05,
    ptt_hotkey: 'Space',
    whisper_hotkey: null,
    noise_suppression: true,
    auto_gain_control: true,
    echo_cancellation: true,
  };

  const rawHostname =
    typeof window !== 'undefined' &&
    window.location.hostname &&
    window.location.hostname !== 'tauri.localhost'
      ? window.location.hostname
      : '127.0.0.1';

  return {
    connectionState: 'disconnected',
    connectionError: null,
    host: rawHostname,
    port: 9989,
    certHash: '',
    nickname: localStorage.getItem('blackteaweb_nick') || 'BlackTeaUser',
    serverPassword: '',
    identityPublicKey: defaultPubKey,

    desktopIdentities: [initialIdentity],
    activeIdentityId: initialIdentity.id,
    desktopFavorites: [
      {
        id: 'fav_local_default',
        name: 'Lokaler BlackTeaSpeak Server',
        address: '127.0.0.1:9989',
        password: '',
        nickname: localStorage.getItem('blackteaweb_nick') || 'BlackTeaUser',
        identity_id: initialIdentity.id,
      },
    ],
    desktopAudioDevices: {
      inputs: ['System-Standard Mikrofon'],
      outputs: ['System-Standard Lautsprecher'],
    },
    pluginSdkStatus: {
      enabled: true,
      running: true,
      listen_addr: '127.0.0.1:28910',
      connected_plugins: 0,
      last_error: null,
    },

    selfClientId: 0,
    selfDatabaseId: 0,
    selfUniqueId: '',
    currentChannelId: 1,

    serverInfo: DEFAULT_SERVER_INFO,
    channels: [],
    clients: [],
    selectedChannelId: 1,
    selectedClientId: null,

    chatMessages: [
      {
        id: 'welcome-sys',
        timestamp: Date.now(),
        targetmode: 3,
        invokerid: 0,
        invokername: 'System',
        invokeruid: 'server',
        msg: 'BlackTeaSpeak Desktop 2.0 bereit — Tauri v2 + HTTP/3 WebTransport + WebCodecs (Opus & VP9/VP8) + 3D HRTF Spatial Audio + Plugin SDK.',
        isSystem: true,
      },
    ],
    activeChatTab: 'channel',
    privateChatTargetClientId: null,
    pokeAlert: null,

    inputMuted: false,
    outputDeafened: false,
    soundPack: soundCues.getSoundPack(),
    transmissionMode: 'vad',
    vadThresholdDb: -42,
    pttKey: 'Space',
    pttPressed: false,
    whisperMode: false,
    cameraSharing: false,
    screenSharing: false,
    showSpatialRadar: false,
    showMusicBotDeck: false,
    selfSpatialPos: { x: 0, y: 0, z: 0 },

    activeVideoStreams: {},
    telemetry: {
      audioCodec: 'Opus 48kHz Stereo (WebCodecs)',
      videoCodec: 'VP9 / VP8 (WebCodecs)',
      voicePacketsSent: 0,
      voicePacketsReceived: 0,
      videoFramesSent: 0,
      videoFramesReceived: 0,
      datagramsDropped: 0,
      inputLevelDb: -90,
      vadActive: false,
      spatial3DEnabled: true,
      rttMs: 4,
    },

    serverGroups: [],
    channelGroups: [],
    permissionCatalog: [],
    inspectedPermissions: [],
    selfNeededPermissions: {},
    tokens: [],
    bans: [],
    complaints: [],
    offlineMessages: [],
    files: [],
    currentFilePath: '/',
    musicBots: [],
    playlistSongs: [],
    tsdnsRecords: [],
    virtualServers: [],

    activeModal: null,
    statusToast: null,
  };
}

