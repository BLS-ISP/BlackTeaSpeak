import React, { useState } from 'react';
import { Plus, Trash2 } from 'lucide-react';
import { blackTeaStore, BlackTeaState } from '../../../store/useBlackTeaStore';

export const ComplaintsTab: React.FC<{ state: BlackTeaState }> = ({ state }) => {
  const [complainTargetDbid, setComplainTargetDbid] = useState<number>(
    state.clients.find((c) => c.clid !== state.selfClientId)?.client_database_id ?? 0
  );
  const [complainMessage, setComplainMessage] = useState('');

  return (
    <>
      <form
        className="perm-assign-form"
        onSubmit={(e) => {
          e.preventDefault();
          if (complainTargetDbid > 0 && complainMessage.trim()) {
            void blackTeaStore.submitComplaint(
              complainTargetDbid,
              complainMessage.trim()
            );
            setComplainMessage('');
          }
        }}
      >
        <select
          value={complainTargetDbid}
          onChange={(e) => setComplainTargetDbid(Number(e.target.value))}
        >
          <option value={0}>— Ziel-Client wählen —</option>
          {state.clients.map((cl) => (
            <option key={cl.clid} value={cl.client_database_id}>
              {cl.client_nickname} (DBID #{cl.client_database_id})
            </option>
          ))}
        </select>
        <input
          type="text"
          placeholder="Beschwerdetext / Begründung..."
          value={complainMessage}
          onChange={(e) => setComplainMessage(e.target.value)}
          style={{ flex: 1 }}
        />
        <button type="submit" className="btn-primary-sm">
          <Plus size={14} /> Beschwerde einreichen
        </button>
      </form>

      <div className="perm-table-wrap">
        <table className="data-table">
          <thead>
            <tr>
              <th>Ziel-Client</th>
              <th>Eingereicht von</th>
              <th>Beschwerdetext</th>
              <th>Aktionen</th>
            </tr>
          </thead>
          <tbody>
            {state.complaints.length === 0 ? (
              <tr>
                <td colSpan={4} className="muted-note">
                  Keine Beschwerden (Complaints) eingereicht.
                </td>
              </tr>
            ) : (
              state.complaints.map((c, idx) => (
                <tr key={idx}>
                  <td>
                    {c.tname} (#{c.tcldbid})
                  </td>
                  <td>
                    {c.fname} (#{c.fcldbid})
                  </td>
                  <td>{c.message}</td>
                  <td>
                    <div style={{ display: 'flex', gap: '6px' }}>
                      <button
                        className="btn-icon-xs danger"
                        onClick={() =>
                          void blackTeaStore.deleteComplaint(c.tcldbid, c.fcldbid)
                        }
                        title="Diese Beschwerde löschen"
                      >
                        <Trash2 size={13} />
                      </button>
                      <button
                        className="btn-secondary-xs"
                        onClick={() =>
                          void blackTeaStore.deleteAllComplaints(c.tcldbid)
                        }
                        title="Alle Beschwerden gegen diesen Client löschen"
                      >
                        Alle gegen #{c.tcldbid}
                      </button>
                    </div>
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
