import React, { useState } from 'react';

interface WhisperSettingsProps {
  whisperHotkey: string;
  setWhisperHotkey: (val: string) => void;
  whisperClientIds: string;
  setWhisperClientIds: (val: string) => void;
  whisperChannelIds: string;
  setWhisperChannelIds: (val: string) => void;
}

export function WhisperSettings({
  whisperHotkey, setWhisperHotkey,
  whisperClientIds, setWhisperClientIds,
  whisperChannelIds, setWhisperChannelIds
}: WhisperSettingsProps) {
  const [isRecordingWhisperHotkey, setIsRecordingWhisperHotkey] = useState(false);

  const handleWhisperHotkeyRecord = (e: React.KeyboardEvent) => {
    e.preventDefault();
    e.stopPropagation();
    if (['Control', 'Shift', 'Alt', 'Meta'].includes(e.key)) return;
    let keys = [];
    if (e.ctrlKey) keys.push('Control');
    if (e.altKey) keys.push('Alt');
    if (e.shiftKey) keys.push('Shift');
    if (e.metaKey) keys.push('Super');
    let mainKey = e.key;
    if (mainKey === ' ') mainKey = 'Space';
    if (mainKey.length === 1) mainKey = mainKey.toUpperCase();
    keys.push(mainKey);
    setWhisperHotkey(keys.join('+'));
    setIsRecordingWhisperHotkey(false);
  };

  return (
    <div className="settings-group">
      <label>Whisper Setup</label>
      <label>Whisper Hotkey</label>
      <button 
        className="btn-secondary" 
        onClick={() => setIsRecordingWhisperHotkey(true)}
        onKeyDown={isRecordingWhisperHotkey ? handleWhisperHotkeyRecord : undefined}
        style={{ textAlign: 'left', background: isRecordingWhisperHotkey ? 'var(--accent-color)' : '' }}
      >
        {isRecordingWhisperHotkey ? "Press any key combination..." : (whisperHotkey || "Click to bind a hotkey")}
      </button>
      
      <label>Target Client IDs (comma separated)</label>
      <input 
        type="text" 
        placeholder="e.g. 1, 5, 12" 
        value={whisperClientIds} 
        onChange={e => setWhisperClientIds(e.target.value)} 
      />
      
      <label>Target Channel IDs (comma separated)</label>
      <input 
        type="text" 
        placeholder="e.g. 2, 8" 
        value={whisperChannelIds} 
        onChange={e => setWhisperChannelIds(e.target.value)} 
      />
    </div>
  );
}
