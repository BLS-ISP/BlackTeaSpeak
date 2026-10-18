import {
  BanEntry,
  ChannelGroup,
  ComplaintEntry,
  FileEntry,
  LocalMusicFileItem,
  MusicBotInfo,
  MusicBotQueueItem,
  OfflineMessageEntry,
  PermissionEntry,
  PrivilegeKeyEntry,
  ServerGroup,
  TsdnsRecord,
  VirtualServerSummary,
  WebTransportFrame,
} from '../../types/protocol';
import { StoreContext } from '../state/blackTeaStateTypes';
import { handleChannelAndClientFrame } from './channelAndClientFrameHandlers';

export function handleServerFrameDispatch(ctx: StoreContext, frame: WebTransportFrame): void {
  const cmd = frame.command;
  const rows = frame.data ?? [];
  if (rows.length === 0) {
    if (cmd === 'notifymusicbotlist' || cmd === 'musicbotlist' || cmd === 'mbotlist') {
      ctx.patchState({ musicBots: [] });
    } else if (
      cmd === 'notifyservergrouppermlist' ||
      cmd === 'notifychannelgrouppermlist' ||
      cmd === 'notifychannelpermlist' ||
      cmd === 'notifyclientpermlist' ||
      cmd === 'notifychannelclientpermlist'
    ) {
      ctx.patchState({ inspectedPermissions: [] });
    }
    return;
  }

  if (handleChannelAndClientFrame(ctx, cmd, rows)) {
    return;
  }

  const state = ctx.getState();

  switch (cmd) {
    case 'notifyservergrouplist':
    case 'servergrouplist': {
      const groups: ServerGroup[] = rows.map((r) => ({
        sgid: Number(r.sgid ?? 0),
        name: String(r.name ?? ''),
        type: Number(r.type ?? 1),
        iconid: Number(r.iconid ?? 0),
        savedb: Number(r.savedb ?? 1) === 1,
        sortid: Number(r.sortid ?? 0),
        namemode: Number(r.namemode ?? 0),
        n_modifyp: Number(r.n_modifyp ?? 75),
        n_member_addp: Number(r.n_member_addp ?? 75),
        n_member_removep: Number(r.n_member_removep ?? 75),
      }));
      ctx.patchState({ serverGroups: groups });
      break;
    }

    case 'notifychannelgrouplist':
    case 'channelgrouplist': {
      const groups: ChannelGroup[] = rows.map((r) => ({
        cgid: Number(r.cgid ?? 0),
        name: String(r.name ?? ''),
        type: Number(r.type ?? 1),
        iconid: Number(r.iconid ?? 0),
        savedb: Number(r.savedb ?? 1) === 1,
        sortid: Number(r.sortid ?? 0),
        namemode: Number(r.namemode ?? 0),
      }));
      ctx.patchState({ channelGroups: groups });
      break;
    }

    case 'notifypermissionlist':
    case 'permissionlist': {
      const catalog: PermissionEntry[] = rows
        .filter((r) => r.permname || r.permsid)
        .map((r) => ({
          permid: Number(r.permid ?? 0),
          permsid: String(r.permname ?? r.permsid ?? ''),
          permname: String(r.permname ?? r.permsid ?? ''),
          permdesc: String(r.permdesc ?? ''),
          permvalue: 0,
          permnegated: false,
          permskip: false,
        }));
      ctx.patchState({ permissionCatalog: catalog });
      break;
    }

    case 'notifyservergrouppermlist':
    case 'notifychannelgrouppermlist':
    case 'notifychannelpermlist':
    case 'notifyclientpermlist':
    case 'notifychannelclientpermlist': {
      const perms: PermissionEntry[] = rows.map((r) => ({
        permid: Number(r.permid ?? 0),
        permsid: String(r.permsid ?? `permid_${r.permid}`),
        permvalue: Number(r.permvalue ?? 0),
        permnegated: Number(r.permnegated ?? 0) === 1,
        permskip: Number(r.permskip ?? 0) === 1,
      }));
      ctx.patchState({ inspectedPermissions: perms });
      break;
    }

    case 'notifytokenlist':
    case 'notifyprivilegekeylist':
    case 'privilegekeylist': {
      const tokens: PrivilegeKeyEntry[] = rows.map((r) => ({
        token: String(r.token ?? ''),
        token_type: Number(r.token_type ?? 0),
        token_id1: Number(r.token_id1 ?? 0),
        token_id2: Number(r.token_id2 ?? 0),
        token_created: Number(r.token_created ?? 0),
        token_description: String(r.token_description ?? ''),
      }));
      ctx.patchState({ tokens });
      break;
    }

    case 'notifybanlist':
    case 'banlist': {
      const bans: BanEntry[] = rows.map((r) => ({
        banid: Number(r.banid ?? 0),
        ip: String(r.ip ?? ''),
        name: String(r.name ?? ''),
        uid: String(r.uid ?? ''),
        mytsid: String(r.mytsid ?? ''),
        created: Number(r.created ?? 0),
        duration: Number(r.duration ?? 0),
        invokername: String(r.invokername ?? ''),
        invokercldbid: Number(r.invokercldbid ?? 0),
        invokeruid: String(r.invokeruid ?? ''),
        reason: String(r.reason ?? ''),
        enforcements: Number(r.enforcements ?? 0),
      }));
      ctx.patchState({ bans });
      break;
    }

    case 'notifycomplainlist':
    case 'complainlist': {
      const complaints: ComplaintEntry[] = rows.map((r) => ({
        tcldbid: Number(r.tcldbid ?? 0),
        tname: String(r.tname ?? ''),
        fcldbid: Number(r.fcldbid ?? 0),
        fname: String(r.fname ?? ''),
        message: String(r.message ?? ''),
        timestamp: Number(r.timestamp ?? 0),
      }));
      ctx.patchState({ complaints });
      break;
    }

    case 'notifymessagelist':
    case 'messagelist': {
      const msgs: OfflineMessageEntry[] = rows.map((r) => ({
        msgid: Number(r.msgid ?? 0),
        cluid: String(r.cluid ?? ''),
        subject: String(r.subject ?? ''),
        timestamp: Number(r.timestamp ?? 0),
        flag_read: Number(r.flag_read ?? 0) === 1,
      }));
      ctx.patchState({ offlineMessages: msgs });
      break;
    }

    case 'notifyfilelist':
    case 'ftgetfilelist': {
      const files: FileEntry[] = rows
        .filter((r) => r.name)
        .map((r) => ({
          name: String(r.name ?? ''),
          size: Number(r.size ?? 0),
          datetime: Number(r.datetime ?? 0),
          type: Number(r.type ?? 1),
        }));
      ctx.patchState({ files });
      break;
    }

    case 'notifymusicbotlist':
    case 'musicbotlist':
    case 'mbotlist':
    case 'notifymusicplayerinfo': {
      const prevMap = new Map(state.musicBots.map((b) => [b.bot_id, b]));
      const map =
        cmd === 'notifymusicplayerinfo'
          ? new Map(prevMap)
          : new Map<number, MusicBotInfo>();
      for (const r of rows) {
        const botId = Number(r.botid ?? r.bot_id ?? r.id ?? 0);
        if (!botId) continue;
        const prev = prevMap.get(botId);
        const clientId = Number(r.clid ?? r.client_id ?? prev?.client_id ?? 0);
        const rawVolStr = String(r.bot_volume ?? r.volume ?? r.player_volume ?? '');
        let volume = prev?.volume ?? 100;
        if (!ctx.botVolumeTimers.has(botId) && rawVolStr !== '') {
          const parsedVol = Number(rawVolStr);
          if (!Number.isNaN(parsedVol)) {
            volume = parsedVol === 1 && prev?.volume === undefined ? 100 : parsedVol;
          }
        }
        if (clientId > 0 && prev?.volume !== volume && !ctx.botVolumeTimers.has(botId)) {
          ctx.voiceEngine.setPeerVolume(clientId, Math.max(0, Math.min(2, volume / 100)));
        }
        const rawTitle = String(
          r.song_title ?? r.player_title ?? r.title ?? prev?.song_title ?? ''
        ).trim();
        let queue: MusicBotQueueItem[] = prev?.queue ?? [];
        if (typeof r.queue_json === 'string' && r.queue_json.startsWith('[')) {
          try {
            queue = JSON.parse(r.queue_json) as MusicBotQueueItem[];
          } catch {
            // keep previous queue
          }
        }
        let local_files: LocalMusicFileItem[] = prev?.local_files ?? [];
        if (typeof r.local_files_json === 'string' && r.local_files_json.startsWith('[')) {
          try {
            local_files = JSON.parse(r.local_files_json) as LocalMusicFileItem[];
          } catch {
            // keep previous local_files
          }
        }
        map.set(botId, {
          bot_id: botId,
          client_id: clientId,
          cid: Number(r.cid ?? prev?.cid ?? 1),
          bot_name: String(
            r.bot_name ?? r.name ?? r.client_nickname ?? prev?.bot_name ?? `MusicBot #${botId}`
          ),
          player_state: Number(r.player_state ?? prev?.player_state ?? 4),
          song_title: rawTitle || 'Kein Titel geladen',
          song_url: String(r.song_url ?? r.url ?? prev?.song_url ?? ''),
          song_thumbnail: String(r.song_thumbnail ?? prev?.song_thumbnail ?? ''),
          position_ms: Number(
            r.player_replay_index ?? r.player_position ?? r.position_ms ?? prev?.position_ms ?? 0
          ),
          duration_ms: Number(r.player_max_index ?? r.duration_ms ?? prev?.duration_ms ?? 0),
          volume,
          playlist_id: Number(r.playlist_id ?? prev?.playlist_id ?? botId),
          replay_mode: Number(r.replay_mode ?? prev?.replay_mode ?? 0),
          queue,
          local_files,
        });
      }
      ctx.patchState({ musicBots: Array.from(map.values()) });
      break;
    }

    case 'notifytsdnslist':
    case 'tsdnslist': {
      const records: TsdnsRecord[] = rows
        .filter((r) => r.domain)
        .map((r) => ({
          domain: String(r.domain ?? ''),
          target: String(r.target ?? ''),
        }));
      ctx.patchState({ tsdnsRecords: records });
      break;
    }

    case 'notifyserverlist':
    case 'serverlist': {
      const servers: VirtualServerSummary[] = rows.map((r) => ({
        virtualserver_id: Number(r.virtualserver_id ?? 1),
        virtualserver_port: Number(r.virtualserver_port ?? 9987),
        virtualserver_status: String(r.virtualserver_status ?? 'online'),
        virtualserver_clientsonline: Number(r.virtualserver_clientsonline ?? 0),
        virtualserver_maxclients: Number(r.virtualserver_maxclients ?? 32),
        virtualserver_name: String(r.virtualserver_name ?? 'BlackTeaSpeak'),
        virtualserver_autostart: Number(r.virtualserver_autostart ?? 1) === 1,
      }));
      ctx.patchState({ virtualServers: servers });
      break;
    }

    case 'notifyclientneededpermissions': {
      const perms: Record<number, number> = {};
      for (const r of rows) {
        const permid = Number(r.permid ?? 0);
        const permvalue = Number(r.permvalue ?? 0);
        if (permid > 0) {
          perms[permid] = permvalue;
        }
      }
      ctx.patchState({ selfNeededPermissions: perms });
      break;
    }
  }
}
