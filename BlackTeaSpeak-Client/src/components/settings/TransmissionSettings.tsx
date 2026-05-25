import React, { useState } from 'react';

interface TransmissionSettingsProps {
  mode: string;
  setMode: (val: string) => void;
  vadThreshold: number;
  setVadThreshold: (val: number) => void;
  pttHotkey: string;
  setPttHotkey: (val: string) => void;
}

export function TransmissionSettings({
  mode, setMode,
  vadThreshold, setVadThreshold,
  pttHotkey, setPttHotkey
}: TransmissionSettingsProps) {
  const [isRecordingHotkey, setIsRecordingHotkey] = useState(false);

  const handleHotkeyRecord = (e: React.KeyboardEvent) => {
    e.preventDefault();
    e.stopPropagation();
    
    // Ignore standalone modifiers
    if (['Control', 'Shift', 'Alt', 'Meta', 'OS'].includes(e.key)) return;

    let keys = [];
    if (e.ctrlKey) keys.push('CommandOrControl');
    if (e.altKey) keys.push('Alt');
    if (e.shiftKey) keys.push('Shift');
    if (e.metaKey) keys.push('Super');
    
    // The main key
    let mainKey = e.key;
    if (mainKey === ' ') mainKey = 'Space';
    if (mainKey.length === 1) mainKey = mainKey.toUpperCase();
    
    keys.push(mainKey);
    setPttHotkey(keys.join('+'));
    setIsRecordingHotkey(false);
  };

  return (
    <div className="settings-group">
      <label>Transmission Mode</label>
      <select value={mode} onChange={e => setMode(e.target.value)}>
        <option value="voice_activation">Voice Activation</option>
        <option value="push_to_talk">Push-To-Talk</option>
        <option value="continuous">Continuous Transmission</option>
      </select>
      
      {mode === 'voice_activation' && (
        <>
          <label>Voice Activation Threshold ({vadThreshold.toFixed(3)})</label>
          <input 
            type="range" min="0.001" max="0.5" step="0.005" 
            value={vadThreshold} onChange={e => setVadThreshold(parseFloat(e.target.value))} 
          />
        </>
      )}

      {mode === 'push_to_talk' && (
        <>
          <label>Push-To-Talk Hotkey</label>
          <button 
            className="btn-secondary" 
            onClick={() => setIsRecordingHotkey(true)}
            onKeyDown={isRecordingHotkey ? handleHotkeyRecord : undefined}
            style={{ textAlign: 'left', background: isRecordingHotkey ? 'var(--accent-color)' : '' }}
          >
            {isRecordingHotkey ? "Press any key combination..." : (pttHotkey || "Click to bind a hotkey")}
          </button>
        </>
      )}
    </div>
  );
}
