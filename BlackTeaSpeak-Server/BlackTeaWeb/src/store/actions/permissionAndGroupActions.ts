import { soundCues } from '../../media/SoundCueEngine';
import { StoreContext } from '../state/blackTeaStateTypes';

export type PermissionTier =
  | 'servergroup'
  | 'channelgroup'
  | 'channel'
  | 'client'
  | 'channelclient';

export async function loadPermissionsDataAction(
  ctx: StoreContext,
  tier: PermissionTier,
  id1: number,
  id2 = 0
): Promise<void> {
  if (ctx.getState().permissionCatalog.length === 0) {
    await ctx.protocol.sendCommand('permissionlist').catch(() => {});
  }
  if (tier === 'servergroup') {
    await ctx.protocol.sendCommand('servergrouppermlist', [{ sgid: String(id1) }], ['-permsid']);
  } else if (tier === 'channelgroup') {
    await ctx.protocol.sendCommand(
      'channelgrouppermlist',
      [{ cgid: String(id1) }],
      ['-permsid']
    );
  } else if (tier === 'channel') {
    await ctx.protocol.sendCommand('channelpermlist', [{ cid: String(id1) }], ['-permsid']);
  } else if (tier === 'client') {
    await ctx.protocol.sendCommand('clientpermlist', [{ cldbid: String(id1) }], ['-permsid']);
  } else if (tier === 'channelclient') {
    await ctx.protocol.sendCommand(
      'channelclientpermlist',
      [{ cid: String(id1), cldbid: String(id2) }],
      ['-permsid']
    );
  }
}

export async function setPermissionValueAction(
  ctx: StoreContext,
  tier: PermissionTier,
  id1: number,
  id2: number,
  permsid: string,
  permvalue: number,
  permskip = false,
  permnegated = false
): Promise<void> {
  const base = {
    permsid,
    permvalue: String(permvalue),
    permskip: permskip ? '1' : '0',
    permnegated: permnegated ? '1' : '0',
  };
  if (tier === 'servergroup') {
    await ctx.protocol.sendCommand('servergroupaddperm', [{ sgid: String(id1), ...base }]);
  } else if (tier === 'channelgroup') {
    await ctx.protocol.sendCommand('channelgroupaddperm', [{ cgid: String(id1), ...base }]);
  } else if (tier === 'channel') {
    await ctx.protocol.sendCommand('channeladdperm', [{ cid: String(id1), ...base }]);
  } else if (tier === 'client') {
    await ctx.protocol.sendCommand('clientaddperm', [{ cldbid: String(id1), ...base }]);
  } else {
    await ctx.protocol.sendCommand('channelclientaddperm', [
      { cid: String(id1), cldbid: String(id2), ...base },
    ]);
  }
  await loadPermissionsDataAction(ctx, tier, id1, id2);
  ctx.showToast(`Permission ${permsid} = ${permvalue} gespeichert`);
}

export async function removePermissionValueAction(
  ctx: StoreContext,
  tier: PermissionTier,
  id1: number,
  id2: number,
  permsid: string
): Promise<void> {
  if (tier === 'servergroup') {
    await ctx.protocol.sendCommand('servergroupdelperm', [{ sgid: String(id1), permsid }]);
  } else if (tier === 'channelgroup') {
    await ctx.protocol.sendCommand('channelgroupdelperm', [{ cgid: String(id1), permsid }]);
  } else if (tier === 'channel') {
    await ctx.protocol.sendCommand('channeldelperm', [{ cid: String(id1), permsid }]);
  } else if (tier === 'client') {
    await ctx.protocol.sendCommand('clientdelperm', [{ cldbid: String(id1), permsid }]);
  } else {
    await ctx.protocol.sendCommand('channelclientdelperm', [
      { cid: String(id1), cldbid: String(id2), permsid },
    ]);
  }
  await loadPermissionsDataAction(ctx, tier, id1, id2);
  ctx.showToast(`Permission ${permsid} entfernt`);
}

export async function createServerGroupAction(ctx: StoreContext, name: string): Promise<void> {
  await ctx.protocol.sendCommand('servergroupadd', [{ name, type: '1' }]);
  await ctx.protocol.sendCommand('servergrouplist');
  ctx.showToast(`Servergruppe '${name}' erstellt`);
}

export async function renameServerGroupAction(
  ctx: StoreContext,
  sgid: number,
  name: string
): Promise<void> {
  await ctx.protocol.sendCommand('servergrouprename', [{ sgid: String(sgid), name }]);
  await ctx.protocol.sendCommand('servergrouplist');
  ctx.showToast(`Servergruppe #${sgid} in '${name}' umbenannt`);
}

export async function copyServerGroupAction(
  ctx: StoreContext,
  ssgid: number,
  name: string
): Promise<void> {
  await ctx.protocol.sendCommand('servergroupcopy', [
    { ssgid: String(ssgid), tsgid: '0', name, type: '1' },
  ]);
  await ctx.protocol.sendCommand('servergrouplist');
  ctx.showToast(`Servergruppe '${name}' als Kopie von #${ssgid} erstellt`);
}

export async function deleteServerGroupAction(ctx: StoreContext, sgid: number): Promise<void> {
  await ctx.protocol.sendCommand('servergroupdel', [{ sgid: String(sgid), force: '1' }]);
  await ctx.protocol.sendCommand('servergrouplist');
  ctx.showToast(`Servergruppe #${sgid} gelöscht`);
}

export async function createChannelGroupAction(ctx: StoreContext, name: string): Promise<void> {
  await ctx.protocol.sendCommand('channelgroupadd', [{ name, type: '1' }]);
  await ctx.protocol.sendCommand('channelgrouplist');
  ctx.showToast(`Channelgruppe '${name}' erstellt`);
}

export async function renameChannelGroupAction(
  ctx: StoreContext,
  cgid: number,
  name: string
): Promise<void> {
  await ctx.protocol.sendCommand('channelgrouprename', [{ cgid: String(cgid), name }]);
  await ctx.protocol.sendCommand('channelgrouplist');
  ctx.showToast(`Channelgruppe #${cgid} in '${name}' umbenannt`);
}

export async function deleteChannelGroupAction(ctx: StoreContext, cgid: number): Promise<void> {
  await ctx.protocol.sendCommand('channelgroupdel', [{ cgid: String(cgid), force: '1' }]);
  await ctx.protocol.sendCommand('channelgrouplist');
  ctx.showToast(`Channelgruppe #${cgid} gelöscht`);
}

export async function toggleClientServerGroupAction(
  ctx: StoreContext,
  cldbid: number,
  sgid: number,
  hasGroup: boolean
): Promise<void> {
  await ctx.protocol.sendCommand(hasGroup ? 'servergroupdelclient' : 'servergroupaddclient', [
    { sgid: String(sgid), cldbid: String(cldbid) },
  ]);
  const state = ctx.getState();
  ctx.patchState({
    clients: state.clients.map((c) => {
      if (c.client_database_id !== cldbid) return c;
      const nextGroups = hasGroup
        ? c.client_servergroups.filter((g) => g !== sgid)
        : Array.from(new Set([...c.client_servergroups, sgid]));
      return {
        ...c,
        client_servergroups: nextGroups.length > 0 ? nextGroups : [8],
      };
    }),
  });
  if (!ctx.getState().outputDeafened) {
    soundCues.play(hasGroup ? 'servergroup_revoked' : 'servergroup_assigned');
  }
  ctx.showToast(hasGroup ? 'Aus Servergruppe entfernt' : 'Zu Servergruppe hinzugefügt');
}

export async function setClientChannelGroupAction(
  ctx: StoreContext,
  cgid: number,
  cid: number,
  cldbid: number
): Promise<void> {
  await ctx.protocol.sendCommand('setclientchannelgroup', [
    { cgid: String(cgid), cid: String(cid), cldbid: String(cldbid) },
  ]);
  const state = ctx.getState();
  ctx.patchState({
    clients: state.clients.map((c) =>
      c.client_database_id === cldbid && c.cid === cid
        ? { ...c, client_channel_group_id: cgid }
        : c
    ),
  });
  ctx.showToast('Channelgruppe aktualisiert');
}

export async function refreshTokensAction(ctx: StoreContext): Promise<void> {
  ctx.patchState({ tokens: [] });
  await ctx.protocol.sendCommand('privilegekeylist').catch(() => {});
}

export async function createTokenAction(
  ctx: StoreContext,
  tokenType: number,
  id1: number,
  id2: number,
  description: string
): Promise<void> {
  await ctx.protocol.sendCommand('privilegekeyadd', [
    {
      tokentype: String(tokenType),
      tokenid1: String(id1),
      tokenid2: String(id2),
      tokendescription: description,
    },
  ]);
  await refreshTokensAction(ctx);
  ctx.showToast('Privilege Key generiert');
}

export async function useTokenAction(ctx: StoreContext, token: string): Promise<void> {
  const cleaned = token.trim();
  if (!cleaned) {
    throw new Error('Bitte gib einen gültigen Berechtigungsschlüssel ein.');
  }
  await ctx.protocol.sendCommand('privilegekeyuse', [{ token: cleaned }]);
  if (!ctx.getState().outputDeafened) soundCues.play('servergroup_assigned');
  ctx.showToast('Berechtigungsschlüssel erfolgreich eingelöst — Admin-Rechte freigeschaltet!');
  ctx.appendSystemChat(
    '🔑 Berechtigungsschlüssel erfolgreich eingelöst. Deine Servergruppen und Rechte wurden aktualisiert.'
  );
}

export async function deleteTokenAction(ctx: StoreContext, token: string): Promise<void> {
  await ctx.protocol.sendCommand('privilegekeydelete', [{ token }]);
  await refreshTokensAction(ctx);
  ctx.showToast('Privilege Key gelöscht');
}
