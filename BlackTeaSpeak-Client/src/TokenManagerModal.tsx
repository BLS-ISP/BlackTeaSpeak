import { useState, useEffect } from 'react';
import { listen } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';
import { Token } from './types';
import { parseTs3Response, escapeTs3String } from './ts3parser';

interface TokenManagerModalProps {
  onClose: () => void;
}

export function TokenManagerModal({ onClose }: TokenManagerModalProps) {
  const [tokens, setTokens] = useState<Token[]>([]);

  useEffect(() => {
    let unlisten: () => void;

    async function setup() {
      unlisten = await listen<string>('server_event', (event) => {
        const parsed = parseTs3Response(event.payload);
        for (const row of parsed) {
          if (row.command === 'tokenlist') {
            setTokens(prev => {
              if (prev.find(t => t.token === row.args.token)) return prev;
              return [...prev, row.args as unknown as Token];
            });
          }
        }
      });
      refreshTokens();
    }
    setup();

    return () => {
      if (unlisten) unlisten();
    };
  }, []);

  const refreshTokens = () => {
    setTokens([]);
    invoke('send_command', { command: 'tokenlist' }).catch(console.error);
  };

  const handleAddToken = () => {
    const type = prompt("Type (0 = Server Group, 1 = Channel Group):", "0");
    const id1 = prompt("Group ID:");
    const id2 = prompt("Channel ID (0 if Server Group):", "0");
    const desc = prompt("Description:");

    if (type && id1) {
      invoke('send_command', { command: `tokenadd tokentype=${type} tokenid1=${id1} tokenid2=${id2 || 0} tokendescription=${escapeTs3String(desc || '')}` });
      setTimeout(refreshTokens, 500);
    }
  };

  const handleDeleteToken = (token: string) => {
    if (confirm("Delete this privilege key?")) {
      invoke('send_command', { command: `tokendelete token=${token}` });
      setTimeout(refreshTokens, 500);
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
