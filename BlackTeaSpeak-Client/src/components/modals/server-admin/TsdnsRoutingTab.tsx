import React, { useState } from 'react';
import { Plus, Trash2 } from 'lucide-react';
import { blackTeaStore, BlackTeaState } from '../../../store/useBlackTeaStore';

export const TsdnsRoutingTab: React.FC<{ state: BlackTeaState }> = ({ state }) => {
  const [tsdnsDomain, setTsdnsDomain] = useState('');
  const [tsdnsTarget, setTsdnsTarget] = useState('127.0.0.1:9987');

  return (
    <>
      <form
        className="perm-assign-form"
        onSubmit={(e) => {
          e.preventDefault();
          if (tsdnsDomain.trim() && tsdnsTarget.trim()) {
            void blackTeaStore.upsertTsdns(tsdnsDomain.trim(), tsdnsTarget.trim());
            setTsdnsDomain('');
          }
        }}
      >
        <input
          type="text"
          placeholder="Domain (z.B. voice.clan.de)"
          value={tsdnsDomain}
          onChange={(e) => setTsdnsDomain(e.target.value)}
        />
        <input
          type="text"
          placeholder="Ziel IP:Port (z.B. 127.0.0.1:9987)"
          value={tsdnsTarget}
          onChange={(e) => setTsdnsTarget(e.target.value)}
        />
        <button
          type="submit"
          className="btn-primary-sm"
          disabled={state.connectionState !== 'connected'}
        >
          <Plus size={14} /> Eintrag speichern
        </button>
      </form>

      <div className="list-box">
        {state.tsdnsRecords.length === 0 ? (
          <div className="muted-note">Keine TSDNS-Mappings hinterlegt.</div>
        ) : (
          state.tsdnsRecords.map((r) => (
            <div key={r.domain} className="list-item-row">
              <div>
                <strong>{r.domain}</strong>
                <span className="sub-id">→ {r.target}</span>
              </div>
              <button
                className="btn-icon-xs danger"
                onClick={() => void blackTeaStore.deleteTsdns(r.domain)}
              >
                <Trash2 size={13} />
              </button>
            </div>
          ))
        )}
      </div>
    </>
  );
};
