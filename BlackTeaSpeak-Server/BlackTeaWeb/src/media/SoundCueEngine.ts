export type SoundPackId = 'default' | 'default_male' | 'none';

export type SoundCueType =
  | 'mic_muted'
  | 'mic_unmuted'
  | 'sound_muted'
  | 'sound_unmuted'
  | 'away_activated'
  | 'away_deactivated'
  | 'connected'
  | 'disconnected'
  | 'connection_lost'
  | 'channel_switched'
  | 'you_were_moved'
  | 'user_connected_current_channel'
  | 'user_disconnected_current_channel'
  | 'user_joined_channel'
  | 'user_left_channel'
  | 'user_moved_to_channel'
  | 'user_moved_from_channel'
  | 'poke_received'
  | 'you_kicked_server'
  | 'you_kicked_channel'
  | 'you_were_banned'
  | 'channel_created'
  | 'channel_deleted'
  | 'channel_edited'
  | 'your_channel_was_edited'
  | 'servergroup_assigned'
  | 'servergroup_revoked'
  | 'channelgroup_changed'
  | 'insufficient_permissions'
  | 'chat_message'
  | 'chat_message_outbound'
  | 'ptt_down'
  | 'ptt_up'
  | 'whisper_on'
  | 'whisper_off'
  | 'error';

const CUE_TO_WAV_FILE: Record<SoundCueType, string> = {
  mic_muted: 'mic_muted.wav',
  mic_unmuted: 'mic_activated.wav',
  sound_muted: 'sound_muted.wav',
  sound_unmuted: 'sound_resumed.wav',
  away_activated: 'away_activated.wav',
  away_deactivated: 'away_deactivated.wav',
  connected: 'connected.wav',
  disconnected: 'disconnected.wav',
  connection_lost: 'connection_lost.wav',
  channel_switched: 'channel_switched.wav',
  you_were_moved: 'you_were_moved.wav',
  user_connected_current_channel: 'neutral_connection_connected_currentchannel.wav',
  user_disconnected_current_channel: 'neutral_connection_disconnected_currentchannel.wav',
  user_joined_channel: 'neutral_switched_tocurrentchannel.wav',
  user_left_channel: 'neutral_switched_awayfromcurrentchannel.wav',
  user_moved_to_channel: 'neutral_moved_tocurrentchannel.wav',
  user_moved_from_channel: 'neutral_moved_awayfromcurrentchannel.wav',
  poke_received: 'you_were_poked.wav',
  you_kicked_server: 'you_kicked_server.wav',
  you_kicked_channel: 'you_kicked_channel.wav',
  you_were_banned: 'you_were_banned.wav',
  channel_created: 'channel_created.wav',
  channel_deleted: 'channel_deleted.wav',
  channel_edited: 'channel_edited.wav',
  your_channel_was_edited: 'your_channel_was_edited.wav',
  servergroup_assigned: 'servergroup_assigned.wav',
  servergroup_revoked: 'servergroup_revoked.wav',
  channelgroup_changed: 'channelgroup_changed.wav',
  insufficient_permissions: 'insufficient_permissions.wav',
  chat_message: 'chat_message_inbound.wav',
  chat_message_outbound: 'chat_message_outbound.wav',
  ptt_down: 'mic_click_on.wav',
  ptt_up: 'mic_click_off.wav',
  whisper_on: 'whisper_notify.wav',
  whisper_off: 'mic_click_off.wav',
  error: 'error.wav',
};

const IMMEDIATE_SFX_CUES = new Set<SoundCueType>([
  'chat_message',
  'chat_message_outbound',
  'ptt_down',
  'ptt_up',
  'whisper_on',
  'whisper_off',
]);

const SELF_STATE_TOGGLE_CUES = new Set<SoundCueType>([
  'mic_muted',
  'mic_unmuted',
  'sound_muted',
  'sound_unmuted',
]);

const PRELOAD_CUES: SoundCueType[] = [
  'mic_muted',
  'mic_unmuted',
  'sound_muted',
  'sound_unmuted',
  'connected',
  'disconnected',
  'channel_switched',
  'user_joined_channel',
  'user_left_channel',
  'user_connected_current_channel',
  'user_disconnected_current_channel',
  'poke_received',
  'chat_message',
  'ptt_down',
  'ptt_up',
];

/**
 * Full TeamSpeak 3 Voice Line Sound Pack Engine for BlackTeaWeb.
 * Supports authentic TS3 Female ('default') and Male ('default_male')
 * voice packs with low-latency Web Audio API buffer caching and sequential
 * voice line queueing.
 */
export class SoundCueEngine {
  private ctx: AudioContext | null = null;
  private soundPack: SoundPackId =
    (localStorage.getItem('blackteaweb_soundpack') as SoundPackId) || 'default';
  private masterVolume = 0.85;
  private rawBytesCache = new Map<string, Promise<ArrayBuffer | null>>();
  private decodedBufferCache = new Map<string, AudioBuffer>();

  private voiceQueue: SoundCueType[] = [];
  private isPlayingVoice = false;
  private activeSource: AudioBufferSourceNode | null = null;
  private activeHtmlAudio: HTMLAudioElement | null = null;

  constructor() {
    this.preloadPack(this.soundPack);
  }

  public getSoundPack(): SoundPackId {
    return this.soundPack;
  }

  public setSoundPack(pack: SoundPackId): void {
    this.soundPack = pack;
    localStorage.setItem('blackteaweb_soundpack', pack);
    if (pack !== 'none') {
      this.preloadPack(pack);
    }
  }

  public setMasterVolume(volume: number): void {
    this.masterVolume = Math.max(0, Math.min(1, volume));
  }

  private ensureContext(): AudioContext | null {
    try {
      if (!this.ctx || this.ctx.state === 'closed') {
        const AudioCtx =
          window.AudioContext ||
          (window as unknown as { webkitAudioContext: typeof AudioContext }).webkitAudioContext;
        this.ctx = new AudioCtx();
      }
      if (this.ctx.state === 'suspended') {
        this.ctx.resume().catch(() => {});
      }
      return this.ctx;
    } catch {
      return null;
    }
  }

  private getWavUrl(pack: SoundPackId, cue: SoundCueType): string {
    const file = CUE_TO_WAV_FILE[cue];
    return `/sounds/${pack === 'none' ? 'default' : pack}/${file}`;
  }

  private fetchRawWav(url: string): Promise<ArrayBuffer | null> {
    const existing = this.rawBytesCache.get(url);
    if (existing) return existing;

    const promise = fetch(url)
      .then((res) => {
        if (!res.ok) return null;
        return res.arrayBuffer();
      })
      .catch(() => null);

    this.rawBytesCache.set(url, promise);
    return promise;
  }

  private preloadPack(pack: SoundPackId): void {
    if (pack === 'none') return;
    for (const cue of PRELOAD_CUES) {
      const url = this.getWavUrl(pack, cue);
      this.fetchRawWav(url);
    }
  }

  public play(cue: SoundCueType): void {
    if (this.soundPack === 'none') return;

    if (IMMEDIATE_SFX_CUES.has(cue)) {
      this.playUrlImmediate(this.getWavUrl(this.soundPack, cue));
      return;
    }

    // If the user rapidly toggles mic mute/unmute or sound mute/unmute, interrupt previous self-toggle line
    if (SELF_STATE_TOGGLE_CUES.has(cue)) {
      this.voiceQueue = this.voiceQueue.filter((q) => !SELF_STATE_TOGGLE_CUES.has(q));
      if (this.isPlayingVoice) {
        this.stopActiveVoice();
      }
    }

    if (this.voiceQueue.length < 6) {
      this.voiceQueue.push(cue);
    }
    this.drainVoiceQueue();
  }

  private stopActiveVoice(): void {
    if (this.activeSource) {
      try {
        this.activeSource.onended = null;
        this.activeSource.stop();
      } catch {
        // ignore
      }
      this.activeSource = null;
    }
    if (this.activeHtmlAudio) {
      try {
        this.activeHtmlAudio.onended = null;
        this.activeHtmlAudio.pause();
      } catch {
        // ignore
      }
      this.activeHtmlAudio = null;
    }
    this.isPlayingVoice = false;
  }

  private async drainVoiceQueue(): Promise<void> {
    if (this.isPlayingVoice || this.voiceQueue.length === 0) return;
    if (this.soundPack === 'none') {
      this.voiceQueue.length = 0;
      return;
    }

    const nextCue = this.voiceQueue.shift()!;
    this.isPlayingVoice = true;

    const url = this.getWavUrl(this.soundPack, nextCue);
    await this.playVoiceUrlAndWait(url);

    this.isPlayingVoice = false;
    if (this.voiceQueue.length > 0) {
      this.drainVoiceQueue();
    }
  }

  private async getDecodedBuffer(ctx: AudioContext, url: string): Promise<AudioBuffer | null> {
    const cached = this.decodedBufferCache.get(url);
    if (cached) return cached;

    const raw = await this.fetchRawWav(url);
    if (!raw) return null;

    try {
      const cloned = raw.slice(0);
      const decoded = await ctx.decodeAudioData(cloned);
      this.decodedBufferCache.set(url, decoded);
      return decoded;
    } catch {
      return null;
    }
  }

  private async playUrlImmediate(url: string): Promise<void> {
    const ctx = this.ensureContext();
    if (ctx) {
      const buf = await this.getDecodedBuffer(ctx, url);
      if (buf) {
        const source = ctx.createBufferSource();
        const gain = ctx.createGain();
        source.buffer = buf;
        gain.gain.value = this.masterVolume;
        source.connect(gain);
        gain.connect(ctx.destination);
        source.start(0);
        return;
      }
    }

    try {
      const audio = new Audio(url);
      audio.volume = this.masterVolume;
      audio.play().catch(() => {});
    } catch {
      // ignore
    }
  }

  private playVoiceUrlAndWait(url: string): Promise<void> {
    return new Promise<void>(async (resolve) => {
      const ctx = this.ensureContext();
      if (ctx) {
        const buf = await this.getDecodedBuffer(ctx, url);
        if (buf) {
          const source = ctx.createBufferSource();
          const gain = ctx.createGain();
          source.buffer = buf;
          gain.gain.value = this.masterVolume;
          source.connect(gain);
          gain.connect(ctx.destination);
          this.activeSource = source;
          source.onended = () => {
            if (this.activeSource === source) {
              this.activeSource = null;
            }
            resolve();
          };
          source.start(0);
          return;
        }
      }

      try {
        const audio = new Audio(url);
        audio.volume = this.masterVolume;
        this.activeHtmlAudio = audio;
        const done = () => {
          if (this.activeHtmlAudio === audio) {
            this.activeHtmlAudio = null;
          }
          resolve();
        };
        audio.onended = done;
        audio.onerror = done;
        audio.play().catch(done);
      } catch {
        resolve();
      }
    });
  }
}

export const soundCues = new SoundCueEngine();
