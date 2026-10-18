import React from 'react';
import { Lock, Unlock } from 'lucide-react';
import { ChannelInfo } from '../../../types/protocol';

export interface ChannelGeneralTabProps {
  mode: 'create' | 'edit';
  targetChannel?: ChannelInfo;
  name: string;
  setName: (v: string) => void;
  topic: string;
  setTopic: (v: string) => void;
  description: string;
  setDescription: (v: string) => void;
  password: string;
  setPassword: (v: string) => void;
  clearPassword: boolean;
  setClearPassword: (v: boolean) => void;
  iconId: number;
  setIconId: (v: number) => void;
}

export const ChannelGeneralTab: React.FC<ChannelGeneralTabProps> = ({
  mode,
  targetChannel,
  name,
  setName,
  topic,
  setTopic,
  description,
  setDescription,
  password,
  setPassword,
  clearPassword,
  setClearPassword,
  iconId,
  setIconId,
}) => {
  return (
    <>
      <div className="form-row-2">
        <div className="form-field">
          <label>Channel-Name *</label>
          <input
            type="text"
            value={name}
            onChange={(e) => setName(e.target.value)}
            required
            placeholder="z.B. Gaming Lounge #1"
          />
        </div>

        <div className="form-field">
          <label>Thema (Topic)</label>
          <input
            type="text"
            value={topic}
            onChange={(e) => setTopic(e.target.value)}
            placeholder="Kurzes Channel-Topic in der Info-Leiste..."
          />
        </div>
      </div>

      <div className="form-field">
        <label>Beschreibung (BBCode / Markdown unterstützt)</label>
        <textarea
          rows={4}
          value={description}
          onChange={(e) => setDescription(e.target.value)}
          placeholder="Ausführliche Channel-Beschreibung, Regeln oder Links..."
        />
      </div>

      <div className="form-row-2">
        <div className="form-field">
          <label>
            {mode === 'edit' && targetChannel?.channel_flag_password ? (
              <span style={{ display: 'inline-flex', alignItems: 'center', gap: 5 }}>
                <Lock size={13} color="#fbbf24" /> Channel-Passwort (aktuell geschützt)
              </span>
            ) : (
              <span style={{ display: 'inline-flex', alignItems: 'center', gap: 5 }}>
                <Unlock size={13} /> Channel-Passwort (optional)
              </span>
            )}
          </label>
          <input
            type="password"
            value={password}
            disabled={clearPassword}
            onChange={(e) => setPassword(e.target.value)}
            placeholder={
              mode === 'edit' && targetChannel?.channel_flag_password
                ? 'Leer lassen um aktuelles Passwort zu behalten...'
                : 'Leer lassen für öffentlichen Channel'
            }
          />
          {mode === 'edit' && targetChannel?.channel_flag_password && (
            <label
              style={{
                display: 'inline-flex',
                alignItems: 'center',
                gap: '6px',
                marginTop: '6px',
                fontSize: '12px',
                color: '#fca5a5',
                cursor: 'pointer',
              }}
            >
              <input
                type="checkbox"
                checked={clearPassword}
                onChange={(e) => {
                  setClearPassword(e.target.checked);
                  if (e.target.checked) setPassword('');
                }}
              />
              Bestehendes Channel-Passwort entfernen (Channel öffentlich machen)
            </label>
          )}
        </div>

        <div className="form-field">
          <label>Channel-Icon ID (optional)</label>
          <input
            type="number"
            value={iconId}
            onChange={(e) => setIconId(Number(e.target.value))}
            placeholder="0 = Standard"
          />
        </div>
      </div>
    </>
  );
};
