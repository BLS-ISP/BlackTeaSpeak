export interface CodecSpec {
  id: number;
  name: string;
  shortName: string;
  sampleRateLabel: string;
  channelsLabel: string;
  description: string;
  recommendedQuality: number;
  baseKbps: number;
  stepKbps: number;
}

export const AUDIO_CODECS: CodecSpec[] = [
  {
    id: 0,
    name: 'Speex Schmalband (Narrowband)',
    shortName: 'Speex NB (8 kHz)',
    sampleRateLabel: '8 kHz',
    channelsLabel: 'Mono',
    description: 'Minimale Bandbreite für sehr langsame Verbindungen (Legacy TS3).',
    recommendedQuality: 5,
    baseKbps: 6.0,
    stepKbps: 2.0,
  },
  {
    id: 1,
    name: 'Speex Breitband (Wideband)',
    shortName: 'Speex WB (16 kHz)',
    sampleRateLabel: '16 kHz',
    channelsLabel: 'Mono',
    description: 'Solide Sprachverständlichkeit bei geringem Datenverbrauch.',
    recommendedQuality: 6,
    baseKbps: 10.0,
    stepKbps: 3.2,
  },
  {
    id: 2,
    name: 'Speex Ultra-Breitband (Ultra-Wideband)',
    shortName: 'Speex UWB (32 kHz)',
    sampleRateLabel: '32 kHz',
    channelsLabel: 'Mono',
    description: 'Erweiterter Frequenzbereich für natürliche Stimmwiedergabe.',
    recommendedQuality: 7,
    baseKbps: 14.0,
    stepKbps: 4.2,
  },
  {
    id: 3,
    name: 'CELT Mono (Low-Delay)',
    shortName: 'CELT Mono (48 kHz)',
    sampleRateLabel: '48 kHz',
    channelsLabel: 'Mono',
    description: 'Extrem niedrige Latenz bei 48 kHz Abtastrate.',
    recommendedQuality: 7,
    baseKbps: 22.0,
    stepKbps: 7.5,
  },
  {
    id: 4,
    name: 'Opus Voice (Standard Sprach-Codec)',
    shortName: 'Opus Voice (48 kHz)',
    sampleRateLabel: '48 kHz',
    channelsLabel: 'Mono / Vollband',
    description:
      'Empfohlener Standard-Codec für Sprach-Channels. Kristallklare Stimme mit FEC & DTX.',
    recommendedQuality: 6,
    baseKbps: 14.4,
    stepKbps: 6.2,
  },
  {
    id: 5,
    name: 'Opus Music (Hi-Fi Stereo & MusicBots)',
    shortName: 'Opus Music (48 kHz Stereo)',
    sampleRateLabel: '48 kHz',
    channelsLabel: 'True Stereo (2.0)',
    description:
      'Optimiert für Musik-Übertragung, Radio, Bühnen-Channels und Studio-Audioqualität.',
    recommendedQuality: 10,
    baseKbps: 28.0,
    stepKbps: 11.8,
  },
];

export function estimateCodecBitrate(codecId: number, quality: number): {
  kbps: number;
  kibPerSec: number;
} {
  const spec = AUDIO_CODECS.find((c) => c.id === codecId) ?? AUDIO_CODECS[4];
  const q = Math.max(0, Math.min(10, quality));
  const kbps = Number((spec.baseKbps + q * spec.stepKbps).toFixed(1));
  const kibPerSec = Number(((kbps + 11.2) / 8).toFixed(2));
  return { kbps, kibPerSec };
}
