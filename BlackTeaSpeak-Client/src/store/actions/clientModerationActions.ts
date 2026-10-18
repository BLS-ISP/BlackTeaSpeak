import { soundCues } from '../../media/SoundCueEngine';
import { StoreContext } from '../state/blackTeaStateTypes';
import { moveSelfToChannel } from './connectionActions';

export async function toggleAwayAction(
  ctx: StoreContext,
  away?: boolean,
  message = ''
): Promise<void> {
  const state = ctx.getState();
  const selfClient = state.clients.find((c) => c.clid === state.selfClientId);
  const nextAway = away !== undefined ? away : !selfClient?.client_away;
  const awayMsg = nextAway ? message : '';
  if (state.connectionState === 'connected') {
    await ctx.protocol
      .sendCommand('clientupdate', [
        {
          client_away: nextAway ? '1' : '0',
          client_away_message: awayMsg,
        },
      ])
      .catch(() => {});
  }
  const curState = ctx.getState();
  ctx.patchState({
    clients: curState.clients.map((c) =>
      c.clid === curState.selfClientId
        ? { ...c, client_away: nextAway, client_away_message: awayMsg }
        : c
    ),
  });
  if (!ctx.getState().outputDeafened) {
    soundCues.play(nextAway ? 'away_activated' : 'away_deactivated');
  }
  ctx.showToast(
    nextAway
      ? `Status: Abwesend${awayMsg ? ` (${awayMsg})` : ''}`
      : 'Status: Online / Anwesend'
  );
}

export async function toggleChannelCommanderAction(ctx: StoreContext): Promise<void> {
  const state = ctx.getState();
  const selfClient = state.clients.find((c) => c.clid === state.selfClientId);
  const nextCommander = !selfClient?.client_is_channel_commander;
  if (state.connectionState === 'connected') {
    await ctx.protocol
      .sendCommand('clientupdate', [
        {
          client_is_channel_commander: nextCommander ? '1' : '0',
        },
      ])
      .catch(() => {});
  }
  const curState = ctx.getState();
  ctx.patchState({
    clients: curState.clients.map((c) =>
      c.clid === curState.selfClientId
        ? { ...c, client_is_channel_commander: nextCommander }
        : c
    ),
  });
  ctx.showToast(
    nextCommander ? 'Channel Commander aktiviert' : 'Channel Commander deaktiviert'
  );
}

export async function requestTalkPowerAction(
  ctx: StoreContext,
  message: string
): Promise<void> {
  const ts = Math.floor(Date.now() / 1000);
  await ctx.protocol.sendCommand('clientupdate', [
    {
      client_talk_request: '1',
      client_talk_request_msg: message.trim(),
    },
  ]);
  const state = ctx.getState();
  ctx.patchState({
    clients: state.clients.map((c) =>
      c.clid === state.selfClientId
        ? { ...c, client_talk_request: ts, client_talk_request_msg: message.trim() }
        : c
    ),
  });
  ctx.showToast('Talk-Power angefordert');
}

export async function cancelTalkPowerRequestAction(ctx: StoreContext): Promise<void> {
  await ctx.protocol.sendCommand('clientupdate', [
    {
      client_talk_request: '0',
      client_talk_request_msg: '',
    },
  ]);
  const state = ctx.getState();
  ctx.patchState({
    clients: state.clients.map((c) =>
      c.clid === state.selfClientId
        ? { ...c, client_talk_request: 0, client_talk_request_msg: '' }
        : c
    ),
  });
  ctx.showToast('Talk-Power-Anfrage zurückgezogen');
}

export async function setClientIsTalkerAction(
  ctx: StoreContext,
  clid: number,
  isTalker: boolean
): Promise<void> {
  await ctx.protocol.sendCommand('clientedit', [
    {
      clid: String(clid),
      client_is_talker: isTalker ? '1' : '0',
    },
  ]);
  const state = ctx.getState();
  ctx.patchState({
    clients: state.clients.map((c) =>
      c.clid === clid
        ? {
            ...c,
            client_is_talker: isTalker,
            client_talk_request: isTalker ? 0 : c.client_talk_request,
            client_talk_request_msg: isTalker ? '' : c.client_talk_request_msg,
          }
        : c
    ),
  });
  ctx.showToast(isTalker ? 'Talk-Power erteilt' : 'Talk-Power entzogen');
}

export async function editClientDescriptionAction(
  ctx: StoreContext,
  clid: number,
  description: string
): Promise<void> {
  if (clid === ctx.getState().selfClientId) {
    await ctx.protocol.sendCommand('clientupdate', [{ client_description: description }]);
  } else {
    await ctx.protocol.sendCommand('clientedit', [
      { clid: String(clid), client_description: description },
    ]);
  }
  const state = ctx.getState();
  ctx.patchState({
    clients: state.clients.map((c) =>
      c.clid === clid ? { ...c, client_description: description } : c
    ),
  });
  ctx.showToast('Client-Beschreibung gespeichert');
}

export async function moveClientToChannelAction(
  ctx: StoreContext,
  clid: number,
  cid: number,
  password = ''
): Promise<void> {
  if (clid === ctx.getState().selfClientId) {
    await moveSelfToChannel(ctx, cid, password);
    return;
  }
  await ctx.protocol.sendCommand('clientmove', [
    {
      clid: String(clid),
      cid: String(cid),
      cpw: password,
    },
  ]);
  const state = ctx.getState();
  ctx.patchState({
    clients: state.clients.map((c) => (c.clid === clid ? { ...c, cid } : c)),
  });
  ctx.showToast('Client verschoben');
}

export async function pokeClientAction(
  ctx: StoreContext,
  clid: number,
  msg: string
): Promise<void> {
  await ctx.protocol.sendCommand('clientpoke', [{ clid: String(clid), msg }]);
  ctx.showToast('Poke gesendet');
}

export async function kickClientAction(
  ctx: StoreContext,
  clid: number,
  fromServer: boolean,
  reason: string
): Promise<void> {
  await ctx.protocol.sendCommand('clientkick', [
    {
      clid: String(clid),
      reasonid: fromServer ? '5' : '4',
      reasonmsg: reason,
    },
  ]);
  ctx.showToast('Client gekickt');
}

export async function banClientAction(
  ctx: StoreContext,
  clid: number,
  timeSeconds: number,
  reason: string
): Promise<void> {
  await ctx.protocol.sendCommand('banclient', [
    {
      clid: String(clid),
      time: String(timeSeconds),
      banreason: reason,
    },
  ]);
  ctx.showToast('Client gebannt');
}

export async function refreshModerationListsAction(ctx: StoreContext): Promise<void> {
  ctx.patchState({ bans: [], complaints: [], offlineMessages: [] });
  await ctx.protocol.sendCommand('banlist').catch(() => {});
  await ctx.protocol.sendCommand('complainlist').catch(() => {});
  await ctx.protocol.sendCommand('messagelist').catch(() => {});
}

export async function addBanRuleAction(
  ctx: StoreContext,
  params: {
    ip?: string;
    name?: string;
    uid?: string;
    timeSeconds: number;
    reason: string;
  }
): Promise<void> {
  await ctx.protocol.sendCommand('banadd', [
    {
      ip: params.ip || undefined,
      name: params.name || undefined,
      uid: params.uid || undefined,
      time: String(params.timeSeconds),
      banreason: params.reason,
    },
  ]);
  await refreshModerationListsAction(ctx);
  ctx.showToast('Ban-Regel hinzugefügt');
}

export async function deleteBanRuleAction(ctx: StoreContext, banid: number): Promise<void> {
  await ctx.protocol.sendCommand('bandel', [{ banid: String(banid) }]);
  await refreshModerationListsAction(ctx);
  ctx.showToast(`Ban #${banid} entfernt`);
}

export async function clearAllBansAction(ctx: StoreContext): Promise<void> {
  await ctx.protocol.sendCommand('bandelall');
  await refreshModerationListsAction(ctx);
  ctx.showToast('Alle Ban-Regeln gelöscht');
}

export async function submitComplaintAction(
  ctx: StoreContext,
  tcldbid: number,
  message: string
): Promise<void> {
  await ctx.protocol.sendCommand('complainadd', [{ tcldbid: String(tcldbid), message }]);
  await refreshModerationListsAction(ctx);
  ctx.showToast('Beschwerde eingereicht');
}

export async function deleteComplaintAction(
  ctx: StoreContext,
  tcldbid: number,
  fcldbid: number
): Promise<void> {
  await ctx.protocol.sendCommand('complaindel', [
    { tcldbid: String(tcldbid), fcldbid: String(fcldbid) },
  ]);
  await refreshModerationListsAction(ctx);
  ctx.showToast('Beschwerde gelöscht');
}

export async function deleteAllComplaintsAction(
  ctx: StoreContext,
  tcldbid: number
): Promise<void> {
  await ctx.protocol.sendCommand('complaindelall', [{ tcldbid: String(tcldbid) }]);
  await refreshModerationListsAction(ctx);
  ctx.showToast('Alle Beschwerden gegen Client gelöscht');
}

export async function sendOfflineMessageAction(
  ctx: StoreContext,
  cluid: string,
  subject: string,
  message: string
): Promise<void> {
  await ctx.protocol.sendCommand('messageadd', [{ cluid, subject, message }]);
  await refreshModerationListsAction(ctx);
  ctx.showToast('Offline-Nachricht gesendet');
}

export async function readOfflineMessageAction(
  ctx: StoreContext,
  msgid: number
): Promise<string> {
  const frames = await ctx.protocol.sendCommand('messageget', [{ msgid: String(msgid) }]);
  await ctx.protocol
    .sendCommand('messageupdateflag', [{ msgid: String(msgid), flag: '1' }])
    .catch(() => {});
  const row = frames.find((f) => f.data && f.data.length > 0)?.data?.[0];
  const body = String(row?.message ?? '');
  const state = ctx.getState();
  ctx.patchState({
    offlineMessages: state.offlineMessages.map((m) =>
      m.msgid === msgid ? { ...m, flag_read: true, message: body || m.message } : m
    ),
  });
  return body;
}

export async function deleteOfflineMessageAction(
  ctx: StoreContext,
  msgid: number
): Promise<void> {
  await ctx.protocol.sendCommand('messagedel', [{ msgid: String(msgid) }]);
  await refreshModerationListsAction(ctx);
  ctx.showToast(`Offline-Nachricht #${msgid} gelöscht`);
}
