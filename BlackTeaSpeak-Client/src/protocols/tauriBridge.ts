import { invoke } from '@tauri-apps/api/core';
import {
  DesktopAppConfig,
  DesktopAudioDevices,
  DesktopFavorite,
  DesktopIdentity,
  DesktopPluginSdkStatus,
  SpatialPosition,
} from '../types/protocol';

const LOCAL_CONFIG_STORAGE_KEY = 'blackteaspeak_desktop_config_v2';

export function isTauriRuntime(): boolean {
  return (
    typeof window !== 'undefined' &&
    ('__TAURI_INTERNALS__' in window || '__TAURI__' in window)
  );
}

function createFallbackIdentity(name = 'Standard-Identität'): DesktopIdentity {
  const randomBytes = new Uint8Array(24);
  if (typeof crypto !== 'undefined' && crypto.getRandomValues) {
    crypto.getRandomValues(randomBytes);
  }
  const pubKey = btoa(String.fromCharCode(...randomBytes));
  const uidBytes = randomBytes.slice(0, 20);
  const uid = btoa(String.fromCharCode(...uidBytes));
  return {
    id: `id_${Date.now()}_${Math.random().toString(36).slice(2, 7)}`,
    name,
    private_key: pubKey,
    public_key: pubKey,
    uid,
    default_nickname: 'BlackTeaUser',
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
}

export async function loadDesktopConfig(): Promise<DesktopAppConfig> {
  try {
    const cfg = await invoke<DesktopAppConfig>('load_config');
    if (cfg && Array.isArray(cfg.identities) && Array.isArray(cfg.favorites)) {
      if (cfg.identities.length === 0) {
        const defaultId = await generateDesktopIdentity('Standard-Identität');
        const nextCfg: DesktopAppConfig = {
          identities: [defaultId],
          favorites: cfg.favorites,
        };
        await saveDesktopConfig(nextCfg);
        return nextCfg;
      }
      return cfg;
    }
  } catch {
    // Fallback to localStorage when running outside Tauri
  }

  try {
    const raw = localStorage.getItem(LOCAL_CONFIG_STORAGE_KEY);
    if (raw) {
      const parsed = JSON.parse(raw) as DesktopAppConfig;
      if (Array.isArray(parsed.identities) && parsed.identities.length > 0) {
        return {
          identities: parsed.identities,
          favorites: Array.isArray(parsed.favorites) ? parsed.favorites : [],
        };
      }
    }
  } catch {
    // Ignore parse error
  }

  const fallbackIdentity = createFallbackIdentity('Standard-Identität');
  const defaultFavorite: DesktopFavorite = {
    id: 'fav_local_default',
    name: 'Lokaler BlackTeaSpeak Server',
    address: '127.0.0.1:9989',
    password: '',
    nickname: 'BlackTeaUser',
    identity_id: fallbackIdentity.id,
  };
  const initial: DesktopAppConfig = {
    identities: [fallbackIdentity],
    favorites: [defaultFavorite],
  };
  try {
    localStorage.setItem(LOCAL_CONFIG_STORAGE_KEY, JSON.stringify(initial));
  } catch {
    // Ignore
  }
  return initial;
}

export async function saveDesktopConfig(config: DesktopAppConfig): Promise<void> {
  try {
    localStorage.setItem(LOCAL_CONFIG_STORAGE_KEY, JSON.stringify(config));
  } catch {
    // Ignore
  }
  try {
    await invoke('save_config', { config });
  } catch {
    // Ignore when running outside Tauri
  }
}

export async function generateDesktopIdentity(name: string): Promise<DesktopIdentity> {
  try {
    const generated = await invoke<DesktopIdentity>('generate_identity', { name });
    if (generated && generated.public_key) {
      return generated;
    }
  } catch {
    // Fallback
  }
  return createFallbackIdentity(name);
}

export async function discoverCertHashViaTauri(host: string): Promise<{
  sha256?: string;
  webtransportPort?: number;
}> {
  try {
    const res = await invoke<{
      sha256?: string | null;
      webtransport_port?: number | null;
    }>('discover_server_cert_hash', { host });
    if (res) {
      return {
        sha256: res.sha256 ?? undefined,
        webtransportPort: res.webtransport_port ?? undefined,
      };
    }
  } catch {
    // Ignore if command unavailable
  }
  return {};
}

export async function resolveTsdnsViaTauri(address: string): Promise<string> {
  try {
    const resolved = await invoke<string>('resolve_tsdns_address', { address });
    if (resolved && typeof resolved === 'string') {
      return resolved;
    }
  } catch {
    // Ignore
  }
  return address;
}

export async function fetchDesktopAudioDevices(): Promise<DesktopAudioDevices> {
  try {
    const res = await invoke<DesktopAudioDevices>('get_audio_devices');
    if (res && Array.isArray(res.inputs) && Array.isArray(res.outputs)) {
      return res;
    }
  } catch {
    // Fallback to navigator.mediaDevices
  }

  if (typeof navigator !== 'undefined' && navigator.mediaDevices?.enumerateDevices) {
    try {
      const list = await navigator.mediaDevices.enumerateDevices();
      const inputs = list
        .filter((d) => d.kind === 'audioinput')
        .map((d, i) => d.label || `Mikrofon #${i + 1}`);
      const outputs = list
        .filter((d) => d.kind === 'audiooutput')
        .map((d, i) => d.label || `Lautsprecher #${i + 1}`);
      return {
        inputs: inputs.length > 0 ? inputs : ['System-Standard Mikrofon'],
        outputs: outputs.length > 0 ? outputs : ['System-Standard Lautsprecher'],
      };
    } catch {
      // Ignore
    }
  }

  return {
    inputs: ['System-Standard Mikrofon'],
    outputs: ['System-Standard Lautsprecher'],
  };
}

export async function fetchPluginSdkStatus(): Promise<DesktopPluginSdkStatus> {
  try {
    const status = await invoke<DesktopPluginSdkStatus>('get_plugin_sdk_status');
    if (status && typeof status.listen_addr === 'string') {
      return status;
    }
  } catch {
    // Ignore
  }
  return {
    enabled: true,
    running: isTauriRuntime(),
    listen_addr: '127.0.0.1:28910',
    connected_plugins: 0,
    last_error: null,
  };
}

export async function setPluginSdkEnabledBridge(
  enabled: boolean
): Promise<DesktopPluginSdkStatus> {
  try {
    const status = await invoke<DesktopPluginSdkStatus>('set_plugin_sdk_enabled', {
      enabled,
    });
    if (status && typeof status.listen_addr === 'string') {
      return status;
    }
  } catch {
    // Ignore
  }
  return {
    enabled,
    running: enabled,
    listen_addr: '127.0.0.1:28910',
    connected_plugins: 0,
    last_error: null,
  };
}

export function syncSpatialPositionToTauri(
  clientId: number,
  pos: SpatialPosition,
  isSelf: boolean
): void {
  if (!isTauriRuntime()) return;
  if (isSelf) {
    invoke('set_listener_3d_pose', {
      x: pos.x,
      y: pos.y,
      z: pos.z,
      forwardX: 0.0,
      forwardY: 0.0,
      forwardZ: 1.0,
    }).catch(() => {});
  } else {
    invoke('set_client_3d_position', {
      clientId,
      x: pos.x,
      y: pos.y,
      z: pos.z,
    }).catch(() => {});
  }
}

export function syncSpatialConfigToTauri(enabled: boolean): void {
  if (!isTauriRuntime()) return;
  invoke('set_spatial_audio_config', {
    enabled,
    minDistance: 1.5,
    maxDistance: 35.0,
    rolloffFactor: 1.0,
  }).catch(() => {});
}
