import React, { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { Identity, AppConfig } from './App';

interface AudioDevices {
  inputs: string[];
  outputs: string[];
}

interface SettingsModalProps {
  onClose: () => void;
  identity: Identity;
  onIdentityUpdated: (identity: Identity) => void;
}

export function SettingsModal({ onClose, identity, onIdentityUpdated }: SettingsModalProps) {
  const [devices, setDevices] = useState<AudioDevices>({ inputs: [], outputs: [] });
  
  const [inputDevice, setInputDevice] = useState(identity.audio_input_device || '');
  const [outputDevice, setOutputDevice] = useState(identity.audio_output_device || '');
  const [inputAmp, setInputAmp] = useState(identity.input_amplification ?? 1.0);
  const [outputAmp, setOutputAmp] = useState(identity.output_amplification ?? 1.0);
  const [mode, setMode] = useState(identity.voice_transmission_mode || 'voice_activation');
  const [vadThreshold, setVadThreshold] = useState(identity.voice_activation_threshold ?? 0.05);
  const [pttHotkey, setPttHotkey] = useState(identity.ptt_hotkey || '');
  const [isRecordingHotkey, setIsRecordingHotkey] = useState(false);
  
  const [inputLevel, setInputLevel] = useState(0);
  const [outputLevel, setOutputLevel] = useState(0);

  useEffect(() => {
    let unlistenIn: () => void;
    let unlistenOut: () => void;
    
    invoke<AudioDevices>('get_audio_devices').then(setDevices).catch(console.error);

    listen<number>('audio_levels_input', (e) => setInputLevel(e.payload)).then(f => unlistenIn = f);
    listen<number>('audio_levels_output', (e) => setOutputLevel(e.payload)).then(f => unlistenOut = f);

    return () => {
      if (unlistenIn) unlistenIn();
      if (unlistenOut) unlistenOut();
    };
  }, []);

  useEffect(() => {
    // Send live updates to backend without saving or restarting streams
    invoke('update_live_audio_settings', {
      settings: {
        input_amplification: inputAmp,
        output_amplification: outputAmp,
        transmission_mode: mode,
        vad_threshold: vadThreshold,
      }
    }).catch(console.error);
  }, [inputAmp, outputAmp, mode, vadThreshold]);

  const handleSave = async () => {
    const newIdentity: Identity = {
      ...identity,
      audio_input_device: inputDevice || undefined,
      audio_output_device: outputDevice || undefined,
      input_amplification: inputAmp,
      output_amplification: outputAmp,
      voice_transmission_mode: mode,
      voice_activation_threshold: vadThreshold,
      ptt_hotkey: pttHotkey || undefined,
    };

    try {
      // 1. Update the backend active audio settings
      await invoke('update_audio_settings', {
        settings: {
          input_device: inputDevice || null,
          output_device: outputDevice || null,
          input_amplification: inputAmp,
          output_amplification: outputAmp,
          transmission_mode: mode,
          vad_threshold: vadThreshold,
          ptt_hotkey: pttHotkey || null,
        }
      });

      // 2. Save to config file
      const config: AppConfig = await invoke('load_config');
      const idIdx = config.identities.findIndex(i => i.id === identity.id);
      if (idIdx !== -1) {
        config.identities[idIdx] = newIdentity;
        await invoke('save_config', { config });
      }

      onIdentityUpdated(newIdentity);
      onClose();
    } catch (e) {
      console.error("Failed to save settings:", e);
      alert("Failed to save settings. See console.");
    }
  };

  const handleHotkeyRecord = (e: React.KeyboardEvent) => {
    e.preventDefault();
    e.stopPropagation();
    
    // Ignore standalone modifiers
    if (['Control', 'Shift', 'Alt', 'Meta'].includes(e.key)) return;

    let keys = [];
    if (e.ctrlKey) keys.push('Control');
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
    <div className="settings-modal-overlay">
      <div className="settings-modal">
        <h2>Audio Settings</h2>
        
        <div className="settings-group">
          <label>Microphone</label>
          <select value={inputDevice} onChange={e => setInputDevice(e.target.value)}>
            <option value="">Default System Device</option>
            {devices.inputs.map(d => <option key={d} value={d}>{d}</option>)}
          </select>
          
          <label>Microphone Amplification ({inputAmp.toFixed(2)}x)</label>
          <input 
            type="range" min="0.1" max="5.0" step="0.1" 
            value={inputAmp} onChange={e => setInputAmp(parseFloat(e.target.value))} 
          />
          <div className="vu-meter">
            <div className="vu-meter-fill" style={{ width: `${Math.min(inputLevel * 200, 100)}%` }}></div>
            {mode === 'voice_activation' && (
              <div className="vu-meter-marker" style={{ left: `${Math.min(vadThreshold * 200, 100)}%` }} title="Voice Activation Threshold"></div>
            )}
          </div>
        </div>

        <div className="settings-group">
          <label>Speaker</label>
          <select value={outputDevice} onChange={e => setOutputDevice(e.target.value)}>
            <option value="">Default System Device</option>
            {devices.outputs.map(d => <option key={d} value={d}>{d}</option>)}
          </select>
          
          <label>Speaker Amplification ({outputAmp.toFixed(2)}x)</label>
          <input 
            type="range" min="0.1" max="5.0" step="0.1" 
            value={outputAmp} onChange={e => setOutputAmp(parseFloat(e.target.value))} 
          />
          <div className="vu-meter">
            <div className="vu-meter-fill" style={{ width: `${Math.min(outputLevel * 200, 100)}%` }}></div>
          </div>
        </div>

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

        <div className="settings-actions">
          <button className="btn-secondary" onClick={onClose}>Cancel</button>
          <button className="btn-primary" onClick={handleSave}>Save</button>
        </div>
      </div>
    </div>
  );
}
