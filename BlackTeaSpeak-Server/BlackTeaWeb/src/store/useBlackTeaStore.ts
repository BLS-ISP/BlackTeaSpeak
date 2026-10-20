import { useSyncExternalStore } from 'react';
import {
  ChannelFormPayload,
  ChatMessage,
  SpatialPosition,
} from '../types/protocol';
import { BlackTeaProtocolClient } from '../protocols/webtransport/BlackTeaProtocolClient';
import { discoverServerCertHash } from '../protocols/webtransport/WebTransportConnection';
import { OpusVoiceEngine } from '../media/webcodecs/OpusVoiceEngine';
import { VideoCodecsEngine } from '../media/webcodecs/VideoCodecsEngine';
import { soundCues } from '../media/SoundCueEngine';
import {
  BlackTeaState,
  createInitialBlackTeaState,
  StoreContext,
} from './state/blackTeaStateTypes';
import {
  ClientCapabilities,
  getClientCapabilities,
} from './capabilities/clientCapabilities';
import {
  connectToServer,
  disconnectFromServer,
  executeRawCommandLineAction,
  moveSelfToChannel,
  sendChatMessageAction,
} from './actions/connectionActions';
import {
  cycleSoundPackAction,
  handleIncomingMediaAction,
  setClientTalkingStateAction,
  setPeerVolumeAction,
  setPttPressedAction,
  setTransmissionModeAction,
  toggleCameraShareAction,
  toggleInputMuteAction,
  toggleOutputDeafenAction,
  togglePeerMuteAction,
  toggleScreenShareAction,
  toggleSpatial3DAction,
  toggleWhisperModeAction,
  updateClientSpatialPositionAction,
} from './actions/mediaAndSpatialActions';
import {
  createChannelAction,
  CreateChannelParams,
  deleteChannelAction,
  editChannelAction,
  moveChannelAction,
} from './actions/channelActions';
import {
  addBanRuleAction,
  banClientAction,
  cancelTalkPowerRequestAction,
  clearAllBansAction,
  deleteAllComplaintsAction,
  deleteBanRuleAction,
  deleteComplaintAction,
  deleteOfflineMessageAction,
  editClientDescriptionAction,
  kickClientAction,
  moveClientToChannelAction,
  pokeClientAction,
  readOfflineMessageAction,
  refreshModerationListsAction,
  requestTalkPowerAction,
  sendOfflineMessageAction,
  setClientIsTalkerAction,
  submitComplaintAction,
  toggleAwayAction,
  toggleChannelCommanderAction,
} from './actions/clientModerationActions';
import {
  copyServerGroupAction,
  createChannelGroupAction,
  createServerGroupAction,
  createTokenAction,
  deleteChannelGroupAction,
  deleteServerGroupAction,
  deleteTokenAction,
  loadPermissionsDataAction,
  PermissionTier,
  refreshTokensAction,
  removePermissionValueAction,
  renameChannelGroupAction,
  renameServerGroupAction,
  setClientChannelGroupAction,
  setPermissionValueAction,
  toggleClientServerGroupAction,
  useTokenAction,
} from './actions/permissionAndGroupActions';
import {
  createChannelDirectoryAction,
  createVirtualServerAction,
  deleteChannelFileAction,
  deleteTsdnsAction,
  deleteVirtualServerAction,
  downloadChannelFileAction,
  fetchChannelFilesAction,
  refreshServerAdminDataAction,
  renameChannelFileAction,
  toggleVirtualServerStateAction,
  updateVirtualServerAction,
  UpdateVirtualServerParams,
  uploadChannelFileAction,
  upsertTsdnsAction,
} from './actions/fileAndServerAdminActions';
import {
  controlMusicBotAction,
  createMusicBotAction,
  deleteMusicBotAction,
  loadMusicBotSongAction,
  refreshMusicBotsAction,
  renameMusicBotAction,
  uploadLocalMusicFileAction,
} from './actions/musicBotActions';
import { handleServerFrameDispatch } from './notifications/adminAndFeatureFrameHandlers';

export type { BlackTeaState, ClientCapabilities };
export { getClientCapabilities };

class BlackTeaStoreManager implements StoreContext {
  public readonly protocol = new BlackTeaProtocolClient();
  public readonly voiceEngine = new OpusVoiceEngine();
  public readonly videoEngine = new VideoCodecsEngine();
  public readonly peerTalkTimers = new Map<number, number>();
  public readonly botVolumeTimers = new Map<number, number>();

  private listeners = new Set<() => void>();
  private state: BlackTeaState = createInitialBlackTeaState();

  constructor() {
    this.protocol.setHandlers({
      onNotification: (frame) => handleServerFrameDispatch(this, frame),
      onMedia: (packetType, senderClientId, payload) =>
        handleIncomingMediaAction(this, packetType, senderClientId, payload),
      onDisconnect: (reason) => {
        for (const [, timer] of this.peerTalkTimers) window.clearTimeout(timer);
        this.peerTalkTimers.clear();
        for (const [, timer] of this.botVolumeTimers) window.clearTimeout(timer);
        this.botVolumeTimers.clear();
        this.voiceEngine.dispose();
        this.videoEngine.dispose();
        soundCues.play('disconnected');
        this.patchState({
          connectionState: 'disconnected',
          connectionError: reason,
          cameraSharing: false,
          screenSharing: false,
        });
        this.appendSystemChat(`Verbindung getrennt: ${reason}`);
      },
    });

    this.voiceEngine.configure({
      sendDatagram: (pktType, payload) => this.protocol.sendMediaDatagram(pktType, payload),
      onTelemetry: (stats) => {
        const vidStats = this.videoEngine.getStats();
        const selfClid = this.state.selfClientId;
        const selfClient = selfClid
          ? this.state.clients.find((c) => c.clid === selfClid)
          : undefined;
        const wantWhisper = stats.vadActive && this.state.whisperMode;
        const needClientUpdate =
          selfClient !== undefined &&
          (selfClient.isTalking !== stats.vadActive ||
            selfClient.isWhispering !== wantWhisper);

        this.patchState({
          ...(needClientUpdate
            ? {
                clients: this.state.clients.map((c) =>
                  c.clid === selfClid
                    ? { ...c, isTalking: stats.vadActive, isWhispering: wantWhisper }
                    : c
                ),
              }
            : {}),
          telemetry: {
            ...this.state.telemetry,
            inputLevelDb: stats.inputLevelDb,
            vadActive: stats.vadActive,
            voicePacketsSent: stats.voicePacketsSent,
            voicePacketsReceived: stats.voicePacketsReceived,
            videoFramesSent: vidStats.videoFramesSent,
            videoFramesReceived: vidStats.videoFramesReceived,
          },
        });
      },
    });

    this.videoEngine.configure({
      sendDatagram: (pktType, payload) => this.protocol.sendMediaDatagram(pktType, payload),
      sendUnidirectional: (pktType, payload) =>
        this.protocol.sendMediaUnidirectional(pktType, payload),
      onStreamUpdate: (streamKey, info) => {
        const peer = this.state.clients.find((c) => c.clid === info.senderClientId);
        const senderName = peer?.client_nickname ?? `Client #${info.senderClientId}`;
        this.patchState({
          activeVideoStreams: {
            ...this.state.activeVideoStreams,
            [streamKey]: { ...info, senderName },
          },
        });
      },
    });

    discoverServerCertHash(this.state.host).then((res) => {
      if (res.sha256 || res.webtransportPort) {
        this.patchState({
          certHash: res.sha256 ?? this.state.certHash,
          port: res.webtransportPort ?? this.state.port,
        });
      }
    });
  }

  public getState = (): BlackTeaState => this.state;

  public subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  };

  public patchState(partial: Partial<BlackTeaState>): void {
    this.state = { ...this.state, ...partial };
    for (const listener of this.listeners) {
      listener();
    }
  }

  public showToast(msg: string): void {
    this.patchState({ statusToast: msg });
    window.setTimeout(() => {
      if (this.state.statusToast === msg) {
        this.patchState({ statusToast: null });
      }
    }, 3500);
  }

  public appendSystemChat(msg: string): void {
    const sysMsg: ChatMessage = {
      id: `sys-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`,
      timestamp: Date.now(),
      targetmode: 3,
      invokerid: 0,
      invokername: 'Server',
      invokeruid: 'system',
      msg,
      isSystem: true,
    };
    this.patchState({
      chatMessages: [...this.state.chatMessages.slice(-299), sysMsg],
    });
  }

  // Connection & Chat
  public connect = (): Promise<void> => connectToServer(this);
  public disconnect = (): void => disconnectFromServer(this);
  public moveToChannel = (cid: number, password = ''): Promise<void> =>
    moveSelfToChannel(this, cid, password);
  public sendChatMessage = (text: string): Promise<void> => sendChatMessageAction(this, text);
  public executeRawCommandLine = (rawLine: string): Promise<void> =>
    executeRawCommandLineAction(this, rawLine);

  // Media & Spatial Audio
  public toggleInputMute = (): void => toggleInputMuteAction(this);
  public toggleOutputDeafen = (): void => toggleOutputDeafenAction(this);
  public cycleSoundPack = (): void => cycleSoundPackAction(this);
  public setTransmissionMode = (mode: 'vad' | 'ptt', thresholdDb?: number): void =>
    setTransmissionModeAction(this, mode, thresholdDb);
  public setPttPressed = (pressed: boolean): void => setPttPressedAction(this, pressed);
  public toggleWhisperMode = (): void => toggleWhisperModeAction(this);
  public toggleSpatial3D = (): void => toggleSpatial3DAction(this);
  public updateClientSpatialPosition = (clientId: number, pos: SpatialPosition): void =>
    updateClientSpatialPositionAction(this, clientId, pos);
  public toggleCameraShare = (): Promise<void> => toggleCameraShareAction(this);
  public toggleScreenShare = (): Promise<void> => toggleScreenShareAction(this);
  public setPeerVolume = (clientId: number, gain: number): void =>
    setPeerVolumeAction(this, clientId, gain);
  public togglePeerMute = (clientId: number): void => togglePeerMuteAction(this, clientId);
  public setClientTalkingState = (
    clid: number,
    isTalking: boolean,
    isWhispering = false
  ): void => setClientTalkingStateAction(this, clid, isTalking, isWhispering);

  // Channels
  public createChannel = (params: CreateChannelParams): Promise<void> =>
    createChannelAction(this, params);
  public editChannel = (
    cid: number,
    params: Partial<ChannelFormPayload> & { maxclients?: number }
  ): Promise<void> => editChannelAction(this, cid, params);
  public moveChannel = (cid: number, cpid: number, order = 0): Promise<void> =>
    moveChannelAction(this, cid, cpid, order);
  public deleteChannel = (cid: number): Promise<void> => deleteChannelAction(this, cid);

  // Self & Peer Client Moderation
  public toggleAway = (away?: boolean, message = ''): Promise<void> =>
    toggleAwayAction(this, away, message);
  public toggleChannelCommander = (): Promise<void> => toggleChannelCommanderAction(this);
  public requestTalkPower = (message: string): Promise<void> =>
    requestTalkPowerAction(this, message);
  public cancelTalkPowerRequest = (): Promise<void> => cancelTalkPowerRequestAction(this);
  public setClientIsTalker = (clid: number, isTalker: boolean): Promise<void> =>
    setClientIsTalkerAction(this, clid, isTalker);
  public editClientDescription = (clid: number, description: string): Promise<void> =>
    editClientDescriptionAction(this, clid, description);
  public moveClientToChannel = (clid: number, cid: number, password = ''): Promise<void> =>
    moveClientToChannelAction(this, clid, cid, password);
  public pokeClient = (clid: number, msg: string): Promise<void> =>
    pokeClientAction(this, clid, msg);
  public kickClient = (clid: number, fromServer: boolean, reason: string): Promise<void> =>
    kickClientAction(this, clid, fromServer, reason);
  public banClient = (clid: number, timeSeconds: number, reason: string): Promise<void> =>
    banClientAction(this, clid, timeSeconds, reason);
  public refreshModerationLists = (): Promise<void> => refreshModerationListsAction(this);
  public addBanRule = (params: {
    ip?: string;
    name?: string;
    uid?: string;
    timeSeconds: number;
    reason: string;
  }): Promise<void> => addBanRuleAction(this, params);
  public deleteBanRule = (banid: number): Promise<void> => deleteBanRuleAction(this, banid);
  public clearAllBans = (): Promise<void> => clearAllBansAction(this);
  public submitComplaint = (tcldbid: number, message: string): Promise<void> =>
    submitComplaintAction(this, tcldbid, message);
  public deleteComplaint = (tcldbid: number, fcldbid: number): Promise<void> =>
    deleteComplaintAction(this, tcldbid, fcldbid);
  public deleteAllComplaints = (tcldbid: number): Promise<void> =>
    deleteAllComplaintsAction(this, tcldbid);
  public sendOfflineMessage = (cluid: string, subject: string, message: string): Promise<void> =>
    sendOfflineMessageAction(this, cluid, subject, message);
  public readOfflineMessage = (msgid: number): Promise<string> =>
    readOfflineMessageAction(this, msgid);
  public deleteOfflineMessage = (msgid: number): Promise<void> =>
    deleteOfflineMessageAction(this, msgid);

  // Permissions, Groups & Privilege Keys
  public loadPermissionsData = (
    tier: PermissionTier,
    id1: number,
    id2 = 0
  ): Promise<void> => loadPermissionsDataAction(this, tier, id1, id2);
  public setPermissionValue = (
    tier: PermissionTier,
    id1: number,
    id2: number,
    permsid: string,
    permvalue: number,
    permskip = false,
    permnegated = false
  ): Promise<void> =>
    setPermissionValueAction(this, tier, id1, id2, permsid, permvalue, permskip, permnegated);
  public removePermissionValue = (
    tier: PermissionTier,
    id1: number,
    id2: number,
    permsid: string
  ): Promise<void> => removePermissionValueAction(this, tier, id1, id2, permsid);
  public createServerGroup = (name: string): Promise<void> =>
    createServerGroupAction(this, name);
  public renameServerGroup = (sgid: number, name: string): Promise<void> =>
    renameServerGroupAction(this, sgid, name);
  public copyServerGroup = (ssgid: number, name: string): Promise<void> =>
    copyServerGroupAction(this, ssgid, name);
  public deleteServerGroup = (sgid: number): Promise<void> =>
    deleteServerGroupAction(this, sgid);
  public createChannelGroup = (name: string): Promise<void> =>
    createChannelGroupAction(this, name);
  public renameChannelGroup = (cgid: number, name: string): Promise<void> =>
    renameChannelGroupAction(this, cgid, name);
  public deleteChannelGroup = (cgid: number): Promise<void> =>
    deleteChannelGroupAction(this, cgid);
  public toggleClientServerGroup = (
    cldbid: number,
    sgid: number,
    hasGroup: boolean
  ): Promise<void> => toggleClientServerGroupAction(this, cldbid, sgid, hasGroup);
  public setClientChannelGroup = (cgid: number, cid: number, cldbid: number): Promise<void> =>
    setClientChannelGroupAction(this, cgid, cid, cldbid);
  public refreshTokens = (): Promise<void> => refreshTokensAction(this);
  public createToken = (
    tokenType: number,
    id1: number,
    id2: number,
    description: string
  ): Promise<void> => createTokenAction(this, tokenType, id1, id2, description);
  public useToken = (token: string): Promise<void> => useTokenAction(this, token);
  public deleteToken = (token: string): Promise<void> => deleteTokenAction(this, token);

  // File Browser & Virtual Server Admin / TSDNS
  public fetchChannelFiles = (cid: number, path = '/'): Promise<void> =>
    fetchChannelFilesAction(this, cid, path);
  public createChannelDirectory = (cid: number, dirname: string): Promise<void> =>
    createChannelDirectoryAction(this, cid, dirname);
  public renameChannelFile = (cid: number, oldName: string, newName: string): Promise<void> =>
    renameChannelFileAction(this, cid, oldName, newName);
  public deleteChannelFile = (cid: number, name: string): Promise<void> =>
    deleteChannelFileAction(this, cid, name);
  public uploadChannelFile = (cid: number, file: File): Promise<void> =>
    uploadChannelFileAction(this, cid, file);
  public downloadChannelFile = (cid: number, name: string): Promise<void> =>
    downloadChannelFileAction(this, cid, name);
  public refreshServerAdminData = (): Promise<void> => refreshServerAdminDataAction(this);
  public updateVirtualServer = (params: UpdateVirtualServerParams): Promise<void> =>
    updateVirtualServerAction(this, params);
  public createVirtualServer = (name: string, port: number, maxClients = 32): Promise<void> =>
    createVirtualServerAction(this, name, port, maxClients);
  public toggleVirtualServerState = (sid: number, start: boolean): Promise<void> =>
    toggleVirtualServerStateAction(this, sid, start);
  public deleteVirtualServer = (sid: number): Promise<void> =>
    deleteVirtualServerAction(this, sid);
  public upsertTsdns = (domain: string, target: string): Promise<void> =>
    upsertTsdnsAction(this, domain, target);
  public deleteTsdns = (domain: string): Promise<void> => deleteTsdnsAction(this, domain);

  // MusicBots
  public refreshMusicBots = (): Promise<void> => refreshMusicBotsAction(this);
  public createMusicBot = (cid: number, name: string): Promise<void> =>
    createMusicBotAction(this, cid, name);
  public deleteMusicBot = (botId: number): Promise<void> => deleteMusicBotAction(this, botId);
  public controlMusicBot = (botId: number, action: string, value?: number): Promise<void> =>
    controlMusicBotAction(this, botId, action, value);
  public renameMusicBot = (botId: number, newName: string): Promise<void> =>
    renameMusicBotAction(this, botId, newName);
  public uploadLocalMusicFile = (
    file: File,
    botId?: number,
    playNow = false
  ): Promise<void> => uploadLocalMusicFileAction(this, file, botId, playNow);
  public loadMusicBotSong = (botId: number, url: string, playNow = true): Promise<void> =>
    loadMusicBotSongAction(this, botId, url, playNow);
}

export const blackTeaStore = new BlackTeaStoreManager();

export function useBlackTeaStore(): BlackTeaState {
  return useSyncExternalStore(blackTeaStore.subscribe, blackTeaStore.getState);
}
