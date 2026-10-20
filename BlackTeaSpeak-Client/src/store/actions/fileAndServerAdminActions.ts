import { StoreContext } from '../state/blackTeaStateTypes';

export async function fetchChannelFilesAction(
  ctx: StoreContext,
  cid: number,
  path = '/'
): Promise<void> {
  ctx.patchState({ files: [], currentFilePath: path });
  await ctx.protocol
    .sendCommand('ftgetfilelist', [{ cid: String(cid), cpw: '', path }])
    .catch(() => {});
}

export async function createChannelDirectoryAction(
  ctx: StoreContext,
  cid: number,
  dirname: string
): Promise<void> {
  const curPath = ctx.getState().currentFilePath;
  const cleanDir = dirname.startsWith('/')
    ? dirname
    : `${curPath.replace(/\/$/, '')}/${dirname}`;
  await ctx.protocol.sendCommand('ftcreatedir', [
    { cid: String(cid), cpw: '', dirname: cleanDir },
  ]);
  await fetchChannelFilesAction(ctx, cid, curPath);
  ctx.showToast(`Ordner '${dirname}' angelegt`);
}

export async function renameChannelFileAction(
  ctx: StoreContext,
  cid: number,
  oldName: string,
  newName: string
): Promise<void> {
  const curPath = ctx.getState().currentFilePath;
  const base = curPath.replace(/\/$/, '');
  const oldPath = oldName.startsWith('/') ? oldName : `${base}/${oldName}`;
  const newPath = newName.startsWith('/') ? newName : `${base}/${newName}`;
  await ctx.protocol.sendCommand('ftrenamefile', [
    {
      cid: String(cid),
      cpw: '',
      tcid: String(cid),
      tcpw: '',
      oldname: oldPath,
      newname: newPath,
    },
  ]);
  await fetchChannelFilesAction(ctx, cid, curPath);
  ctx.showToast(`Umbenannt in '${newName}'`);
}

export async function deleteChannelFileAction(
  ctx: StoreContext,
  cid: number,
  name: string
): Promise<void> {
  const curPath = ctx.getState().currentFilePath;
  const fullPath = name.startsWith('/')
    ? name
    : `${curPath.replace(/\/$/, '')}/${name}`;
  await ctx.protocol.sendCommand('ftdeletefile', [
    { cid: String(cid), cpw: '', name: fullPath },
  ]);
  await fetchChannelFilesAction(ctx, cid, curPath);
  ctx.showToast(`Datei '${name}' gelöscht`);
}

export async function uploadChannelFileAction(
  ctx: StoreContext,
  cid: number,
  file: File
): Promise<void> {
  try {
    const curPath = ctx.getState().currentFilePath || '/';
    const safeName = file.name.replace(/[/\\]/g, '_');
    const fullPath = `${curPath.replace(/\/$/, '')}/${safeName}`;
    ctx.showToast(`⬆️ Lade '${safeName}' in Channel #${cid} hoch...`);

    const initResp = await ctx.protocol.sendCommand('ftinitupload', [
      {
        cid: String(cid),
        path: curPath,
        name: fullPath,
        size: String(file.size),
        overwrite: '1',
        resume: '0',
        cpw: '',
      },
    ]);
    const row = initResp[0]?.data?.[0];
    const ftkey = String(row?.ftkey ?? '');
    const ftPort = Number(row?.port ?? 30303);
    if (!ftkey) {
      throw new Error('Kein Transfer-Key vom Server erhalten (fehlende Berechtigung?)');
    }

    const rawHost = ctx.getState().host || '127.0.0.1';
    const cleanHost = rawHost.includes(':') ? rawHost.split(':')[0] : rawHost;
    const encodedKey = encodeURIComponent(ftkey);
    const endpoints = [
      `http://${cleanHost}:8081/api/files/transfer?transfer-key=${encodedKey}`,
      `https://${cleanHost}:8080/api/files/transfer?transfer-key=${encodedKey}`,
      `/api/files/transfer?transfer-key=${encodedKey}`,
    ];

    let uploaded = false;
    let lastErr = 'Transfer-Endpunkt nicht erreichbar';
    for (const url of endpoints) {
      try {
        const res = await fetch(url, {
          method: 'POST',
          headers: {
            'Content-Type': file.type || 'application/octet-stream',
            'X-Transfer-Key': ftkey,
          },
          body: file,
        });
        if (res.ok) {
          uploaded = true;
          break;
        }
        const errBody = await res.text().catch(() => '');
        lastErr = errBody || `HTTP ${res.status}`;
      } catch (e) {
        lastErr = e instanceof Error ? e.message : String(e);
      }
    }

    if (!uploaded) {
      const formData = new FormData();
      formData.append('file', file, safeName);
      const fallbackRes = await fetch(
        `https://${cleanHost}:${ftPort}/?transfer-key=${encodedKey}`,
        {
          method: 'POST',
          body: formData,
        }
      );
      if (!fallbackRes.ok) {
        throw new Error(lastErr);
      }
    }

    await fetchChannelFilesAction(ctx, cid, curPath);
    ctx.showToast(`✅ '${safeName}' erfolgreich in Channel #${cid} hochgeladen`);
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err);
    ctx.showToast(`Channel-Upload fehlgeschlagen: ${msg}`);
  }
}

export async function downloadChannelFileAction(
  ctx: StoreContext,
  cid: number,
  name: string
): Promise<void> {
  try {
    const curPath = ctx.getState().currentFilePath || '/';
    const fullPath = name.startsWith('/')
      ? name
      : `${curPath.replace(/\/$/, '')}/${name}`;
    const initResp = await ctx.protocol.sendCommand('ftinitdownload', [
      {
        cid: String(cid),
        path: curPath,
        name: fullPath,
        cpw: '',
        seekpos: '0',
      },
    ]);
    const row = initResp[0]?.data?.[0];
    const ftkey = String(row?.ftkey ?? '');
    const ftPort = Number(row?.port ?? 30303);
    if (!ftkey) {
      throw new Error('Kein Download-Key vom Server erhalten (fehlende Berechtigung?)');
    }

    const rawHost = ctx.getState().host || '127.0.0.1';
    const cleanHost = rawHost.includes(':') ? rawHost.split(':')[0] : rawHost;
    const encodedKey = encodeURIComponent(ftkey);
    const encodedName = encodeURIComponent(name);
    const endpoints = [
      `http://${cleanHost}:8081/api/files/transfer?transfer-key=${encodedKey}&file-name=${encodedName}`,
      `https://${cleanHost}:8080/api/files/transfer?transfer-key=${encodedKey}&file-name=${encodedName}`,
      `/api/files/transfer?transfer-key=${encodedKey}&file-name=${encodedName}`,
      `https://${cleanHost}:${ftPort}/?transfer-key=${encodedKey}&file-name=${encodedName}`,
    ];

    let blob: Blob | null = null;
    let lastErr = 'Download-Endpunkt nicht erreichbar';
    for (const url of endpoints) {
      try {
        const res = await fetch(url, { method: 'GET' });
        if (res.ok) {
          blob = await res.blob();
          break;
        }
        lastErr = `HTTP ${res.status}`;
      } catch (e) {
        lastErr = e instanceof Error ? e.message : String(e);
      }
    }

    if (!blob) {
      throw new Error(lastErr);
    }

    const objectUrl = URL.createObjectURL(blob);
    const anchor = document.createElement('a');
    anchor.href = objectUrl;
    anchor.download = name;
    document.body.appendChild(anchor);
    anchor.click();
    document.body.removeChild(anchor);
    window.setTimeout(() => URL.revokeObjectURL(objectUrl), 15000);
    ctx.showToast(`⬇️ Download gestartet: '${name}'`);
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err);
    ctx.showToast(`Download fehlgeschlagen: ${msg}`);
  }
}

export async function refreshServerAdminDataAction(ctx: StoreContext): Promise<void> {
  await ctx.protocol.sendCommand('serverlist').catch(() => {});
  await ctx.protocol.sendCommand('tsdnslist').catch(() => {});
}

export interface UpdateVirtualServerParams {
  name: string;
  welcomeMessage: string;
  maxClients: number;
  hostbannerUrl: string;
  hostbannerGfxUrl?: string;
  hostbuttonTooltip?: string;
  hostbuttonUrl?: string;
  hostmessage?: string;
  hostmessageMode?: number;
  codecEncryptionMode?: number;
  defaultServerGroup?: number;
  defaultChannelGroup?: number;
  antifloodPointsTickReduce?: number;
  antifloodPointsNeededCommandBlock?: number;
  antifloodPointsNeededIpBlock?: number;
}

export async function updateVirtualServerAction(
  ctx: StoreContext,
  params: UpdateVirtualServerParams
): Promise<void> {
  const payload: Record<string, string> = {
    virtualserver_name: params.name,
    virtualserver_welcomemessage: params.welcomeMessage,
    virtualserver_maxclients: String(params.maxClients),
    virtualserver_hostbanner_gfx_url: params.hostbannerGfxUrl ?? params.hostbannerUrl,
  };
  if (params.hostbannerUrl !== undefined && params.hostbannerGfxUrl !== undefined) {
    payload.virtualserver_hostbanner_url = params.hostbannerUrl;
  }
  if (params.hostbuttonTooltip !== undefined) {
    payload.virtualserver_hostbutton_tooltip = params.hostbuttonTooltip;
  }
  if (params.hostbuttonUrl !== undefined) {
    payload.virtualserver_hostbutton_url = params.hostbuttonUrl;
  }
  if (params.hostmessage !== undefined) {
    payload.virtualserver_hostmessage = params.hostmessage;
  }
  if (params.hostmessageMode !== undefined) {
    payload.virtualserver_hostmessage_mode = String(params.hostmessageMode);
  }
  if (params.codecEncryptionMode !== undefined) {
    payload.virtualserver_codec_encryption_mode = String(params.codecEncryptionMode);
  }
  if (params.defaultServerGroup !== undefined) {
    payload.virtualserver_default_server_group = String(params.defaultServerGroup);
  }
  if (params.defaultChannelGroup !== undefined) {
    payload.virtualserver_default_channel_group = String(params.defaultChannelGroup);
  }
  if (params.antifloodPointsTickReduce !== undefined) {
    payload.virtualserver_antiflood_points_tick_reduce = String(
      params.antifloodPointsTickReduce
    );
  }
  if (params.antifloodPointsNeededCommandBlock !== undefined) {
    payload.virtualserver_antiflood_points_needed_command_block = String(
      params.antifloodPointsNeededCommandBlock
    );
  }
  if (params.antifloodPointsNeededIpBlock !== undefined) {
    payload.virtualserver_antiflood_points_needed_ip_block = String(
      params.antifloodPointsNeededIpBlock
    );
  }
  await ctx.protocol.sendCommand('serveredit', [payload]);
  const state = ctx.getState();
  ctx.patchState({
    serverInfo: {
      ...state.serverInfo,
      virtualserver_name: params.name,
      virtualserver_welcomemessage: params.welcomeMessage,
      virtualserver_maxclients: params.maxClients,
      virtualserver_hostbanner_gfx_url: params.hostbannerGfxUrl ?? params.hostbannerUrl,
      ...(params.hostbannerGfxUrl !== undefined
        ? { virtualserver_hostbanner_url: params.hostbannerUrl }
        : {}),
      ...(params.hostbuttonTooltip !== undefined
        ? { virtualserver_hostbutton_tooltip: params.hostbuttonTooltip }
        : {}),
      ...(params.hostbuttonUrl !== undefined
        ? { virtualserver_hostbutton_url: params.hostbuttonUrl }
        : {}),
      ...(params.hostmessage !== undefined
        ? { virtualserver_hostmessage: params.hostmessage }
        : {}),
      ...(params.hostmessageMode !== undefined
        ? { virtualserver_hostmessage_mode: params.hostmessageMode }
        : {}),
      ...(params.codecEncryptionMode !== undefined
        ? { virtualserver_codec_encryption_mode: params.codecEncryptionMode }
        : {}),
      ...(params.defaultServerGroup !== undefined
        ? { virtualserver_default_server_group: params.defaultServerGroup }
        : {}),
      ...(params.defaultChannelGroup !== undefined
        ? { virtualserver_default_channel_group: params.defaultChannelGroup }
        : {}),
    },
  });
  ctx.showToast('Virtueller Server aktualisiert');
}

export async function createVirtualServerAction(
  ctx: StoreContext,
  name: string,
  port: number,
  maxClients = 32
): Promise<void> {
  await ctx.protocol.sendCommand('servercreate', [
    {
      virtualserver_name: name,
      virtualserver_port: String(port),
      virtualserver_maxclients: String(maxClients),
    },
  ]);
  await refreshServerAdminDataAction(ctx);
  ctx.showToast(`Virtueller Server '${name}' (Port ${port}) erstellt`);
}

export async function toggleVirtualServerStateAction(
  ctx: StoreContext,
  sid: number,
  start: boolean
): Promise<void> {
  await ctx.protocol.sendCommand(start ? 'serverstart' : 'serverstop', [
    { sid: String(sid) },
  ]);
  await refreshServerAdminDataAction(ctx);
  ctx.showToast(`Virtueller Server #${sid} ${start ? 'gestartet' : 'gestoppt'}`);
}

export async function deleteVirtualServerAction(
  ctx: StoreContext,
  sid: number
): Promise<void> {
  await ctx.protocol.sendCommand('serverdelete', [{ sid: String(sid) }]);
  await refreshServerAdminDataAction(ctx);
  ctx.showToast(`Virtueller Server #${sid} gelöscht`);
}

export async function upsertTsdnsAction(
  ctx: StoreContext,
  domain: string,
  target: string
): Promise<void> {
  await ctx.protocol.sendCommand('tsdnsadd', [{ domain, target }]);
  await refreshServerAdminDataAction(ctx);
  ctx.showToast(`TSDNS ${domain} -> ${target} gespeichert`);
}

export async function deleteTsdnsAction(ctx: StoreContext, domain: string): Promise<void> {
  await ctx.protocol.sendCommand('tsdnsdel', [{ domain }]);
  await refreshServerAdminDataAction(ctx);
  ctx.showToast(`TSDNS ${domain} entfernt`);
}
