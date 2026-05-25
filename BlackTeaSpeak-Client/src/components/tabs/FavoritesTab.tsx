import React from 'react';
import { AppConfig } from '../../App';

interface FavoritesTabProps {
  config: AppConfig;
  isConnecting: boolean;
  setActiveTab: (tab: string) => void;
  handleConnect: (address: string, nickname: string, identityId: string) => void;
  handleDeleteFavorite: (id: string) => void;
}

export function FavoritesTab({ config, isConnecting, setActiveTab, handleConnect, handleDeleteFavorite }: FavoritesTabProps) {
  return (
    <div className="tab-pane slide-in">
      <h2>Your Favorites</h2>
      <div className="card-grid">
        {config.favorites.length === 0 && <p className="empty-state">No favorites saved yet.</p>}
        {config.favorites.map(fav => (
          <div className="card" key={fav.id}>
            <h3>{fav.name}</h3>
            <p className="card-meta">{fav.address}</p>
            <p className="card-meta">As: {fav.nickname}</p>
            <div className="card-actions">
              <button className="btn-primary" onClick={() => handleConnect(fav.address, fav.nickname, fav.identity_id)} disabled={isConnecting}>Connect</button>
              <button className="btn-danger" onClick={() => handleDeleteFavorite(fav.id)}>Delete</button>
            </div>
          </div>
        ))}
        <div className="card add-card" onClick={() => setActiveTab('add_favorite')}>
          + Add Favorite
        </div>
      </div>
    </div>
  );
}
