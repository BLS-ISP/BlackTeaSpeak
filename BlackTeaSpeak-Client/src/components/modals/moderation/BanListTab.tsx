import React, { useState } from 'react';
import { Plus, Trash2 } from 'lucide-react';
import { blackTeaStore, BlackTeaState } from '../../../store/useBlackTeaStore';

export const BanListTab: React.FC<{ state: BlackTeaState }> = ({ state }) => {
  const [banIp, setBanIp] = useState('');
  const [banName, setBanName] = useState('');
  const [banUid, setBanUid] = useState('');
  const [banTime, setBanTime] = useState(3600);
  const [banReason, setBanReason] = useState('');

  return (
    <>
      <form
        className="perm-assign-form"
        onSubmit={(e) => {
          e.preventDefault();
          void blackTeaStore.addBanRule({
            ip: banIp,
            name: banName,
            uid: banUid,
            timeSeconds: banTime,
            reason: banReason,
          });
          setBanIp('');
          setBanName('');
          setBanUid('');
        }}
      >
        <input
          type="text"
          placeholder="IP / Regex"
          value={banIp}
          onChange={(e) => setBanIp(e.target.value)}
        />
        <input
          type="text"
          placeholder="Nickname"
          value={banName}
          onChange={(e) => setBanName(e.target.value)}
        />
        <input
          type="text"
          placeholder="Unique ID"
          value={banUid}
          onChange={(e) => setBanUid(e.target.value)}
        />
        <select
          value={banTime}
          onChange={(e) => setBanTime(Number(e.target.value))}
        >
          <option value={900}>15 Min (900s)</option>
          <option value={3600}>1 Std (3600s)</option>
          <option value={86400}>24 Std (1 Tag)</option>
          <option value={604800}>7 Tage</option>
          <option value={0}>Permanent (0s)</option>
        </select>
        <input
          type="text"
          placeholder="Grund"
          value={banReason}
          onChange={(e) => setBanReason(e.target.value)}
        />
        <button type="submit" className="btn-danger-sm">
          <Plus size={14} /> Ban anlegen
        </button>
        {state.bans.length > 0 && (
          <button
            type="button"
            className="btn-secondary-sm"
            onClick={() => {
              if (window.confirm('Wirklich alle Ban-Regeln löschen?')) {
                void blackTeaStore.clearAllBans();
              }
            }}
          >
            Alle löschen
          </button>
        )}
      </form>

      <div className="perm-table-wrap">
        <table className="data-table">
          <thead>
            <tr>
              <th>ID</th>
              <th>Name / IP / UID</th>
              <th>Dauer</th>
              <th>Grund</th>
              <th>Ersteller</th>
              <th>Aktion</th>
            </tr>
          </thead>
          <tbody>
            {state.bans.length === 0 ? (
              <tr>
                <td colSpan={6} className="muted-note">
                  Keine aktiven Bans vorhanden.
                </td>
              </tr>
            ) : (
              state.bans.map((b) => (
                <tr key={b.banid}>
                  <td>#{b.banid}</td>
                  <td>
                    {[
                      b.name && `Name: ${b.name}`,
                      b.ip && `IP: ${b.ip}`,
                      b.uid && `UID: ${b.uid}`,
                    ]
                      .filter(Boolean)
                      .join(' • ')}
                  </td>
                  <td>
                    {b.duration === 0
                      ? 'Permanent'
                      : `${Math.round(b.duration / 60)} Min`}
                  </td>
                  <td>{b.reason || '—'}</td>
                  <td>{b.invokername}</td>
                  <td>
                    <button
                      className="btn-icon-xs danger"
                      onClick={() => void blackTeaStore.deleteBanRule(b.banid)}
                      title="Ban aufheben"
                    >
                      <Trash2 size={13} />
                    </button>
                  </td>
                </tr>
              ))
            )}
          </tbody>
        </table>
      </div>
    </>
  );
};
