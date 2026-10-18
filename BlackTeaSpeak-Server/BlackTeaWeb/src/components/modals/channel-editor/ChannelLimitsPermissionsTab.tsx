import React from 'react';
import { FolderTree, Radio, Sliders, Users } from 'lucide-react';

export interface ChannelLimitsPermissionsTabProps {
  maxClientsUnlimited: boolean;
  setMaxClientsUnlimited: (v: boolean) => void;
  maxClientsValue: number;
  setMaxClientsValue: (v: number) => void;
  maxFamilyUnlimited: boolean;
  setMaxFamilyUnlimited: (v: boolean) => void;
  maxFamilyValue: number;
  setMaxFamilyValue: (v: number) => void;
  neededTalkPower: number;
  setNeededTalkPower: (v: number) => void;
  neededJoinPower: number;
  setNeededJoinPower: (v: number) => void;
  neededSubscribePower: number;
  setNeededSubscribePower: (v: number) => void;
  neededDescViewPower: number;
  setNeededDescViewPower: (v: number) => void;
  neededModifyPower: number;
  setNeededModifyPower: (v: number) => void;
  neededDeletePower: number;
  setNeededDeletePower: (v: number) => void;
}

export const ChannelLimitsPermissionsTab: React.FC<ChannelLimitsPermissionsTabProps> = ({
  maxClientsUnlimited,
  setMaxClientsUnlimited,
  maxClientsValue,
  setMaxClientsValue,
  maxFamilyUnlimited,
  setMaxFamilyUnlimited,
  maxFamilyValue,
  setMaxFamilyValue,
  neededTalkPower,
  setNeededTalkPower,
  neededJoinPower,
  setNeededJoinPower,
  neededSubscribePower,
  setNeededSubscribePower,
  neededDescViewPower,
  setNeededDescViewPower,
  neededModifyPower,
  setNeededModifyPower,
  neededDeletePower,
  setNeededDeletePower,
}) => {
  return (
    <>
      <div className="form-row-2">
        <div
          style={{
            padding: '12px',
            borderRadius: '8px',
            background: 'rgba(255,255,255,0.03)',
            border: '1px solid rgba(255,255,255,0.08)',
          }}
        >
          <div
            style={{
              display: 'flex',
              justifyContent: 'space-between',
              alignItems: 'center',
              marginBottom: '8px',
            }}
          >
            <label style={{ fontWeight: 600, fontSize: '13px', margin: 0 }}>
              <Users size={14} style={{ verticalAlign: 'middle', marginRight: 5 }} />
              Max. Clients im Channel
            </label>
            <label
              style={{
                display: 'inline-flex',
                alignItems: 'center',
                gap: '6px',
                fontSize: '12px',
                cursor: 'pointer',
              }}
            >
              <input
                type="checkbox"
                checked={maxClientsUnlimited}
                onChange={(e) => setMaxClientsUnlimited(e.target.checked)}
              />
              Unbegrenzt
            </label>
          </div>
          <input
            type="number"
            min={0}
            max={1024}
            disabled={maxClientsUnlimited}
            value={maxClientsUnlimited ? '' : maxClientsValue}
            placeholder="Unbegrenzt (-1)"
            onChange={(e) => setMaxClientsValue(Math.max(0, Number(e.target.value)))}
          />
        </div>

        <div
          style={{
            padding: '12px',
            borderRadius: '8px',
            background: 'rgba(255,255,255,0.03)',
            border: '1px solid rgba(255,255,255,0.08)',
          }}
        >
          <div
            style={{
              display: 'flex',
              justifyContent: 'space-between',
              alignItems: 'center',
              marginBottom: '8px',
            }}
          >
            <label style={{ fontWeight: 600, fontSize: '13px', margin: 0 }}>
              <FolderTree size={14} style={{ verticalAlign: 'middle', marginRight: 5 }} />
              Max. Family-Clients (inkl. Sub-Channels)
            </label>
            <label
              style={{
                display: 'inline-flex',
                alignItems: 'center',
                gap: '6px',
                fontSize: '12px',
                cursor: 'pointer',
              }}
            >
              <input
                type="checkbox"
                checked={maxFamilyUnlimited}
                onChange={(e) => setMaxFamilyUnlimited(e.target.checked)}
              />
              Unbegrenzt
            </label>
          </div>
          <input
            type="number"
            min={0}
            max={1024}
            disabled={maxFamilyUnlimited}
            value={maxFamilyUnlimited ? '' : maxFamilyValue}
            placeholder="Unbegrenzt (-1)"
            onChange={(e) => setMaxFamilyValue(Math.max(0, Number(e.target.value)))}
          />
        </div>
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
            <Radio size={14} style={{ verticalAlign: 'middle', marginRight: 5 }} />
            Benötigte Talk-Power (`channel_needed_talk_power` — Moderierter Channel)
          </label>
          <div style={{ display: 'flex', gap: '4px' }}>
            <button
              type="button"
              className="btn-secondary-sm"
              onClick={() => setNeededTalkPower(0)}
            >
              0 (Offen)
            </button>
            <button
              type="button"
              className="btn-secondary-sm"
              onClick={() => setNeededTalkPower(25)}
            >
              25 (Voice)
            </button>
            <button
              type="button"
              className="btn-secondary-sm"
              onClick={() => setNeededTalkPower(50)}
            >
              50 (Operator)
            </button>
            <button
              type="button"
              className="btn-secondary-sm"
              onClick={() => setNeededTalkPower(75)}
            >
              75 (Admin)
            </button>
          </div>
        </div>
        <input
          type="number"
          value={neededTalkPower}
          onChange={(e) => setNeededTalkPower(Number(e.target.value))}
        />
      </div>

      <div className="form-field">
        <label>
          <Sliders size={14} style={{ verticalAlign: 'middle', marginRight: 5 }} />
          Channel-Berechtigungsschwellen (Permission Powers)
        </label>
        <div
          style={{
            display: 'grid',
            gridTemplateColumns: 'repeat(3, 1fr)',
            gap: '10px',
          }}
        >
          <div className="form-field">
            <label style={{ fontSize: '11px' }}>Benötigte Join-Power</label>
            <input
              type="number"
              value={neededJoinPower}
              onChange={(e) => setNeededJoinPower(Number(e.target.value))}
            />
          </div>
          <div className="form-field">
            <label style={{ fontSize: '11px' }}>Benötigte Subscribe-Power</label>
            <input
              type="number"
              value={neededSubscribePower}
              onChange={(e) => setNeededSubscribePower(Number(e.target.value))}
            />
          </div>
          <div className="form-field">
            <label style={{ fontSize: '11px' }}>Beschreibung-Lesen Power</label>
            <input
              type="number"
              value={neededDescViewPower}
              onChange={(e) => setNeededDescViewPower(Number(e.target.value))}
            />
          </div>
          <div className="form-field">
            <label style={{ fontSize: '11px' }}>Benötigte Modify-Power</label>
            <input
              type="number"
              value={neededModifyPower}
              onChange={(e) => setNeededModifyPower(Number(e.target.value))}
            />
          </div>
          <div className="form-field">
            <label style={{ fontSize: '11px' }}>Benötigte Delete-Power</label>
            <input
              type="number"
              value={neededDeletePower}
              onChange={(e) => setNeededDeletePower(Number(e.target.value))}
            />
          </div>
        </div>
      </div>
    </>
  );
};
