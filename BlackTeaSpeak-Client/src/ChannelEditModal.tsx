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
  const [codec, setCodec] = useState('4'); // 4 = Opus Voice, 5 = Opus Music
  const [codecQuality, setCodecQuality] = useState('10');
  const [channelType, setChannelType] = useState('permanent'); // temporary, semi, permanent
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
            if (row.args.channel_password) setPassword(row.args.channel_password);
            if (row.args.channel_codec) setCodec(row.args.channel_codec);
            if (row.args.channel_codec_quality) setCodecQuality(row.args.channel_codec_quality);
            
            if (row.args.channel_flag_permanent === '1') setChannelType('permanent');
            else if (row.args.channel_flag_semi_permanent === '1') setChannelType('semi');
            else setChannelType('temporary');
            
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
    
    cmd += ` channel_codec=${codec} channel_codec_quality=${codecQuality}`;
    
    if (channelType === 'permanent') cmd += ` channel_flag_permanent=1 channel_flag_semi_permanent=0`;
    else if (channelType === 'semi') cmd += ` channel_flag_permanent=0 channel_flag_semi_permanent=1`;
    else cmd += ` channel_flag_permanent=0 channel_flag_semi_permanent=0`;

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

        <div className="form-group">
          <label>Audio Codec:</label>
          <select value={codec} onChange={e => setCodec(e.target.value)}>
            <option value="4">Opus Voice</option>
            <option value="5">Opus Music</option>
            <option value="0">Speex</option>
          </select>
        </div>

        <div className="form-group">
          <label>Codec Quality (0-10):</label>
          <div style={{ display: 'flex', alignItems: 'center', gap: '10px' }}>
            <input type="range" min="0" max="10" value={codecQuality} onChange={e => setCodecQuality(e.target.value)} style={{ flex: 1 }} />
            <span>{codecQuality}</span>
          </div>
        </div>

        <div className="form-group">
          <label>Channel Type:</label>
          <div style={{ display: 'flex', gap: '10px' }}>
            <label><input type="radio" name="ctype" value="temporary" checked={channelType === 'temporary'} onChange={e => setChannelType(e.target.value)} /> Temporary</label>
            <label><input type="radio" name="ctype" value="semi" checked={channelType === 'semi'} onChange={e => setChannelType(e.target.value)} /> Semi-Permanent</label>
            <label><input type="radio" name="ctype" value="permanent" checked={channelType === 'permanent'} onChange={e => setChannelType(e.target.value)} /> Permanent</label>
          </div>
        </div>

        <div className="form-actions">
          <button className="btn-secondary" onClick={onClose}>Cancel</button>
          <button className="btn-primary" onClick={handleSave}>Save</button>
        </div>
      </div>
    </div>
  );
}
