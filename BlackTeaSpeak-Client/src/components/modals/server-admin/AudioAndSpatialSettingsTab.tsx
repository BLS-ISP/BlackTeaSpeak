import React from 'react';
import { KeyRound } from 'lucide-react';
import {
  blackTeaStore,
  BlackTeaState,
  ClientCapabilities,
} from '../../../store/useBlackTeaStore';

export interface AudioAndSpatialSettingsTabProps {
  state: BlackTeaState;
  caps: ClientCapabilities;
}

export const AudioAndSpatialSettingsTab: React.FC<
  AudioAndSpatialSettingsTabProps
> = ({ state, caps }) => {
  return (
    <div className="stack-form">
      <div className="form-field">
        <label>Sprachübertragungs-Modus (Opus 48kHz Stereo)</label>
        <div className="form-row-2">
          <button
            type="button"
            className={`btn-secondary-sm ${
              state.transmissionMode === 'vad' ? 'active-choice' : ''
            }`}
            onClick={() => blackTeaStore.setTransmissionMode('vad')}
          >
            Sprachaktivierung (VAD)
          </button>
          <button
            type="button"
            className={`btn-secondary-sm ${
              state.transmissionMode === 'ptt' ? 'active-choice' : ''
            }`}
            onClick={() => blackTeaStore.setTransmissionMode('ptt')}
          >
            Push-To-Talk (Leertaste)
          </button>
        </div>
      </div>

      {state.transmissionMode === 'vad' && (
        <div className="form-field">
          <label>
            VAD Auslöse-Schwellenwert: <strong>{state.vadThresholdDb} dBFS</strong>{' '}
            (Aktuell: {state.telemetry.inputLevelDb} dBFS)
          </label>
          <input
            type="range"
            min={-75}
            max={-15}
            value={state.vadThresholdDb}
            onChange={(e) =>
              blackTeaStore.setTransmissionMode('vad', Number(e.target.value))
            }
          />
        </div>
      )}

      <div className="form-row-2">
        <div className="form-field">
          <label>Raumklang (WebAudio 3D HRTF)</label>
          <button
            type="button"
            className="btn-primary-sm"
            onClick={() => blackTeaStore.toggleSpatial3D()}
          >
            {state.telemetry.spatial3DEnabled
              ? '3D HRTF Raumklang ist AKTIV'
              : '3D HRTF Raumklang einschalten'}
          </button>
        </div>

        <div className="form-field">
          <label>TeamSpeak 3 Sprachansagen (Sound Pack)</label>
          <button
            type="button"
            className="btn-secondary-sm"
            onClick={() => blackTeaStore.cycleSoundPack()}
          >
            Aktuell:{' '}
            {state.soundPack === 'default'
              ? 'TS3 Female (Standard)'
              : state.soundPack === 'default_male'
              ? 'TS3 Male'
              : 'Stummgeschaltet'}
          </button>
        </div>
      </div>

      {!caps.isAdmin && caps.isConnected && (
        <div className="privilege-hint-row">
          <span>Du hast einen Server-Admin Berechtigungsschlüssel?</span>
          <button
            type="button"
            className="btn-secondary-xs"
            onClick={() => blackTeaStore.patchState({ activeModal: 'redeem_key' })}
          >
            <KeyRound size={12} /> Schlüssel einlösen
          </button>
        </div>
      )}
    </div>
  );
};
