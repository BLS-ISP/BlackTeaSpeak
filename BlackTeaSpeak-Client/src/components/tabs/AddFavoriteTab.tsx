import React, { useState } from 'react';
import { AppConfig } from '../../App';

interface AddFavoriteTabProps {
  config: AppConfig;
  setActiveTab: (tab: string) => void;
  handleAddFavoriteSubmit: (fav: any) => void;
}

export function AddFavoriteTab({ config, setActiveTab, handleAddFavoriteSubmit }: AddFavoriteTabProps) {
  const [favName, setFavName] = useState("");
  const [favAddress, setFavAddress] = useState("");
  const [favPassword, setFavPassword] = useState("");
  const [favNickname, setFavNickname] = useState("");
  const [favIdentityId, setFavIdentityId] = useState("");

  const handleFavoriteIdentitySelect = (id: string) => {
    setFavIdentityId(id);
    const ident = config.identities.find(i => i.id === id);
    if (ident && ident.default_nickname) {
      setFavNickname(ident.default_nickname);
    }
  };

  const onSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    handleAddFavoriteSubmit({
      name: favName,
      address: favAddress,
      password: favPassword,
      nickname: favNickname,
      identity_id: favIdentityId
    });
  };

  return (
    <div className="tab-pane slide-in">
      <h2>Add Favorite</h2>
      <form className="form-layout" onSubmit={onSubmit}>
        <div className="input-group">
          <label>Server Name</label>
          <input required value={favName} onChange={e => setFavName(e.target.value)} placeholder="e.g. My Clan Server" />
        </div>
        <div className="input-group">
          <label>Address:Port</label>
          <input required value={favAddress} onChange={e => setFavAddress(e.target.value)} placeholder="e.g. 127.0.0.1:9987" />
        </div>
        <div className="input-group">
          <label>Password (optional)</label>
          <input type="password" value={favPassword} onChange={e => setFavPassword(e.target.value)} />
        </div>
        <div className="input-group">
          <label>Nickname</label>
          <input required value={favNickname} onChange={e => setFavNickname(e.target.value)} placeholder="Nickname" />
        </div>
        <div className="input-group">
          <label>Identity</label>
          <select value={favIdentityId} onChange={e => handleFavoriteIdentitySelect(e.target.value)}>
            <option value="">No Identity</option>
            {config.identities.map(i => <option key={i.id} value={i.id}>{i.name}</option>)}
          </select>
        </div>
        <div className="form-actions">
          <button type="button" className="btn-secondary" onClick={() => setActiveTab('favorites')}>Cancel</button>
          <button type="submit" className="btn-primary">Save Favorite</button>
        </div>
      </form>
    </div>
  );
}
