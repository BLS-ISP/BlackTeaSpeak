import React, { useState, useEffect } from 'react';
import { Client } from '../../types';
import { eventBus } from '../../EventBus';

interface ClientInfoProps {
  selectedClient: Client;
  myClientId?: string;
  avatarCache?: Record<string, string>;
  onUploadAvatar?: (file: File) => void;
  fetchAvatar?: (hash: string) => void;
}

export function ClientInfo({ selectedClient, myClientId, avatarCache, onUploadAvatar, fetchAvatar }: ClientInfoProps) {
  const [clientInfo, setClientInfo] = useState<any>({});

  useEffect(() => {
    setClientInfo({}); // Reset when client changes
    
    if (selectedClient.client_flag_avatar && fetchAvatar && avatarCache && !avatarCache[selectedClient.client_flag_avatar]) {
      fetchAvatar(selectedClient.client_flag_avatar);
    }

    const unsubscribe = eventBus.subscribe((rows) => {
      for (const row of rows) {
        if (row.command === 'clientinfo' || row.command === 'clientgetids') {
          setClientInfo((prev: any) => ({ ...prev, ...row.args }));
        }
      }
    });
    
    import('@tauri-apps/api/core').then(({ invoke }) => {
      invoke('send_command', { command: `clientinfo clid=${selectedClient.clid}` }).catch(console.error);
      invoke('send_command', { command: `clientgetids cluid=${(selectedClient as any).client_unique_identifier || ''}` }).catch(console.error);
    });

    return () => { unsubscribe(); };
  }, [selectedClient, fetchAvatar, avatarCache]);

  return (
    <div className="info-pane">
      <div className="info-header" style={{ display: 'flex', gap: '16px', alignItems: 'center' }}>
        <div className="avatar-container" style={{ width: 80, height: 80, borderRadius: '50%', backgroundColor: '#2a2d32', display: 'flex', alignItems: 'center', justifyContent: 'center', overflow: 'hidden' }}>
          {selectedClient.client_flag_avatar && avatarCache && avatarCache[selectedClient.client_flag_avatar] ? (
            <img src={avatarCache[selectedClient.client_flag_avatar]} alt="Avatar" style={{ width: '100%', height: '100%', objectFit: 'cover' }} />
          ) : (
            <span style={{ fontSize: '32px' }}>{selectedClient.client_nickname.charAt(0).toUpperCase()}</span>
          )}
        </div>
        <div style={{ flexGrow: 1 }}>
          <h2>{selectedClient.client_nickname}</h2>
          <p>Client ID: {selectedClient.clid}</p>
          {selectedClient.clid === myClientId && onUploadAvatar && (
            <div>
              <input type="file" id="upload-avatar-input" style={{display: 'none'}} accept="image/*" onChange={(e) => {
                if (e.target.files && e.target.files[0]) {
                  onUploadAvatar(e.target.files[0]);
                  e.target.value = ''; // Reset
                }
              }} />
              <button className="btn-secondary" style={{ marginTop: 8, padding: '4px 8px', fontSize: 12 }} onClick={() => document.getElementById('upload-avatar-input')?.click()}>Upload Avatar</button>
            </div>
          )}
        </div>
      </div>
      <div className="info-body">
        <div className="info-row">
          <span>Type:</span>
          <span>{selectedClient.client_type === '0' ? 'Normal Client' : 'Server Query'}</span>
        </div>
        {Object.entries(clientInfo).map(([key, value]) => (
          <div key={key} className="info-row">
            <span>{key}:</span>
            <span>{String(value)}</span>
          </div>
        ))}
      </div>
    </div>
  );
}
