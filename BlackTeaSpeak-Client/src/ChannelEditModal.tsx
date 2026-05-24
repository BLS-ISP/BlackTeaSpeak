import { useState, useEffect } from 'react';
import { escapeTs3String } from './ts3parser';
import { Toast } from './ui/Toast';
import { eventBus } from './EventBus';

interface ChannelEditModalProps {
  cid?: string; // If provided, we edit.
  cpid?: string; // If provided, we create under this parent.
  onClose: () => void;
}

export function ChannelEditModal({ cid, cpid, onClose }: ChannelEditModalProps) {
  const [name, setName] = useState('');
  const [topic, setTopic] = useState('');
  const [description, setDescription] = useState('');
  const [password, setPassword] = useState('');
  const [maxClients, setMaxClients] = useState('-1');
  const [loading, setLoading] = useState(!!cid);

  useEffect(() => {
    let unsubscribe: () => void;
    
    if (cid) {
      unsubscribe = eventBus.subscribe((rows) => {
        for (const row of rows) {
          if (row.command === 'channelinfo') {
            setName(row.args.channel_name || '');
            setTopic(row.args.channel_topic || '');
            setDescription(row.args.channel_description || '');
            setMaxClients(row.args.channel_maxclients || '-1');
            setLoading(false);
          }
        }
      });
      
      import('@tauri-apps/api/core').then(({ invoke }) => {
        invoke('send_command', { command: `channelinfo cid=${cid}` }).catch(console.error);
      });
    }

    return () => { if (unsubscribe) unsubscribe(); };
  }, [cid]);

  const handleSave = () => {
    if (!name.trim()) {
      Toast.error("Channel name is required.");
      return;
    }

    let cmd = cid ? `channeledit cid=${cid}` : `channelcreate cpid=${cpid || 0}`;
    cmd += ` channel_name=${escapeTs3String(name)}`;
    if (topic) cmd += ` channel_topic=${escapeTs3String(topic)}`;
    if (description) cmd += ` channel_description=${escapeTs3String(description)}`;
    if (password) cmd += ` channel_password=${escapeTs3String(password)}`;
    if (maxClients !== '-1') cmd += ` channel_maxclients=${maxClients} channel_flag_maxclients_unlimited=0`;
    else cmd += ` channel_flag_maxclients_unlimited=1`;

    import('@tauri-apps/api/core').then(({ invoke }) => {
      invoke('send_command', { command: cmd }).then(() => {
        onClose();
      }).catch(e => {
        Toast.error("Error saving channel: " + e);
      });
    });
  };

  if (loading) return (
    <div className="modal-overlay">
      <div className="modal-content"><p>Loading...</p></div>
    </div>
  );

  return (
    <div className="modal-overlay" style={{ zIndex: 1000 }}>
      <div className="modal-content" style={{ width: '400px', maxWidth: '90vw' }}>
        <h2>{cid ? 'Edit Channel' : 'Create Channel'}</h2>
        
        <div className="form-group">
          <label>Name:</label>
          <input type="text" value={name} onChange={e => setName(e.target.value)} />
        </div>
        
        <div className="form-group">
          <label>Password (optional):</label>
          <input type="password" value={password} onChange={e => setPassword(e.target.value)} />
        </div>
        
        <div className="form-group">
          <label>Topic:</label>
          <input type="text" value={topic} onChange={e => setTopic(e.target.value)} />
        </div>
        
        <div className="form-group">
          <label>Description:</label>
          <textarea value={description} onChange={e => setDescription(e.target.value)} rows={3} />
        </div>

        <div className="form-group">
          <label>Max Clients (-1 for unlimited):</label>
          <input type="number" value={maxClients} onChange={e => setMaxClients(e.target.value)} />
        </div>

        <div className="form-actions">
          <button className="btn-secondary" onClick={onClose}>Cancel</button>
          <button className="btn-primary" onClick={handleSave}>Save</button>
        </div>
      </div>
    </div>
  );
}
