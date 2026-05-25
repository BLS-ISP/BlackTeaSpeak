import React from 'react';

interface AudioDevices {
  inputs: string[];
  outputs: string[];
}

interface AudioDeviceSettingsProps {
  devices: AudioDevices;
  inputDevice: string;
  setInputDevice: (val: string) => void;
  inputAmp: number;
  setInputAmp: (val: number) => void;
  inputLevel: number;
  
  outputDevice: string;
  setOutputDevice: (val: string) => void;
  outputAmp: number;
  setOutputAmp: (val: number) => void;
  outputLevel: number;

  mode: string;
  vadThreshold: number;
}

export function AudioDeviceSettings({
  devices,
  inputDevice, setInputDevice,
  inputAmp, setInputAmp,
  inputLevel,
  outputDevice, setOutputDevice,
  outputAmp, setOutputAmp,
  outputLevel,
  mode,
  vadThreshold
}: AudioDeviceSettingsProps) {
  return (
    <>
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
    </>
  );
}
