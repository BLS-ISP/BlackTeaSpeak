import React, { useState } from 'react';
import {
  Copy,
  Edit3,
  Hash,
  Plus,
  Shield,
  Trash2,
  Users,
} from 'lucide-react';
import { blackTeaStore, BlackTeaState } from '../../../store/useBlackTeaStore';

export const GroupManagementPane: React.FC<{ state: BlackTeaState }> = ({
  state,
}) => {
  const [groupKind, setGroupKind] = useState<'server' | 'channel'>('server');
  const [newGroupName, setNewGroupName] = useState('');

  return (
    <div className="modal-pane">
      <div
        style={{
          display: 'flex',
          justifyContent: 'space-between',
          alignItems: 'center',
          marginBottom: '8px',
        }}
      >
        <div style={{ display: 'flex', gap: '6px' }}>
          <button
            type="button"
            className={groupKind === 'server' ? 'btn-primary-sm' : 'btn-secondary-sm'}
            onClick={() => setGroupKind('server')}
          >
            <Users size={14} /> Servergruppen ({state.serverGroups.length})
          </button>
          <button
            type="button"
            className={
              groupKind === 'channel' ? 'btn-primary-sm' : 'btn-secondary-sm'
            }
            onClick={() => setGroupKind('channel')}
          >
            <Hash size={14} /> Channelgruppen ({state.channelGroups.length})
          </button>
        </div>
      </div>

      <form
        className="inline-form"
        onSubmit={(e) => {
          e.preventDefault();
          if (newGroupName.trim()) {
            if (groupKind === 'server') {
              void blackTeaStore.createServerGroup(newGroupName.trim());
            } else {
              void blackTeaStore.createChannelGroup(newGroupName.trim());
            }
            setNewGroupName('');
          }
        }}
      >
        <input
          type="text"
          placeholder={
            groupKind === 'server'
              ? 'Neue Servergruppe erstellen...'
              : 'Neue Channelgruppe erstellen...'
          }
          value={newGroupName}
          onChange={(e) => setNewGroupName(e.target.value)}
        />
        <button type="submit" className="btn-primary-sm">
          <Plus size={14} /> Erstellen
        </button>
      </form>

      {groupKind === 'server' ? (
        <div className="list-box">
          {state.serverGroups.map((g) => (
            <div key={g.sgid} className="list-item-row">
              <div>
                <strong>{g.name}</strong>
                <span className="sub-id">SGID #{g.sgid}</span>
              </div>
              <div style={{ display: 'flex', gap: '5px' }}>
                <button
                  className="btn-icon-xs"
                  onClick={() => {
                    blackTeaStore.patchState({ activeModal: 'permissions' });
                    void blackTeaStore.loadPermissionsData('servergroup', g.sgid);
                  }}
                  title="Rechte dieser Servergruppe bearbeiten"
                >
                  <Shield size={13} />
                </button>
                <button
                  className="btn-icon-xs"
                  onClick={() => {
                    const nextName = window.prompt(
                      `Neuer Name für Servergruppe '${g.name}':`,
                      g.name
                    );
                    if (nextName && nextName.trim()) {
                      void blackTeaStore.renameServerGroup(g.sgid, nextName.trim());
                    }
                  }}
                  title="Servergruppe umbenennen"
                >
                  <Edit3 size={13} />
                </button>
                <button
                  className="btn-icon-xs"
                  onClick={() => {
                    const copyName = window.prompt(
                      `Name für die Kopie von '${g.name}':`,
                      `${g.name} (Kopie)`
                    );
                    if (copyName && copyName.trim()) {
                      void blackTeaStore.copyServerGroup(g.sgid, copyName.trim());
                    }
                  }}
                  title="Servergruppe kopieren (inkl. Rechte)"
                >
                  <Copy size={13} />
                </button>
                <button
                  className="btn-icon-xs danger"
                  onClick={() => {
                    if (
                      window.confirm(
                        `Servergruppe '${g.name}' (#${g.sgid}) wirklich löschen?`
                      )
                    ) {
                      void blackTeaStore.deleteServerGroup(g.sgid);
                    }
                  }}
                  title="Servergruppe löschen"
                >
                  <Trash2 size={13} />
                </button>
              </div>
            </div>
          ))}
        </div>
      ) : (
        <div className="list-box">
          {state.channelGroups.map((cg) => (
            <div key={cg.cgid} className="list-item-row">
              <div>
                <strong>{cg.name}</strong>
                <span className="sub-id">CGID #{cg.cgid}</span>
              </div>
              <div style={{ display: 'flex', gap: '5px' }}>
                <button
                  className="btn-icon-xs"
                  onClick={() => {
                    blackTeaStore.patchState({ activeModal: 'permissions' });
                    void blackTeaStore.loadPermissionsData('channelgroup', cg.cgid);
                  }}
                  title="Rechte dieser Channelgruppe bearbeiten"
                >
                  <Shield size={13} />
                </button>
                <button
                  className="btn-icon-xs"
                  onClick={() => {
                    const nextName = window.prompt(
                      `Neuer Name für Channelgruppe '${cg.name}':`,
                      cg.name
                    );
                    if (nextName && nextName.trim()) {
                      void blackTeaStore.renameChannelGroup(
                        cg.cgid,
                        nextName.trim()
                      );
                    }
                  }}
                  title="Channelgruppe umbenennen"
                >
                  <Edit3 size={13} />
                </button>
                <button
                  className="btn-icon-xs danger"
                  onClick={() => {
                    if (
                      window.confirm(
                        `Channelgruppe '${cg.name}' (#${cg.cgid}) wirklich löschen?`
                      )
                    ) {
                      void blackTeaStore.deleteChannelGroup(cg.cgid);
                    }
                  }}
                  title="Channelgruppe löschen"
                >
                  <Trash2 size={13} />
                </button>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
};
