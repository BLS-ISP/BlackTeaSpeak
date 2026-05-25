import React, { useState } from 'react';
import { AppConfig } from '../../App';

interface ConnectTabProps {
  config: AppConfig;
  isConnecting: boolean;
  handleConnect: (address: string, nickname: string, identityId: string) => void;
}

export function ConnectTab({ config, isConnecting, handleConnect }: ConnectTabProps) {
  const [address, setAddress] = useState("127.0.0.1:9987");
  const [nickname, setNickname] = useState("NewUser");
  const [selectedIdentity, setSelectedIdentity] = useState("");

  const handleConnectIdentitySelect = (id: string) => {
    setSelectedIdentity(id);
    const ident = config.identities.find(i => i.id === id);
    if (ident && ident.default_nickname) {
      setNickname(ident.default_nickname);
    }
  };

  const onSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    handleConnect(address, nickname, selectedIdentity);
  };

  return (
    <div className="tab-pane slide-in">
      <h2>Direct Connect</h2>
      <form className="form-layout" onSubmit={onSubmit}>
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
  );
}
