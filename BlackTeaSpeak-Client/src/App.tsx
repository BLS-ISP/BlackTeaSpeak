import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import ConnectedView from "./ConnectedView";
import { Sidebar } from "./components/Sidebar";
import { FavoritesTab } from "./components/tabs/FavoritesTab";
import { AddFavoriteTab } from "./components/tabs/AddFavoriteTab";
import { IdentitiesTab } from "./components/tabs/IdentitiesTab";
import { ConnectTab } from "./components/tabs/ConnectTab";
import { AudioService } from "./services/AudioService";
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
  noise_suppression?: boolean;
  auto_gain_control?: boolean;
  echo_cancellation?: boolean;
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
  
  const [selectedIdentity, setSelectedIdentity] = useState<string>("");
  const [activeIdentity, setActiveIdentity] = useState<Identity | null>(null);
  const [newIdentityName, setNewIdentityName] = useState("");
  
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
    let actIdent = null;
    if (targetIdentityId) {
      const ident = config.identities.find(i => i.id === targetIdentityId);
      if (ident) {
        pubkey = ident.public_key;
        actIdent = ident;
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
      AudioService.playConnected();
      setIsConnected(true);
    } catch (error) {
      setStatus(`Error: ${error}`);
      setIsConnected(false);
    } finally {
      setIsConnecting(false);
    }
  }

  async function handleDisconnect() {
    try {
      await invoke("disconnect");
    } catch (e) {
      console.error(e);
    }
    AudioService.playDisconnected();
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

  async function handleAddFavoriteSubmit(fav: any) {
    const newFav: Favorite = {
      id: "fav_" + Date.now().toString(),
      name: fav.name,
      address: fav.address,
      password: fav.password,
      nickname: fav.nickname,
      identity_id: fav.identity_id
    };
    const newConfig = { ...config, favorites: [...config.favorites, newFav] };
    await saveConfig(newConfig);
    setActiveTab('favorites');
  }

  async function handleDeleteIdentity(id: string) {
    const newConfig = { ...config, identities: config.identities.filter(i => i.id !== id) };
    await saveConfig(newConfig);
  }

  async function handleSaveIdentity(identity: Identity) {
    const newConfig = {
      ...config,
      identities: config.identities.map(i => i.id === identity.id ? identity : i)
    };
    await saveConfig(newConfig);
  }

  async function handleDeleteFavorite(id: string) {
    const newConfig = { ...config, favorites: config.favorites.filter(f => f.id !== id) };
    await saveConfig(newConfig);
  }

  if (isConnected) {
    return (
      <ConnectedView 
        onDisconnect={handleDisconnect} 
        identity={activeIdentity || config.identities[0] || ({} as Identity)}
        onIdentityUpdated={(newIdent) => setActiveIdentity(newIdent)}
      />
    );
  }

  return (
    <div className="app-container">
      <Sidebar 
        activeTab={activeTab} 
        setActiveTab={setActiveTab} 
        isConnecting={isConnecting} 
        status={status} 
      />

      <div className="main-content">
        {status && <div className="global-status">{status}</div>}
        
        {activeTab === 'favorites' && (
          <FavoritesTab 
            config={config} 
            isConnecting={isConnecting} 
            setActiveTab={setActiveTab} 
            handleConnect={handleConnect} 
            handleDeleteFavorite={handleDeleteFavorite} 
          />
        )}

        {activeTab === 'add_favorite' && (
          <AddFavoriteTab 
            config={config} 
            setActiveTab={setActiveTab} 
            handleAddFavoriteSubmit={handleAddFavoriteSubmit} 
          />
        )}

        {activeTab === 'identities' && (
          <IdentitiesTab 
            config={config} 
            handleGenerateIdentity={handleGenerateIdentity} 
            handleDeleteIdentity={handleDeleteIdentity} 
            handleSaveIdentity={handleSaveIdentity} 
          />
        )}

        {activeTab === 'connect' && (
          <ConnectTab 
            config={config} 
            isConnecting={isConnecting} 
            handleConnect={handleConnect} 
          />
        )}
      </div>
    </div>
  );
}

export default App;
