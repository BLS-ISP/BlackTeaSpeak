import { useState, useEffect } from 'react';

import { invoke } from '@tauri-apps/api/core';
import { Token } from './types';
import { escapeTs3String } from './ts3parser';
import { Dialogs } from './ui/Dialogs';
import { Toast } from './ui/Toast';
import { eventBus } from './EventBus';

interface TokenManagerModalProps {
  onClose: () => void;
}

export function TokenManagerModal({ onClose }: TokenManagerModalProps) {
  const [tokens, setTokens] = useState<Token[]>([]);

  const [, setWaitingForRefresh] = useState(false);

  useEffect(() => {
    const unsubscribe = eventBus.subscribe((rows) => {
      for (const row of rows) {
        if (row.command === 'tokenlist') {
          setTokens(prev => {
            if (prev.find(t => t.token === row.args.token)) return prev;
            return [...prev, row.args as any as Token];
          });
        } else if (row.command === 'error' && row.args.msg === 'ok') {
          setWaitingForRefresh(waiting => {
            if (waiting) {
              refreshTokens();
              return false;
            }
            return waiting;
          });
        }
      }
    });

    refreshTokens();

    return () => {
      unsubscribe();
    };
  }, []);

  const refreshTokens = () => {
    setTokens([]);
    invoke('send_command', { command: 'tokenlist' }).catch(console.error);
  };

  const handleAddToken = async () => {
    const type = await Dialogs.prompt("Token Type", "Type (0 = Server Group, 1 = Channel Group):", "0");
    if (type === null) return;
    const id1 = await Dialogs.prompt("Group ID", "Group ID:");
    if (id1 === null) return;
    const id2 = await Dialogs.prompt("Channel ID", "Channel ID (0 if Server Group):", "0");
    if (id2 === null) return;
    const desc = await Dialogs.prompt("Description", "Description:");
    if (desc === null) return;

    if (type && id1) {
      invoke('send_command', { command: `tokenadd tokentype=${type} tokenid1=${id1} tokenid2=${id2 || 0} tokendescription=${escapeTs3String(desc || '')}` });
      setWaitingForRefresh(true);
      Toast.success("Privilege Key created");
    }
  };

  const handleDeleteToken = async (token: string) => {
    if (await Dialogs.confirm("Delete Token", "Delete this privilege key?")) {
      invoke('send_command', { command: `tokendelete token=${token}` });
      setWaitingForRefresh(true);
      Toast.success("Privilege Key deleted");
    }
  };

  return (
    <div className="modal-overlay" style={{ zIndex: 1000 }}>
      <div className="modal-content" style={{ width: '600px', maxWidth: '90vw' }}>
        <h2>Privilege Keys</h2>
        <div className="list-view" style={{ maxHeight: '400px', overflowY: 'auto', marginBottom: '15px' }}>
          {tokens.map((t, idx) => (
            <div className="list-item" key={idx}>
              <div className="list-info">
                <h4>{t.description || 'No Description'}</h4>
                <span className="mono-text">
                  Token: {t.token}
                </span>
                <span className="mono-text">
                  Type: {t.type === '0' ? 'Server Group' : 'Channel Group'} | Group ID: {t.id1}
                </span>
              </div>
              <div className="card-actions">
                <button className="btn-secondary" onClick={() => navigator.clipboard.writeText(t.token)}>Copy</button>
                <button className="btn-danger" onClick={() => handleDeleteToken(t.token)}>Remove</button>
              </div>
            </div>
          ))}
          {tokens.length === 0 && <p className="loading-text">No active keys found.</p>}
          <button className="btn-primary" style={{ marginTop: '10px' }} onClick={handleAddToken}>+ Add Privilege Key</button>
        </div>
        <div className="form-actions">
          <button className="btn-primary" onClick={onClose}>Close</button>
        </div>
      </div>
    </div>
  );
}
