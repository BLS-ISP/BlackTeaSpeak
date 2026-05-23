import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import ConnectedView from "./ConnectedView";
import "./App.scss";

export type Identity = {
  id: string;
  name: string;
  private_key: string;
  public_key: string;
  uid: string;
  default_nickname: string;
  audio_input_device?: string;
  audio_output_device?: string;
  input_amplification?: number;
  output_amplification?: number;
  voice_transmission_mode?: string;
  voice_activation_threshold?: number;
  ptt_hotkey?: string;
  whisper_hotkey?: string;
  whisper_targets?: { client_ids: string[], channel_ids: string[] };
};

type Favorite = {
  id: string;
  name: string;
  address: string;
  password?: string;
  nickname: string;
  identity_id: string;
};

export type AppConfig = {
  identities: Identity[];
  favorites: Favorite[];
};

function App() {
  const [activeTab, setActiveTab] = useState("favorites");
  const [config, setConfig] = useState<AppConfig>({ identities: [], favorites: [] });
  const [status, setStatus] = useState("");
  const [isConnecting, setIsConnecting] = useState(false);
  const [isConnected, setIsConnected] = useState(false);
  
  // Connect Form State
  const [address, setAddress] = useState("127.0.0.1:9987");
  const [nickname, setNickname] = useState("NewUser");
  const [selectedIdentity, setSelectedIdentity] = useState("");
  const [activeIdentity, setActiveIdentity] = useState<Identity | null>(null);

  // New Identity State
  const [newIdentityName, setNewIdentityName] = useState("");

  // New Favorite State
  const [favName, setFavName] = useState("");
  const [favAddress, setFavAddress] = useState("");
  const [favPassword, setFavPassword] = useState("");
  const [favNickname, setFavNickname] = useState("");
  const [favIdentityId, setFavIdentityId] = useState("");

  // Edit Identity State
  const [editingIdentity, setEditingIdentity] = useState<Identity | null>(null);

  useEffect(() => {
    loadConfig();
  }, []);

  async function loadConfig() {
    try {
      const cfg: AppConfig = await invoke("load_config");
      setConfig(cfg);
      if (cfg.identities.length > 0 && !selectedIdentity) {
        setSelectedIdentity(cfg.identities[0].id);
      }
    } catch (e) {
      console.error("Failed to load config", e);
    }
  }

  async function saveConfig(newConfig: AppConfig) {
    try {
      await invoke("save_config", { config: newConfig });
      setConfig(newConfig);
    } catch (e) {
      console.error("Failed to save config", e);
    }
  }

  async function handleConnect(targetAddress: string, targetNickname: string, targetIdentityId: string) {
    setIsConnecting(true);
    setStatus("Connecting...");
    
    let pubkey = null;
    if (targetIdentityId) {
      const ident = config.identities.find(i => i.id === targetIdentityId);
      if (ident) {
        pubkey = ident.public_key;
        setActiveIdentity(ident);
      }
    }

    try {
      const response = await invoke("connect_to_server", { 
        address: targetAddress,
        nickname: targetNickname,
        identityPublicKey: pubkey
      });
      setStatus(response as string);
      setIsConnected(true);
    } catch (error) {
      setStatus(`Error: ${error}`);
      setIsConnected(false);
    } finally {
      setIsConnecting(false);
    }
  }

  function handleDisconnect() {
    setIsConnected(false);
    setStatus("Disconnected.");
  }

  async function handleGenerateIdentity() {
    if (!newIdentityName.trim()) return;
    try {
      const newIdent: Identity = await invoke("generate_identity", { name: newIdentityName });
      const newConfig = { ...config, identities: [...config.identities, newIdent] };
      await saveConfig(newConfig);
      setNewIdentityName("");
    } catch (e) {
      console.error("Failed to generate identity", e);
    }
  }

  async function handleAddFavorite(e: React.FormEvent) {
    e.preventDefault();
    const newFav: Favorite = {
      id: "fav_" + Date.now().toString(),
      name: favName,
      address: favAddress,
      password: favPassword,
      nickname: favNickname,
      identity_id: favIdentityId
    };
    const newConfig = { ...config, favorites: [...config.favorites, newFav] };
    await saveConfig(newConfig);
    
    setFavName("");
    setFavAddress("");
    setFavPassword("");
    setFavNickname("");
    setActiveTab("favorites");
  }

  async function handleDeleteIdentity(id: string) {
    const newConfig = { ...config, identities: config.identities.filter(i => i.id !== id) };
    await saveConfig(newConfig);
  }

  async function handleSaveIdentity(e: React.FormEvent) {
    e.preventDefault();
    if (!editingIdentity) return;
    const newConfig = {
      ...config,
      identities: config.identities.map(i => i.id === editingIdentity.id ? editingIdentity : i)
    };
    await saveConfig(newConfig);
    setEditingIdentity(null);
  }

  function handleFavoriteIdentitySelect(id: string) {
    setFavIdentityId(id);
    const ident = config.identities.find(i => i.id === id);
    if (ident && ident.default_nickname) {
      setFavNickname(ident.default_nickname);
    }
  }

  function handleConnectIdentitySelect(id: string) {
    setSelectedIdentity(id);
    const ident = config.identities.find(i => i.id === id);
    if (ident && ident.default_nickname) {
      setNickname(ident.default_nickname);
    }
  }

  async function handleDeleteFavorite(id: string) {
    const newConfig = { ...config, favorites: config.favorites.filter(f => f.id !== id) };
    await saveConfig(newConfig);
  }

  if (isConnected) {
    return (
      <ConnectedView 
        onDisconnect={handleDisconnect} 
        identity={activeIdentity!}
        onIdentityUpdated={(newIdent) => setActiveIdentity(newIdent)}
      />
    );
  }

  return (
    <div className="app-container">
      <div className="sidebar">
        <div className="sidebar-header">
          <h1>BlackTeaSpeak</h1>
          <p>Next-gen Voice & Chat</p>
        </div>
        <nav className="sidebar-nav">
          <button className={activeTab === 'favorites' ? 'active' : ''} onClick={() => setActiveTab('favorites')}>
            Favorites
          </button>
          <button className={activeTab === 'identities' ? 'active' : ''} onClick={() => setActiveTab('identities')}>
            Identities
          </button>
          <button className={activeTab === 'connect' ? 'active' : ''} onClick={() => setActiveTab('connect')}>
            Direct Connect
          </button>
        </nav>
        <div className="status-panel">
          <div className={`status-indicator ${isConnecting ? 'connecting' : (status.includes('Success') ? 'connected' : 'offline')}`}></div>
          <span className="status-text">{isConnecting ? 'Connecting...' : (status ? (status.includes('Success') ? 'Connected' : 'Error') : 'Offline')}</span>
        </div>
      </div>

      <div className="main-content">
        {status && <div className="global-status">{status}</div>}
        
        {activeTab === 'favorites' && (
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
        )}

        {activeTab === 'add_favorite' && (
          <div className="tab-pane slide-in">
            <h2>Add Favorite</h2>
            <form className="form-layout" onSubmit={handleAddFavorite}>
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
        )}

        {activeTab === 'identities' && (
          <div className="tab-pane slide-in">
            <h2>Identity Manager</h2>
            <div className="add-bar">
              <input 
                placeholder="New Identity Name" 
                value={newIdentityName} 
                onChange={e => setNewIdentityName(e.target.value)}
              />
              <button className="btn-primary" onClick={handleGenerateIdentity}>Generate</button>
            </div>
            
            <div className="list-view">
              {config.identities.length === 0 && <p className="empty-state">No identities. Generate one to store permissions on servers.</p>}
              {config.identities.map(ident => (
                <div className="list-item" key={ident.id}>
                  {editingIdentity?.id === ident.id ? (
                    <form className="edit-form" onSubmit={handleSaveIdentity} style={{ width: '100%', display: 'flex', gap: '10px', flexDirection: 'column' }}>
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
        )}

        {activeTab === 'connect' && (
          <div className="tab-pane slide-in">
            <h2>Direct Connect</h2>
            <form className="form-layout" onSubmit={(e) => { e.preventDefault(); handleConnect(address, nickname, selectedIdentity); }}>
              <div className="input-group">
                <label>Server Address</label>
                <input required value={address} onChange={(e) => setAddress(e.target.value)} placeholder="e.g. 127.0.0.1:9987" />
              </div>
              <div className="input-group">
                <label>Nickname</label>
                <input required value={nickname} onChange={(e) => setNickname(e.target.value)} />
              </div>
              <div className="input-group">
                <label>Identity</label>
                <select value={selectedIdentity} onChange={e => handleConnectIdentitySelect(e.target.value)}>
                  <option value="">No Identity</option>
                  {config.identities.map(i => <option key={i.id} value={i.id}>{i.name}</option>)}
                </select>
              </div>
              <button type="submit" className="btn-primary full-width" disabled={isConnecting}>
                {isConnecting ? "Connecting..." : "Connect"}
              </button>
            </form>
          </div>
        )}
      </div>
    </div>
  );
}

export default App;
