import { useState, useEffect } from 'react';
import { listen } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';
import { Ban } from './types';
import { parseTs3Response, escapeTs3String } from './ts3parser';

interface BanManagerModalProps {
  onClose: () => void;
}

export function BanManagerModal({ onClose }: BanManagerModalProps) {
  const [bans, setBans] = useState<Ban[]>([]);

  useEffect(() => {
    let unlisten: () => void;

    async function setup() {
      unlisten = await listen<string>('server_event', (event) => {
        const parsed = parseTs3Response(event.payload);
        for (const row of parsed) {
          if (row.command === 'banlist') {
            setBans(prev => {
              if (prev.find(b => b.banid === row.args.banid)) return prev;
              return [...prev, row.args as unknown as Ban];
            });
          }
        }
      });
      refreshBans();
    }
    setup();

    return () => {
      if (unlisten) unlisten();
    };
  }, []);

  const refreshBans = () => {
    setBans([]);
    invoke('send_command', { command: 'banlist' }).catch(console.error);
  };

  const handleAddBan = () => {
    const ip = prompt("Enter IP to ban (optional):");
    const uid = prompt("Enter UID to ban (optional):");
    const name = prompt("Enter Name to ban (optional):");
    const time = prompt("Enter time in seconds (0 = perm):", "0");
    const reason = prompt("Enter ban reason:");

    if (!ip && !uid && !name) {
      alert("You must provide at least one of IP, UID, or Name.");
      return;
    }

    let cmd = 'banadd';
    if (ip) cmd += ` ip=${escapeTs3String(ip)}`;
    if (uid) cmd += ` uid=${escapeTs3String(uid)}`;
    if (name) cmd += ` name=${escapeTs3String(name)}`;
    if (time) cmd += ` time=${time}`;
    if (reason) cmd += ` banreason=${escapeTs3String(reason)}`;

    invoke('send_command', { command: cmd });
    setTimeout(refreshBans, 500);
  };

  const handleDeleteBan = (banid: string) => {
    if (confirm(`Remove ban #${banid}?`)) {
      invoke('send_command', { command: `bandel banid=${banid}` });
      setTimeout(refreshBans, 500);
    }
  };

  return (
    <div className="modal-overlay" style={{ zIndex: 1000 }}>
      <div className="modal-content" style={{ width: '600px', maxWidth: '90vw' }}>
        <h2>Ban Manager</h2>
        <div className="list-view" style={{ maxHeight: '400px', overflowY: 'auto', marginBottom: '15px' }}>
          {bans.map((b, idx) => (
            <div className="list-item" key={idx}>
              <div className="list-info">
                <h4>{b.name || b.ip || b.uid}</h4>
                <span className="mono-text">
                  ID: {b.banid} | Time: {b.duration === '0' ? 'Permanent' : `${b.duration}s`} | Reason: {b.reason || 'None'}
                </span>
                <span className="mono-text">Banned By: {b.invokername}</span>
              </div>
              <div className="card-actions">
                <button className="btn-danger" onClick={() => handleDeleteBan(b.banid)}>Remove</button>
              </div>
            </div>
          ))}
          {bans.length === 0 && <p className="loading-text">No active bans found.</p>}
          <button className="btn-primary" style={{ marginTop: '10px' }} onClick={handleAddBan}>+ Add Ban Rule</button>
        </div>
        <div className="form-actions">
          <button className="btn-primary" onClick={onClose}>Close</button>
        </div>
      </div>
    </div>
  );
}
