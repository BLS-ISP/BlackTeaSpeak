import React from 'react';

interface AdvancedAudioSettingsProps {
  noiseSuppression: boolean;
  setNoiseSuppression: (val: boolean) => void;
  autoGainControl: boolean;
  setAutoGainControl: (val: boolean) => void;
  echoCancellation: boolean;
  setEchoCancellation: (val: boolean) => void;
}

export function AdvancedAudioSettings({
  noiseSuppression, setNoiseSuppression,
  autoGainControl, setAutoGainControl,
  echoCancellation, setEchoCancellation
}: AdvancedAudioSettingsProps) {
  return (
    <div className="settings-group">
      <label>Advanced Audio Processing</label>
      <div style={{ display: 'flex', alignItems: 'center', gap: '8px', marginBottom: '8px' }}>
        <input type="checkbox" id="ns_toggle" checked={noiseSuppression} onChange={e => setNoiseSuppression(e.target.checked)} />
        <label htmlFor="ns_toggle" style={{ margin: 0, fontWeight: 'normal' }}>Noise Suppression (RNNoise)</label>
      </div>
      <div style={{ display: 'flex', alignItems: 'center', gap: '8px', marginBottom: '8px' }}>
        <input type="checkbox" id="agc_toggle" checked={autoGainControl} onChange={e => setAutoGainControl(e.target.checked)} />
        <label htmlFor="agc_toggle" style={{ margin: 0, fontWeight: 'normal' }}>Automatic Gain Control (Normalize Volume)</label>
      </div>
      <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
        <input type="checkbox" id="aec_toggle" checked={echoCancellation} onChange={e => setEchoCancellation(e.target.checked)} />
        <label htmlFor="aec_toggle" style={{ margin: 0, fontWeight: 'normal' }}>Echo Cancellation (Duck Mic when Speaker is active)</label>
      </div>
    </div>
  );
}
