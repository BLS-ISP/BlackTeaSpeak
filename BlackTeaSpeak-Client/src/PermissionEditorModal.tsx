import { useState, useEffect } from 'react';
import { listen } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';
import { Permission } from './types';
import { parseTs3Response, escapeTs3String } from './ts3parser';

interface PermissionEditorModalProps {
  targetType: 'servergroup' | 'channelgroup' | 'client' | 'channel';
  targetId: string;
  onClose: () => void;
}

export function PermissionEditor({ targetType, targetId }: Omit<PermissionEditorModalProps, 'onClose'>) {
  const [permissions, setPermissions] = useState<Permission[]>([]);

  useEffect(() => {
    let unlisten: () => void;

    async function setup() {
      unlisten = await listen<string>('server_event', (event) => {
        const parsed = parseTs3Response(event.payload);
        for (const row of parsed) {
          const commandMatch = 
            (targetType === 'servergroup' && row.command === 'servergrouppermlist') ||
            (targetType === 'channelgroup' && row.command === 'channelgrouppermlist') ||
            (targetType === 'client' && row.command === 'clientpermlist') ||
            (targetType === 'channel' && row.command === 'channelpermlist');

          if (commandMatch) {
            setPermissions(prev => {
              const existingIndex = prev.findIndex(p => p.permid === row.args.permid || p.permname === row.args.permname);
              const newPerm = {
                permid: row.args.permid || '',
                permname: row.args.permsid || row.args.permname || `id_${row.args.permid}`,
                permvalue: row.args.permvalue || '0',
                permskip: row.args.permskip === '1',
                permnegated: row.args.permnegated === '1'
              };

              if (existingIndex !== -1) {
                const copy = [...prev];
                copy[existingIndex] = newPerm;
                return copy;
              }
              return [...prev, newPerm];
            });
          }
        }
      });
      refreshPermissions();
    }
    setup();

    return () => {
      if (unlisten) unlisten();
    };
  }, [targetType, targetId]);

  const refreshPermissions = () => {
    setPermissions([]);
    if (targetType === 'servergroup') invoke('send_command', { command: `servergrouppermlist sgid=${targetId}` });
    if (targetType === 'channelgroup') invoke('send_command', { command: `channelgrouppermlist cgid=${targetId}` });
    if (targetType === 'client') invoke('send_command', { command: `clientpermlist cldbid=${targetId}` });
    if (targetType === 'channel') invoke('send_command', { command: `channelpermlist cid=${targetId}` });
  };

  const handleAddPerm = () => {
    const name = prompt("Enter Permission Name (e.g. b_serverinstance_help_view):");
    const val = prompt("Enter Permission Value (e.g. 1):", "1");
    if (name && val) {
      const escName = escapeTs3String(name);
      if (targetType === 'servergroup') invoke('send_command', { command: `servergroupaddperm sgid=${targetId} permsid=${escName} permvalue=${val} permnegated=0 permskip=0` });
      if (targetType === 'channelgroup') invoke('send_command', { command: `channelgroupaddperm cgid=${targetId} permsid=${escName} permvalue=${val} permnegated=0 permskip=0` });
      if (targetType === 'client') invoke('send_command', { command: `clientaddperm cldbid=${targetId} permsid=${escName} permvalue=${val} permnegated=0 permskip=0` });
      if (targetType === 'channel') invoke('send_command', { command: `channeladdperm cid=${targetId} permsid=${escName} permvalue=${val}` });
      setTimeout(refreshPermissions, 500);
    }
  };

  const handleDeletePerm = (permname: string) => {
    if (confirm(`Remove permission ${permname}?`)) {
      const escName = escapeTs3String(permname);
      if (targetType === 'servergroup') invoke('send_command', { command: `servergroupdelperm sgid=${targetId} permsid=${escName}` });
      if (targetType === 'channelgroup') invoke('send_command', { command: `channelgroupdelperm cgid=${targetId} permsid=${escName}` });
      if (targetType === 'client') invoke('send_command', { command: `clientdelperm cldbid=${targetId} permsid=${escName}` });
      if (targetType === 'channel') invoke('send_command', { command: `channeldelperm cid=${targetId} permsid=${escName}` });
      setTimeout(refreshPermissions, 500);
    }
  };

  return (
    <div className="permission-editor-embedded" style={{ flexGrow: 1, display: 'flex', flexDirection: 'column' }}>
      <div className="info-header" style={{ marginBottom: '16px' }}>
        <h3 style={{ margin: 0 }}>Permissions - {targetType} ({targetId})</h3>
      </div>
      <div className="list-view" style={{ flexGrow: 1, overflowY: 'auto', marginBottom: '15px', paddingRight: '8px' }}>
        {permissions.map((p, idx) => (
          <div className="list-item" key={idx}>
            <div className="list-info">
              <h4>{p.permname}</h4>
              <span className="mono-text">Value: {p.permvalue} | Skip: {p.permskip ? 'Yes' : 'No'} | Negated: {p.permnegated ? 'Yes' : 'No'}</span>
            </div>
            <div className="card-actions">
              <button className="btn-icon muted" style={{ padding: '6px 10px' }} onClick={() => handleDeletePerm(p.permname)}>🗑️</button>
            </div>
          </div>
        ))}
        {permissions.length === 0 && <p className="loading-text">No custom permissions found.</p>}
      </div>
      <div className="form-actions" style={{ marginTop: 'auto' }}>
        <button className="btn-secondary" onClick={handleAddPerm}>+ Add Permission</button>
      </div>
    </div>
  );
}

export function PermissionEditorModal({ targetType, targetId, onClose }: PermissionEditorModalProps) {
  return (
    <div className="modal-overlay" style={{ zIndex: 1000 }}>
      <div className="modal-content" style={{ width: '800px', maxWidth: '90vw', height: '80vh' }}>
        <PermissionEditor targetType={targetType} targetId={targetId} />
        <div className="form-actions" style={{ marginTop: '24px' }}>
          <button className="btn-primary" onClick={onClose}>Close Editor</button>
        </div>
      </div>
    </div>
  );
}
