import {
  DesktopAppConfig,
  DesktopFavorite,
  DesktopIdentity,
} from '../../types/protocol';
import {
  fetchDesktopAudioDevices,
  fetchPluginSdkStatus,
  generateDesktopIdentity,
  loadDesktopConfig,
  saveDesktopConfig,
  setPluginSdkEnabledBridge,
} from '../../protocols/tauriBridge';
import { StoreContext } from '../state/blackTeaStateTypes';
import { connectToServer, disconnectFromServer } from './connectionActions';

export async function initializeDesktopEnvironmentAction(
  ctx: StoreContext
): Promise<void> {
  const [cfg, audioDevices, sdkStatus] = await Promise.all([
    loadDesktopConfig(),
    fetchDesktopAudioDevices(),
    fetchPluginSdkStatus(),
  ]);

  const current = ctx.getState();
  const activeIdentity = cfg.identities[0] ?? current.desktopIdentities[0];
  ctx.patchState({
    desktopIdentities: cfg.identities.length > 0 ? cfg.identities : current.desktopIdentities,
    activeIdentityId: activeIdentity?.id ?? current.activeIdentityId,
    identityPublicKey: activeIdentity?.public_key ?? current.identityPublicKey,
    nickname: activeIdentity?.default_nickname || current.nickname,
    desktopFavorites: cfg.favorites,
    desktopAudioDevices: audioDevices,
    pluginSdkStatus: sdkStatus,
  });
}

async function persistCurrentDesktopConfig(ctx: StoreContext): Promise<void> {
  const state = ctx.getState();
  const config: DesktopAppConfig = {
    identities: state.desktopIdentities,
    favorites: state.desktopFavorites,
  };
  await saveDesktopConfig(config);
}

export function selectDesktopIdentityAction(
  ctx: StoreContext,
  identityId: string
): void {
  const state = ctx.getState();
  const found = state.desktopIdentities.find((i) => i.id === identityId);
  if (!found) return;

  ctx.patchState({
    activeIdentityId: found.id,
    identityPublicKey: found.public_key,
    nickname: found.default_nickname || state.nickname,
    pttKey: found.ptt_hotkey || state.pttKey,
  });
  ctx.showToast(`Identität '${found.name}' ausgewählt`);
}

export async function createDesktopIdentityAction(
  ctx: StoreContext,
  name: string,
  defaultNickname = 'BlackTeaUser'
): Promise<void> {
  const cleanName = name.trim() || 'Neue Identität';
  const generated = await generateDesktopIdentity(cleanName);
  const identity: DesktopIdentity = {
    ...generated,
    name: cleanName,
    default_nickname: defaultNickname.trim() || generated.default_nickname || 'BlackTeaUser',
  };

  const state = ctx.getState();
  const nextIdentities = [...state.desktopIdentities, identity];
  ctx.patchState({
    desktopIdentities: nextIdentities,
    activeIdentityId: identity.id,
    identityPublicKey: identity.public_key,
    nickname: identity.default_nickname,
  });
  await persistCurrentDesktopConfig(ctx);
  ctx.showToast(`Kryptografische Ed25519-Identität '${cleanName}' erstellt`);
}

export async function deleteDesktopIdentityAction(
  ctx: StoreContext,
  identityId: string
): Promise<void> {
  const state = ctx.getState();
  if (state.desktopIdentities.length <= 1) {
    ctx.showToast('Mindestens eine Identität muss erhalten bleiben');
    return;
  }
  const filtered = state.desktopIdentities.filter((i) => i.id !== identityId);
  const nextActive =
    state.activeIdentityId === identityId ? filtered[0] : filtered.find((i) => i.id === state.activeIdentityId) ?? filtered[0];

  ctx.patchState({
    desktopIdentities: filtered,
    activeIdentityId: nextActive.id,
    identityPublicKey: nextActive.public_key,
  });
  await persistCurrentDesktopConfig(ctx);
  ctx.showToast('Identität gelöscht');
}

export async function updateActiveIdentitySettingsAction(
  ctx: StoreContext,
  patch: Partial<DesktopIdentity>
): Promise<void> {
  const state = ctx.getState();
  const nextIdentities = state.desktopIdentities.map((id) =>
    id.id === state.activeIdentityId ? { ...id, ...patch } : id
  );
  const updatedActive = nextIdentities.find((i) => i.id === state.activeIdentityId);
  ctx.patchState({
    desktopIdentities: nextIdentities,
    ...(updatedActive?.default_nickname ? { nickname: updatedActive.default_nickname } : {}),
    ...(updatedActive?.ptt_hotkey ? { pttKey: updatedActive.ptt_hotkey } : {}),
  });
  await persistCurrentDesktopConfig(ctx);
}

export async function saveCurrentServerAsFavoriteAction(
  ctx: StoreContext,
  customName?: string
): Promise<void> {
  const state = ctx.getState();
  const cleanHost = state.host.trim() || '127.0.0.1';
  const address = cleanHost.includes(':') ? cleanHost : `${cleanHost}:${state.port || 9989}`;
  const favName =
    customName?.trim() ||
    (state.connectionState === 'connected'
      ? state.serverInfo.virtualserver_name
      : `Server ${address}`);

  const newFav: DesktopFavorite = {
    id: `fav_${Date.now()}_${Math.random().toString(36).slice(2, 6)}`,
    name: favName,
    address,
    password: state.serverPassword || '',
    nickname: state.nickname || 'BlackTeaUser',
    identity_id: state.activeIdentityId,
  };

  ctx.patchState({
    desktopFavorites: [...state.desktopFavorites, newFav],
  });
  await persistCurrentDesktopConfig(ctx);
  ctx.showToast(`★ '${favName}' (${address}) als Favorit gespeichert`);
}

export async function addOrUpdateDesktopFavoriteAction(
  ctx: StoreContext,
  fav: Omit<DesktopFavorite, 'id'> & { id?: string }
): Promise<void> {
  const state = ctx.getState();
  const id = fav.id || `fav_${Date.now()}_${Math.random().toString(36).slice(2, 6)}`;
  const entry: DesktopFavorite = {
    id,
    name: fav.name.trim() || fav.address.trim(),
    address: fav.address.trim() || '127.0.0.1:9989',
    password: fav.password ?? '',
    nickname: fav.nickname.trim() || state.nickname || 'BlackTeaUser',
    identity_id: fav.identity_id || state.activeIdentityId,
  };

  const exists = state.desktopFavorites.some((f) => f.id === id);
  const nextFavorites = exists
    ? state.desktopFavorites.map((f) => (f.id === id ? entry : f))
    : [...state.desktopFavorites, entry];

  ctx.patchState({ desktopFavorites: nextFavorites });
  await persistCurrentDesktopConfig(ctx);
  ctx.showToast(`Favorit '${entry.name}' gespeichert`);
}

export async function deleteDesktopFavoriteAction(
  ctx: StoreContext,
  favId: string
): Promise<void> {
  const state = ctx.getState();
  ctx.patchState({
    desktopFavorites: state.desktopFavorites.filter((f) => f.id !== favId),
  });
  await persistCurrentDesktopConfig(ctx);
  ctx.showToast('Favorit entfernt');
}

export async function connectToFavoriteAction(
  ctx: StoreContext,
  fav: DesktopFavorite
): Promise<void> {
  const state = ctx.getState();
  if (state.connectionState === 'connected') {
    disconnectFromServer(ctx);
  }

  let host = fav.address.trim() || '127.0.0.1';
  let port = 9989;
  if (host.includes(':')) {
    const [h, p] = host.split(':');
    host = h || '127.0.0.1';
    const parsedPort = Number(p);
    if (parsedPort > 0) port = parsedPort;
  }

  const matchedIdentity =
    state.desktopIdentities.find((i) => i.id === fav.identity_id) ??
    state.desktopIdentities.find((i) => i.id === state.activeIdentityId) ??
    state.desktopIdentities[0];

  ctx.patchState({
    host,
    port,
    nickname: fav.nickname || matchedIdentity?.default_nickname || state.nickname,
    serverPassword: fav.password || '',
    activeIdentityId: matchedIdentity?.id ?? state.activeIdentityId,
    identityPublicKey: matchedIdentity?.public_key ?? state.identityPublicKey,
    activeModal: null,
  });

  await connectToServer(ctx);
}

export async function refreshDesktopHardwareAndSdkAction(
  ctx: StoreContext
): Promise<void> {
  const [audioDevices, sdkStatus] = await Promise.all([
    fetchDesktopAudioDevices(),
    fetchPluginSdkStatus(),
  ]);
  ctx.patchState({
    desktopAudioDevices: audioDevices,
    pluginSdkStatus: sdkStatus,
  });
}

export async function togglePluginSdkAction(
  ctx: StoreContext,
  enabled: boolean
): Promise<void> {
  const status = await setPluginSdkEnabledBridge(enabled);
  ctx.patchState({ pluginSdkStatus: status });
  ctx.showToast(
    enabled
      ? `Plugin SDK WebSocket (${status.listen_addr}) aktiviert`
      : 'Plugin SDK WebSocket deaktiviert'
  );
}
