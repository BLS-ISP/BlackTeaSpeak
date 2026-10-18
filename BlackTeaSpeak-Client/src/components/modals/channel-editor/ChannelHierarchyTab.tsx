import React from 'react';
import { ChannelInfo } from '../../../types/protocol';

export type ChannelTypeOption = 'permanent' | 'semi_permanent' | 'temporary' | 'default';

export interface ChannelHierarchyTabProps {
  channelType: ChannelTypeOption;
  setChannelType: (v: ChannelTypeOption) => void;
  pid: number;
  handleParentChange: (nextPid: number) => void;
  order: number;
  setOrder: (v: number) => void;
  availableParentChannels: ChannelInfo[];
  siblingChannels: ChannelInfo[];
}

export const ChannelHierarchyTab: React.FC<ChannelHierarchyTabProps> = ({
  channelType,
  setChannelType,
  pid,
  handleParentChange,
  order,
  setOrder,
  availableParentChannels,
  siblingChannels,
}) => {
  return (
    <>
      <div className="form-field">
        <label>Channel-Typ (Lebensdauer & Server-Verhalten)</label>
        <div
          style={{
            display: 'grid',
            gridTemplateColumns: 'repeat(2, 1fr)',
            gap: '10px',
          }}
        >
          <label
            style={{
              display: 'flex',
              alignItems: 'flex-start',
              gap: '10px',
              padding: '10px 12px',
              borderRadius: '8px',
              border:
                channelType === 'permanent'
                  ? '1px solid #38bdf8'
                  : '1px solid rgba(255,255,255,0.1)',
              background:
                channelType === 'permanent'
                  ? 'rgba(56, 189, 248, 0.12)'
                  : 'rgba(255,255,255,0.02)',
              cursor: 'pointer',
            }}
          >
            <input
              type="radio"
              name="channelType"
              checked={channelType === 'permanent'}
              onChange={() => setChannelType('permanent')}
            />
            <div>
              <div style={{ fontWeight: 600, fontSize: '13px' }}>Permanent</div>
              <div style={{ fontSize: '11px', color: 'rgba(255,255,255,0.6)' }}>
                Bleibt dauerhaft in der Server-Datenbank gespeichert, auch nach Neustarts.
              </div>
            </div>
          </label>

          <label
            style={{
              display: 'flex',
              alignItems: 'flex-start',
              gap: '10px',
              padding: '10px 12px',
              borderRadius: '8px',
              border:
                channelType === 'semi_permanent'
                  ? '1px solid #38bdf8'
                  : '1px solid rgba(255,255,255,0.1)',
              background:
                channelType === 'semi_permanent'
                  ? 'rgba(56, 189, 248, 0.12)'
                  : 'rgba(255,255,255,0.02)',
              cursor: 'pointer',
            }}
          >
            <input
              type="radio"
              name="channelType"
              checked={channelType === 'semi_permanent'}
              onChange={() => setChannelType('semi_permanent')}
            />
            <div>
              <div style={{ fontWeight: 600, fontSize: '13px' }}>Semi-Permanent</div>
              <div style={{ fontSize: '11px', color: 'rgba(255,255,255,0.6)' }}>
                Bleibt bestehen bis der virtuelle Server neu gestartet wird, auch wenn er leer ist.
              </div>
            </div>
          </label>

          <label
            style={{
              display: 'flex',
              alignItems: 'flex-start',
              gap: '10px',
              padding: '10px 12px',
              borderRadius: '8px',
              border:
                channelType === 'temporary'
                  ? '1px solid #38bdf8'
                  : '1px solid rgba(255,255,255,0.1)',
              background:
                channelType === 'temporary'
                  ? 'rgba(56, 189, 248, 0.12)'
                  : 'rgba(255,255,255,0.02)',
              cursor: 'pointer',
            }}
          >
            <input
              type="radio"
              name="channelType"
              checked={channelType === 'temporary'}
              onChange={() => setChannelType('temporary')}
            />
            <div>
              <div style={{ fontWeight: 600, fontSize: '13px' }}>Temporär</div>
              <div style={{ fontSize: '11px', color: 'rgba(255,255,255,0.6)' }}>
                Wird automatisch gelöscht, sobald der letzte Client den Channel verlässt.
              </div>
            </div>
          </label>

          <label
            style={{
              display: 'flex',
              alignItems: 'flex-start',
              gap: '10px',
              padding: '10px 12px',
              borderRadius: '8px',
              border:
                channelType === 'default'
                  ? '1px solid #34d399'
                  : '1px solid rgba(255,255,255,0.1)',
              background:
                channelType === 'default'
                  ? 'rgba(52, 211, 153, 0.12)'
                  : 'rgba(255,255,255,0.02)',
              cursor: 'pointer',
            }}
          >
            <input
              type="radio"
              name="channelType"
              checked={channelType === 'default'}
              onChange={() => setChannelType('default')}
            />
            <div>
              <div style={{ fontWeight: 600, fontSize: '13px', color: '#34d399' }}>
                Standard-Channel (Default Channel)
              </div>
              <div style={{ fontSize: '11px', color: 'rgba(255,255,255,0.6)' }}>
                Eingangshalle für neue Clients beim Verbinden (automatisch Permanent).
              </div>
            </div>
          </label>
        </div>
      </div>

      <div className="form-row-2">
        <div className="form-field">
          <label>Übergeordneter Channel (`cpid` / Parent)</label>
          <select value={pid} onChange={(e) => handleParentChange(Number(e.target.value))}>
            <option value={0}>— Hauptebene (Root / Server-Ebene) —</option>
            {availableParentChannels.map((ch) => (
              <option key={ch.cid} value={ch.cid}>
                {ch.pid > 0 ? '↳ ' : ''}
                {ch.channel_name} (#{ch.cid})
              </option>
            ))}
          </select>
        </div>

        <div className="form-field">
          <label>Sortier-Reihenfolge (`channel_order` — einsortieren nach)</label>
          <select value={order} onChange={(e) => setOrder(Number(e.target.value))}>
            <option value={0}>— Ganz oben (Erster Channel in dieser Ebene) —</option>
            {siblingChannels.map((sib) => (
              <option key={sib.cid} value={sib.cid}>
                Nach: {sib.channel_name} (#{sib.cid})
              </option>
            ))}
          </select>
        </div>
      </div>
    </>
  );
};
