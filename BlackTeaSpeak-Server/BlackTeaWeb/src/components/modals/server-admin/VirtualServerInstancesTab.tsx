import React, { useState } from 'react';
import { Play, Plus, Square, Trash2 } from 'lucide-react';
import { blackTeaStore, BlackTeaState } from '../../../store/useBlackTeaStore';

export const VirtualServerInstancesTab: React.FC<{ state: BlackTeaState }> = ({
  state,
}) => {
  const [newInstanceName, setNewInstanceName] = useState('BlackTeaSpeak Server #2');
  const [newInstancePort, setNewInstancePort] = useState(9990);
  const [newInstanceMaxClients, setNewInstanceMaxClients] = useState(32);

  return (
    <>
      <form
        className="perm-assign-form"
        onSubmit={(e) => {
          e.preventDefault();
          if (newInstanceName.trim()) {
            void blackTeaStore.createVirtualServer(
              newInstanceName.trim(),
              newInstancePort,
              newInstanceMaxClients
            );
          }
        }}
      >
        <input
          type="text"
          placeholder="Servername..."
          value={newInstanceName}
          onChange={(e) => setNewInstanceName(e.target.value)}
        />
        <input
          type="number"
          placeholder="UDP/QUIC Port"
          value={newInstancePort}
          onChange={(e) => setNewInstancePort(Number(e.target.value))}
          className="small-num"
        />
        <input
          type="number"
          placeholder="Max Clients"
          value={newInstanceMaxClients}
          onChange={(e) => setNewInstanceMaxClients(Number(e.target.value))}
          className="small-num"
        />
        <button type="submit" className="btn-primary-sm">
          <Plus size={14} /> Instanz erstellen
        </button>
      </form>

      <div className="list-box">
        {state.virtualServers.length === 0 ? (
          <div className="muted-note">Keine virtuellen Server geladen.</div>
        ) : (
          state.virtualServers.map((vs) => {
            const isOnline = vs.virtualserver_status === 'online';
            return (
              <div key={vs.virtualserver_id} className="list-item-row">
                <div>
                  <strong>
                    #{vs.virtualserver_id} {vs.virtualserver_name}
                  </strong>
                  <span className="sub-id">
                    Port {vs.virtualserver_port} • {vs.virtualserver_clientsonline}/
                    {vs.virtualserver_maxclients} Clients • Status:{' '}
                    {vs.virtualserver_status}
                  </span>
                </div>
                <div style={{ display: 'flex', gap: '6px' }}>
                  <button
                    className="btn-secondary-xs"
                    onClick={() =>
                      void blackTeaStore.toggleVirtualServerState(
                        vs.virtualserver_id,
                        !isOnline
                      )
                    }
                    title={isOnline ? 'Server stoppen' : 'Server starten'}
                  >
                    {isOnline ? <Square size={12} /> : <Play size={12} />}
                    {isOnline ? ' Stop' : ' Start'}
                  </button>
                  {vs.virtualserver_id !== 1 && (
                    <button
                      className="btn-icon-xs danger"
                      onClick={() => {
                        if (
                          window.confirm(
                            `Virtuellen Server #${vs.virtualserver_id} wirklich löschen?`
                          )
                        ) {
                          void blackTeaStore.deleteVirtualServer(vs.virtualserver_id);
                        }
                      }}
                      title="Virtuellen Server löschen"
                    >
                      <Trash2 size={13} />
                    </button>
                  )}
                </div>
              </div>
            );
          })
        )}
      </div>
    </>
  );
};
