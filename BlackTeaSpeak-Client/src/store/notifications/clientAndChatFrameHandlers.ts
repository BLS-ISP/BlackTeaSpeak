import { soundCues } from '../../media/SoundCueEngine';
import { ChatMessage } from '../../types/protocol';
import { parseClientRow } from '../parsers/protocolRowParsers';
import { StoreContext } from '../state/blackTeaStateTypes';

export function handleClientAndChatFrame(
  ctx: StoreContext,
  cmd: string,
  rows: Record<string, string | number | undefined>[]
): boolean {
  const state = ctx.getState();

  switch (cmd) {
    case 'clientlist':
    case 'notifycliententerview': {
      const map = new Map(state.clients.map((c) => [c.clid, c]));
      let selfClid = state.selfClientId;
      let selfDbid = state.selfDatabaseId;
      let currentCid = state.currentChannelId;
      let peerConnectedCurrentChannel = false;
      for (const r of rows) {
        const rawClid = Number(r.clid ?? 0);
        const existed = map.has(rawClid);
        const parsed = parseClientRow(r, map.get(rawClid));
        if (parsed.clid > 0) {
          map.set(parsed.clid, parsed);
          if (
            parsed.client_nickname === state.nickname &&
            (!selfClid || parsed.clid === selfClid)
          ) {
            selfClid = parsed.clid;
            currentCid = parsed.cid;
            if (parsed.client_database_id > 0) {
              selfDbid = parsed.client_database_id;
            }
          } else if (parsed.clid === selfClid && parsed.client_database_id > 0) {
            selfDbid = parsed.client_database_id;
          } else if (
            cmd === 'notifycliententerview' &&
            !existed &&
            parsed.clid !== selfClid &&
            parsed.cid === currentCid
          ) {
            peerConnectedCurrentChannel = true;
          }
        }
      }
      ctx.patchState({
        clients: Array.from(map.values()),
        selfClientId: selfClid,
        selfDatabaseId: selfDbid,
        currentChannelId: currentCid,
      });
      if (peerConnectedCurrentChannel && !state.outputDeafened) {
        soundCues.play('user_connected_current_channel');
      }
      return true;
    }

    case 'notifyclientleftview': {
      const leftIds = new Set(rows.map((r) => Number(r.clid ?? 0)));
      const firstReason = Number(rows[0]?.reasonid ?? 8);
      const selfWasRemoved = leftIds.has(state.selfClientId);
      const peerLeftCurrentChannel = state.clients.some(
        (c) =>
          leftIds.has(c.clid) &&
          c.clid !== state.selfClientId &&
          c.cid === state.currentChannelId
      );
      ctx.patchState({
        clients: state.clients.filter((c) => !leftIds.has(c.clid)),
      });
      if (selfWasRemoved) {
        if (firstReason === 6) {
          soundCues.play('you_were_banned');
        } else if (firstReason === 5) {
          soundCues.play('you_kicked_server');
        }
      } else if (peerLeftCurrentChannel && !state.outputDeafened) {
        soundCues.play('user_disconnected_current_channel');
      }
      return true;
    }

    case 'notifyclientmoved': {
      const moves = new Map<number, { ctid: number; reasonid: number; invokerid: number }>();
      for (const r of rows) {
        moves.set(Number(r.clid ?? 0), {
          ctid: Number(r.ctid ?? r.cid ?? 1),
          reasonid: Number(r.reasonid ?? 0),
          invokerid: Number(r.invokerid ?? 0),
        });
      }
      const prevSelfCid = state.currentChannelId;
      const selfMove = moves.get(state.selfClientId);
      const selfNewCid = selfMove?.ctid;
      let peerJoined = false;
      let peerMovedIn = false;
      let peerLeft = false;
      let peerMovedOut = false;
      for (const c of state.clients) {
        if (c.clid === state.selfClientId) continue;
        const mv = moves.get(c.clid);
        if (mv && mv.ctid !== c.cid) {
          const byAdmin = mv.invokerid > 0 && mv.invokerid !== c.clid;
          if (mv.ctid === prevSelfCid) {
            if (byAdmin) peerMovedIn = true;
            else peerJoined = true;
          }
          if (c.cid === prevSelfCid) {
            if (byAdmin) peerMovedOut = true;
            else peerLeft = true;
          }
        }
      }
      const nextClients = state.clients.map((c) =>
        moves.has(c.clid) ? { ...c, cid: moves.get(c.clid)!.ctid } : c
      );
      ctx.patchState({
        clients: nextClients,
        currentChannelId: selfNewCid ?? state.currentChannelId,
      });
      if (!state.outputDeafened) {
        if (selfMove && selfMove.ctid !== prevSelfCid) {
          if (selfMove.reasonid === 4) {
            soundCues.play('you_kicked_channel');
          } else if (
            selfMove.invokerid > 0 &&
            selfMove.invokerid !== state.selfClientId
          ) {
            soundCues.play('you_were_moved');
          }
        } else if (peerMovedIn) {
          soundCues.play('user_moved_to_channel');
        } else if (peerJoined) {
          soundCues.play('user_joined_channel');
        } else if (peerMovedOut) {
          soundCues.play('user_moved_from_channel');
        } else if (peerLeft) {
          soundCues.play('user_left_channel');
        }
      }
      return true;
    }

    case 'notifyclientupdated': {
      const map = new Map(state.clients.map((c) => [c.clid, c]));
      for (const r of rows) {
        const clid = Number(r.clid ?? 0);
        const existing = map.get(clid);
        if (existing) {
          map.set(clid, parseClientRow(r, existing));
        }
      }
      ctx.patchState({ clients: Array.from(map.values()) });
      return true;
    }

    case 'notifyservergroupclientadd':
    case 'notifyservergroupclientdel': {
      const isAdd = cmd === 'notifyservergroupclientadd';
      let nextClients = [...state.clients];
      for (const r of rows) {
        const sgid = Number(r.sgid ?? 0);
        const clid = Number(r.clid ?? 0);
        const cldbid = Number(r.cldbid ?? 0);
        if (!sgid) continue;
        nextClients = nextClients.map((c) => {
          if ((clid > 0 && c.clid === clid) || (cldbid > 0 && c.client_database_id === cldbid)) {
            const groups = isAdd
              ? Array.from(new Set([...c.client_servergroups, sgid]))
              : c.client_servergroups.filter((g) => g !== sgid);
            return { ...c, client_servergroups: groups.length > 0 ? groups : [8] };
          }
          return c;
        });
      }
      ctx.patchState({ clients: nextClients });
      return true;
    }

    case 'notifyclientchannelgroupchanged': {
      let nextClients = [...state.clients];
      for (const r of rows) {
        const cgid = Number(r.cgid ?? 0);
        const clid = Number(r.clid ?? 0);
        const cid = Number(r.cid ?? 0);
        if (!cgid) continue;
        nextClients = nextClients.map((c) =>
          c.clid === clid && (!cid || c.cid === cid)
            ? { ...c, client_channel_group_id: cgid }
            : c
        );
      }
      ctx.patchState({ clients: nextClients });
      return true;
    }

    case 'notifytalkstatus':
    case 'notifytalkstatuschange': {
      for (const r of rows) {
        const clid = Number(r.clid ?? 0);
        const status = Number(r.status ?? r.is_talking ?? 0);
        const isWhisper = Number(r.isreceivedwhisper ?? r.is_whisper ?? 0) === 1;
        ctx.setClientTalkingState(clid, status > 0, isWhisper);
      }
      return true;
    }

    case 'notifytextmessage': {
      let shouldPlayChatCue = false;
      for (const r of rows) {
        const targetmode = (Number(r.targetmode ?? 2) as 1 | 2 | 3) || 2;
        const invokerid = Number(r.invokerid ?? 0);
        if (invokerid !== state.selfClientId) {
          shouldPlayChatCue = true;
        }
        const msg: ChatMessage = {
          id: `${Date.now()}-${Math.random().toString(36).slice(2, 7)}`,
          timestamp: Date.now(),
          targetmode,
          targetClientId: r.target !== undefined ? Number(r.target) : undefined,
          invokerid,
          invokername: String(r.invokername ?? 'Client'),
          invokeruid: String(r.invokeruid ?? ''),
          msg: String(r.msg ?? ''),
        };
        ctx.patchState({
          chatMessages: [...ctx.getState().chatMessages.slice(-299), msg],
        });
      }
      if (shouldPlayChatCue && !ctx.getState().outputDeafened) {
        soundCues.play('chat_message');
      }
      return true;
    }

    case 'notifyclientpoke': {
      const r = rows[0];
      const invokerName = String(r.invokername ?? 'Jemand');
      const message = String(r.msg ?? 'hat dich angestupst!');
      soundCues.play('poke_received');
      ctx.patchState({
        pokeAlert: { invokerName, message, at: Date.now() },
      });
      ctx.appendSystemChat(`⚡ POKE von ${invokerName}: ${message}`);
      return true;
    }

    case 'notifybroadcasts':
    case 'broadcastlist': {
      const activeKeys = new Set<string>();
      for (const r of rows) {
        if (r.empty === '1') continue;
        const clid = Number(r.clid ?? r.bid ?? r.sclid ?? r.scl ?? 0);
        const bt = Number(r.bt ?? 0);
        if (clid > 0 && (bt === 0 || bt === 1)) {
          const kind: 'camera' | 'screen' = bt === 1 ? 'screen' : 'camera';
          activeKeys.add(`${clid}:${kind}`);
        }
      }
      const nextStreams = { ...state.activeVideoStreams };
      let changed = false;
      for (const key of Object.keys(nextStreams)) {
        if (!activeKeys.has(key)) {
          delete nextStreams[key];
          ctx.videoEngine.removeStream(key);
          changed = true;
        }
      }
      if (changed) {
        ctx.patchState({ activeVideoStreams: nextStreams });
      }
      return true;
    }

    default:
      return false;
  }
}
