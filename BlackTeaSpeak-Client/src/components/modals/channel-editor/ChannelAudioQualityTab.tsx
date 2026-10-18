import React from 'react';
import { AUDIO_CODECS, CodecSpec } from './codecSpecs';

export interface ChannelAudioQualityTabProps {
  codec: number;
  setCodec: (v: number) => void;
  codecQuality: number;
  setCodecQuality: (v: number) => void;
  selectedCodecSpec: CodecSpec;
  bitrateInfo: { kbps: number; kibPerSec: number };
  applyAudioPreset: (presetCodec: number, presetQuality: number) => void;
}

export const ChannelAudioQualityTab: React.FC<ChannelAudioQualityTabProps> = ({
  codec,
  setCodec,
  codecQuality,
  setCodecQuality,
  selectedCodecSpec,
  bitrateInfo,
  applyAudioPreset,
}) => {
  return (
    <>
      <div className="form-field">
        <label>Schnell-Profile (Presets)</label>
        <div style={{ display: 'flex', gap: '8px', flexWrap: 'wrap' }}>
          <button
            type="button"
            className={
              codec === 4 && codecQuality === 6 ? 'btn-primary-sm' : 'btn-secondary-sm'
            }
            onClick={() => applyAudioPreset(4, 6)}
          >
            🎙️ Standard Sprache (Opus Voice Q6)
          </button>
          <button
            type="button"
            className={
              codec === 4 && codecQuality === 8 ? 'btn-primary-sm' : 'btn-secondary-sm'
            }
            onClick={() => applyAudioPreset(4, 8)}
          >
            🎧 HD Sprache (Opus Voice Q8)
          </button>
          <button
            type="button"
            className={
              codec === 4 && codecQuality === 10 ? 'btn-primary-sm' : 'btn-secondary-sm'
            }
            onClick={() => applyAudioPreset(4, 10)}
          >
            💎 Ultra Voice (Opus Voice Q10)
          </button>
          <button
            type="button"
            className={
              codec === 5 && codecQuality === 10 ? 'btn-primary-sm' : 'btn-secondary-sm'
            }
            onClick={() => applyAudioPreset(5, 10)}
          >
            🎵 Studio Musik / Bot (Opus Music Stereo Q10)
          </button>
          <button
            type="button"
            className={
              codec === 1 && codecQuality === 5 ? 'btn-primary-sm' : 'btn-secondary-sm'
            }
            onClick={() => applyAudioPreset(1, 5)}
          >
            📡 Datensparmodus (Speex WB Q5)
          </button>
        </div>
      </div>

      <div className="form-field">
        <label>Audio-Codec (`channel_codec`)</label>
        <select value={codec} onChange={(e) => setCodec(Number(e.target.value))}>
          {AUDIO_CODECS.map((c) => (
            <option key={c.id} value={c.id}>
              [{c.id}] {c.name} — {c.sampleRateLabel} ({c.channelsLabel})
            </option>
          ))}
        </select>
        <span style={{ fontSize: '12px', color: 'rgba(255,255,255,0.6)', marginTop: 4 }}>
          {selectedCodecSpec.description}
        </span>
      </div>

      <div className="form-field">
        <div
          style={{
            display: 'flex',
            justifyContent: 'space-between',
            alignItems: 'center',
            marginBottom: '6px',
          }}
        >
          <label style={{ margin: 0 }}>
            Audio-Qualitätsstufe (`channel_codec_quality`):{' '}
            <strong style={{ color: '#38bdf8' }}>{codecQuality} / 10</strong>
          </label>
          <span
            className="badge-tag"
            style={{
              background: 'rgba(56, 189, 248, 0.16)',
              color: '#38bdf8',
              fontWeight: 600,
            }}
          >
            ~ {bitrateInfo.kbps} kbit/s ({bitrateInfo.kibPerSec} KiB/s)
          </span>
        </div>

        <input
          type="range"
          min={0}
          max={10}
          step={1}
          value={codecQuality}
          onChange={(e) => setCodecQuality(Number(e.target.value))}
          style={{ width: '100%', accentColor: '#38bdf8', cursor: 'pointer' }}
        />

        <div
          style={{
            display: 'flex',
            justifyContent: 'space-between',
            fontSize: '11px',
            color: 'rgba(255,255,255,0.5)',
            marginTop: '4px',
          }}
        >
          <span>0 (Min. Bandbreite)</span>
          <span>3 (Niedrig)</span>
          <span>6 (Standard)</span>
          <span>8 (Hoch)</span>
          <span>10 (Max. Studio-Qualität)</span>
        </div>
      </div>

      <div
        style={{
          display: 'grid',
          gridTemplateColumns: 'repeat(4, 1fr)',
          gap: '10px',
          padding: '12px',
          borderRadius: '10px',
          background: 'rgba(255,255,255,0.03)',
          border: '1px solid rgba(255,255,255,0.08)',
        }}
      >
        <div>
          <div style={{ fontSize: '11px', color: 'rgba(255,255,255,0.5)' }}>
            Ausgewählter Codec
          </div>
          <div style={{ fontSize: '13px', fontWeight: 600, marginTop: 2 }}>
            {selectedCodecSpec.shortName}
          </div>
        </div>
        <div>
          <div style={{ fontSize: '11px', color: 'rgba(255,255,255,0.5)' }}>
            Abtastrate / Kanäle
          </div>
          <div style={{ fontSize: '13px', fontWeight: 600, marginTop: 2 }}>
            {selectedCodecSpec.sampleRateLabel} · {selectedCodecSpec.channelsLabel}
          </div>
        </div>
        <div>
          <div style={{ fontSize: '11px', color: 'rgba(255,255,255,0.5)' }}>
            Sprach-/Musik-Bitrate
          </div>
          <div
            style={{
              fontSize: '13px',
              fontWeight: 600,
              color: '#34d399',
              marginTop: 2,
            }}
          >
            {bitrateInfo.kbps} kbit/s
          </div>
        </div>
        <div>
          <div style={{ fontSize: '11px', color: 'rgba(255,255,255,0.5)' }}>
            Netzwerk-Transferrate
          </div>
          <div style={{ fontSize: '13px', fontWeight: 600, marginTop: 2 }}>
            {bitrateInfo.kibPerSec} KiB/s pro Sprecher
          </div>
        </div>
      </div>
    </>
  );
};
