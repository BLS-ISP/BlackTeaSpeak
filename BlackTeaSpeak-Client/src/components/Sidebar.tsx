import React from 'react';

interface SidebarProps {
  activeTab: string;
  setActiveTab: (tab: string) => void;
  isConnecting: boolean;
  status: string;
}

export function Sidebar({ activeTab, setActiveTab, isConnecting, status }: SidebarProps) {
  return (
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
  );
}
