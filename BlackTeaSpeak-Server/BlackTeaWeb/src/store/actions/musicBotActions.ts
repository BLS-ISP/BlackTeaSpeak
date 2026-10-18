import { MusicBotInfo } from '../../types/protocol';
import { StoreContext } from '../state/blackTeaStateTypes';

export async function refreshMusicBotsAction(ctx: StoreContext): Promise<void> {
  await ctx.protocol.sendCommand('musicbotlist').catch(() => {});
}

export async function createMusicBotAction(
  ctx: StoreContext,
  cid: number,
  name: string
): Promise<void> {
  try {
    const resp = await ctx.protocol.sendCommand('musicbotcreate', [
      { cid: String(cid), botname: name, bot_name: name },
    ]);
    const r = resp[0]?.data?.[0];
    if (r) {
      const botId = Number(r.botid ?? r.bot_id ?? 0);
      if (botId > 0) {
        const newBot: MusicBotInfo = {
          bot_id: botId,
          client_id: Number(r.clid ?? r.client_id ?? 0),
          cid: Number(r.cid ?? cid),
          bot_name: String(r.bot_name ?? name),
          player_state: 4,
          song_title: 'Kein Titel geladen',
          song_url: '',
          position_ms: 0,
          duration_ms: 0,
          volume: 100,
          playlist_id: botId,
          queue: [],
          local_files: [],
        };
        const filtered = ctx.getState().musicBots.filter((b) => b.bot_id !== botId);
        ctx.patchState({ musicBots: [...filtered, newBot] });
      }
    }
    await refreshMusicBotsAction(ctx);
    ctx.showToast(`MusicBot '${name}' erstellt`);
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err);
    ctx.showToast(`MusicBot konnte nicht erstellt werden: ${msg}`);
  }
}

export async function deleteMusicBotAction(ctx: StoreContext, botId: number): Promise<void> {
  try {
    await ctx.protocol.sendCommand('musicbotdelete', [
      { botid: String(botId), bot_id: String(botId) },
    ]);
    ctx.patchState({
      musicBots: ctx.getState().musicBots.filter((b) => b.bot_id !== botId),
    });
    ctx.showToast(`MusicBot #${botId} gelöscht`);
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err);
    ctx.showToast(`MusicBot löschen fehlgeschlagen: ${msg}`);
  }
}

export async function controlMusicBotAction(
  ctx: StoreContext,
  botId: number,
  action: string,
  value?: number
): Promise<void> {
  try {
    const state = ctx.getState();
    if (action === 'volume' && value !== undefined) {
      const targetBot = state.musicBots.find((b) => b.bot_id === botId);
      const gain = Math.max(0, Math.min(2, value / 100));
      if (targetBot && targetBot.client_id > 0) {
        ctx.voiceEngine.setPeerVolume(targetBot.client_id, gain);
      }
      ctx.patchState({
        musicBots: state.musicBots.map((b) =>
          b.bot_id === botId ? { ...b, volume: value } : b
        ),
        ...(targetBot && targetBot.client_id > 0
          ? {
              clients: state.clients.map((c) =>
                c.clid === targetBot.client_id ? { ...c, volumeGain: gain } : c
              ),
            }
          : {}),
      });

      const existingTimer = ctx.botVolumeTimers.get(botId);
      if (existingTimer !== undefined) {
        window.clearTimeout(existingTimer);
      }
      const timer = window.setTimeout(() => {
        ctx.botVolumeTimers.delete(botId);
        void ctx.protocol
          .sendCommand('musicbotplayeraction', [
            {
              botid: String(botId),
              bot_id: String(botId),
              action: 'volume',
              value: String(value),
            },
          ])
          .catch(() => {});
      }, 200);
      ctx.botVolumeTimers.set(botId, timer);
      return;
    }

    if (action === 'seek' && value !== undefined) {
      const clamped = Math.max(0, Math.round(value));
      ctx.patchState({
        musicBots: state.musicBots.map((b) =>
          b.bot_id === botId ? { ...b, position_ms: clamped, player_state: 2 } : b
        ),
      });
    } else if (action === 'repeat') {
      ctx.patchState({
        musicBots: state.musicBots.map((b) =>
          b.bot_id === botId
            ? {
                ...b,
                replay_mode:
                  value !== undefined ? value % 3 : ((b.replay_mode ?? 0) + 1) % 3,
              }
            : b
        ),
      });
    } else if (action === 'summon' && value !== undefined) {
      ctx.patchState({
        musicBots: state.musicBots.map((b) =>
          b.bot_id === botId ? { ...b, cid: value } : b
        ),
      });
    }

    await ctx.protocol.sendCommand('musicbotplayeraction', [
      {
        botid: String(botId),
        bot_id: String(botId),
        action,
        value: value !== undefined ? String(Math.round(value)) : undefined,
      },
    ]);
    await refreshMusicBotsAction(ctx);
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err);
    ctx.showToast(`MusicBot-Steuerung fehlgeschlagen: ${msg}`);
  }
}

export async function renameMusicBotAction(
  ctx: StoreContext,
  botId: number,
  newName: string
): Promise<void> {
  const clean = newName.trim();
  if (!clean) return;
  try {
    ctx.patchState({
      musicBots: ctx.getState().musicBots.map((b) =>
        b.bot_id === botId ? { ...b, bot_name: clean } : b
      ),
    });
    await ctx.protocol.sendCommand('musicbotplayeraction', [
      {
        botid: String(botId),
        bot_id: String(botId),
        action: 'rename',
        value: clean,
      },
    ]);
    await refreshMusicBotsAction(ctx);
    ctx.showToast(`MusicBot umbenannt in '${clean}'`);
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err);
    ctx.showToast(`Umbenennen fehlgeschlagen: ${msg}`);
  }
}

export async function uploadLocalMusicFileAction(
  ctx: StoreContext,
  file: File,
  botId?: number,
  playNow = false
): Promise<void> {
  try {
    ctx.showToast(`⬆️ Lade '${file.name}' auf den Server hoch...`);
    const rawHost = ctx.getState().host || '127.0.0.1';
    const cleanHost = rawHost.includes(':') ? rawHost.split(':')[0] : rawHost;
    const uploadUrls = [
      '/api/music/upload',
      `http://${cleanHost}:8081/api/music/upload`,
      `https://${cleanHost}:8080/api/music/upload`,
    ];

    let data: {
      ok?: boolean;
      name?: string;
      url?: string;
      error?: string;
    } | null = null;
    let lastError = 'Keine Verbindung zum Upload-Endpunkt';

    for (const targetUrl of uploadUrls) {
      try {
        const resp = await fetch(targetUrl, {
          method: 'POST',
          headers: {
            'Content-Type': file.type || 'application/octet-stream',
            'X-File-Name': encodeURIComponent(file.name),
          },
          body: file,
        });
        const parsed = (await resp.json()) as {
          ok?: boolean;
          name?: string;
          url?: string;
          error?: string;
        };
        if (resp.ok && parsed.ok && parsed.url) {
          data = parsed;
          break;
        }
        lastError = parsed.error || `HTTP ${resp.status}`;
      } catch (e) {
        lastError = e instanceof Error ? e.message : String(e);
      }
    }

    if (!data || !data.ok || !data.url) {
      throw new Error(lastError);
    }
    await refreshMusicBotsAction(ctx);
    if (botId !== undefined && playNow) {
      await loadMusicBotSongAction(ctx, botId, data.url, true);
    } else {
      ctx.showToast(`✅ '${data.name ?? file.name}' in Server-Musikbibliothek gespeichert`);
    }
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err);
    ctx.showToast(`Upload fehlgeschlagen: ${msg}`);
  }
}

export async function loadMusicBotSongAction(
  ctx: StoreContext,
  botId: number,
  url: string,
  playNow = true
): Promise<void> {
  try {
    if (playNow) {
      const isLocal = url.startsWith('/music/') || url.startsWith('channel://');
      ctx.patchState({
        musicBots: ctx.getState().musicBots.map((b) =>
          b.bot_id === botId
            ? {
                ...b,
                player_state: 1,
                song_title: isLocal
                  ? `⏳ [1/2] Lade lokale Audiodatei (${url.split('/').pop()})...`
                  : '⏳ [1/3] Prüfe Link & lade Metadaten... (ca. 2–4s)',
                song_url: url,
                position_ms: 0,
                duration_ms: 0,
              }
            : b
        ),
      });
    }
    await ctx.protocol.sendCommand('musicbotqueueadd', [
      {
        botid: String(botId),
        bot_id: String(botId),
        url,
        type: 'any',
        play_now: playNow ? '1' : '0',
      },
    ]);
    await refreshMusicBotsAction(ctx);
    ctx.showToast(
      playNow
        ? '⏳ Starte Audio-Stream...'
        : '➕ Track/Stream zur Warteschlange hinzugefügt'
    );
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err);
    ctx.showToast(`Stream/Track laden fehlgeschlagen: ${msg}`);
  }
}
