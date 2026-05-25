import React, { useState } from 'react';
import { AppConfig, Identity } from '../../App';

interface IdentitiesTabProps {
  config: AppConfig;
  handleGenerateIdentity: (name: string) => void;
  handleDeleteIdentity: (id: string) => void;
  handleSaveIdentity: (identity: Identity) => void;
}

export function IdentitiesTab({ config, handleGenerateIdentity, handleDeleteIdentity, handleSaveIdentity }: IdentitiesTabProps) {
  const [newIdentityName, setNewIdentityName] = useState("");
  const [editingIdentity, setEditingIdentity] = useState<Identity | null>(null);

  const onGenerate = () => {
    if (!newIdentityName.trim()) return;
    handleGenerateIdentity(newIdentityName);
    setNewIdentityName("");
  };

  const onSave = (e: React.FormEvent) => {
    e.preventDefault();
    if (!editingIdentity) return;
    handleSaveIdentity(editingIdentity);
    setEditingIdentity(null);
  };

  return (
    <div className="tab-pane slide-in">
      <h2>Identity Manager</h2>
      <div className="add-bar">
        <input 
          placeholder="New Identity Name" 
          value={newIdentityName} 
          onChange={e => setNewIdentityName(e.target.value)}
        />
        <button className="btn-primary" onClick={onGenerate}>Generate</button>
      </div>
      
      <div className="list-view">
        {config.identities.length === 0 && <p className="empty-state">No identities. Generate one to store permissions on servers.</p>}
        {config.identities.map(ident => (
          <div className="list-item" key={ident.id}>
            {editingIdentity?.id === ident.id ? (
              <form className="edit-form" onSubmit={onSave} style={{ width: '100%', display: 'flex', gap: '10px', flexDirection: 'column' }}>
                <div className="input-group">
                  <label>Identity Name</label>
                  <input value={editingIdentity.name} onChange={e => setEditingIdentity({...editingIdentity, name: e.target.value})} />
                </div>
                <div className="input-group">
                  <label>Default Nickname</label>
                  <input value={editingIdentity.default_nickname} onChange={e => setEditingIdentity({...editingIdentity, default_nickname: e.target.value})} />
                </div>
                <div className="form-actions" style={{ marginTop: '0' }}>
                  <button type="button" className="btn-secondary" onClick={() => setEditingIdentity(null)}>Cancel</button>
                  <button type="submit" className="btn-primary">Save</button>
                </div>
              </form>
            ) : (
              <>
                <div className="list-info">
                  <h4>{ident.name}</h4>
                  <p className="card-meta">Default Nickname: {ident.default_nickname}</p>
                  <span className="mono-text" title={ident.uid}>UID: {ident.uid.substring(0, 16)}...</span>
                </div>
                <div className="card-actions" style={{ marginTop: 0 }}>
                  <button className="btn-secondary" onClick={() => setEditingIdentity(ident)}>Edit</button>
                  <button className="btn-danger" onClick={() => handleDeleteIdentity(ident.id)}>Delete</button>
                </div>
              </>
            )}
          </div>
        ))}
      </div>
    </div>
  );
}
