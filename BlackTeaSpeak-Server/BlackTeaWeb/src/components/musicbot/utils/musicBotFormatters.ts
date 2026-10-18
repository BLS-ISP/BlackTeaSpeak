import { MusicBotInfo } from '../../../types/protocol';

export function formatMs(ms: number): string {
  const totalSecs = Math.max(0, Math.floor(ms / 1000));
  const hours = Math.floor(totalSecs / 3600);
  const mins = Math.floor((totalSecs % 3600) / 60);
  const secs = totalSecs % 60;
  if (hours > 0) {
    return `${hours}:${String(mins).padStart(2, '0')}:${String(secs).padStart(2, '0')}`;
  }
  return `${String(mins).padStart(2, '0')}:${String(secs).padStart(2, '0')}`;
}

export function formatBytes(bytes: number): string {
  if (bytes >= 1024 * 1024) {
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  }
  return `${Math.max(1, Math.round(bytes / 1024))} KB`;
}

export function extractYouTubeId(url: string): string | null {
  if (!url) return null;
  try {
    const shortMatch = url.match(/youtu\.be\/([a-zA-Z0-9_-]{11})/);
    if (shortMatch) return shortMatch[1];
    const vMatch = url.match(/[?&]v=([a-zA-Z0-9_-]{11})/);
    if (vMatch) return vMatch[1];
    const embedMatch = url.match(/youtube\.com\/(?:embed|shorts)\/([a-zA-Z0-9_-]{11})/);
    if (embedMatch) return embedMatch[1];
  } catch {
    // ignore
  }
  return null;
}

export function resolveBotArtwork(bot: MusicBotInfo): string | null {
  if (
    bot.song_thumbnail &&
    bot.song_thumbnail !== 'none' &&
    bot.song_thumbnail.startsWith('http')
  ) {
    return bot.song_thumbnail;
  }
  const ytId = extractYouTubeId(bot.song_url);
  if (ytId) {
    return `https://i.ytimg.com/vi/${ytId}/hqdefault.jpg`;
  }
  const currentInQueue = bot.queue?.find((q) => q.is_current);
  if (currentInQueue) {
    if (currentInQueue.thumbnail && currentInQueue.thumbnail.startsWith('http')) {
      return currentInQueue.thumbnail;
    }
    const qYtId = extractYouTubeId(currentInQueue.url);
    if (qYtId) {
      return `https://i.ytimg.com/vi/${qYtId}/hqdefault.jpg`;
    }
  }
  return null;
}

export function getSourceLabel(url: string): string {
  if (!url) return 'Bereit';
  if (url.includes('youtube.com/') || url.includes('youtu.be/')) return 'YouTube';
  if (url.startsWith('/music/') || url.startsWith('local://')) return 'Server-Audio';
  if (url.startsWith('channel://')) return 'Channel-Datei';
  if (url.startsWith('http://') || url.startsWith('https://')) return 'Web-Stream';
  return 'Track';
}
