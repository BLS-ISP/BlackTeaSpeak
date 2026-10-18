import React, { useState } from 'react';
import { Globe, Layers, Mic, Server, Settings, X } from 'lucide-react';
import {
  getClientCapabilities,
  useBlackTeaStore,
} from '../../store/useBlackTeaStore';
import { AudioAndSpatialSettingsTab } from './server-admin/AudioAndSpatialSettingsTab';
import { VirtualServerSettingsTab } from './server-admin/VirtualServerSettingsTab';
import { VirtualServerInstancesTab } from './server-admin/VirtualServerInstancesTab';
import { TsdnsRoutingTab } from './server-admin/TsdnsRoutingTab';

export const ServerAdminModal: React.FC<{ onClose: () => void }> = ({ onClose }) => {
  const state = useBlackTeaStore();
  const caps = getClientCapabilities(state);
  const [tab, setTab] = useState<'audio' | 'server' | 'instances' | 'tsdns'>('audio');

  const activeTab = !caps.canManageServer && tab !== 'audio' ? 'audio' : tab;

  return (
    <div className={`modal-card ${caps.canManageServer ? 'wide' : ''}`}>
      <div className="modal-header">
        <div className="modal-title">
          <Settings size={18} />
          <span>
            {caps.canManageServer
              ? 'Audio-Optionen, Virtueller Server, Instanzen & TSDNS'
              : 'Audio- & Sprachübertragungs-Einstellungen'}
          </span>
        </div>
        <button className="btn-icon-sm" onClick={onClose}>
          <X size={16} />
        </button>
      </div>

      {caps.canManageServer && (
        <div className="modal-tabs">
          <button
            className={`modal-tab ${activeTab === 'audio' ? 'active' : ''}`}
            onClick={() => setTab('audio')}
          >
            <Mic size={14} /> Audio &amp; 3D HRTF
          </button>
          <button
            className={`modal-tab ${activeTab === 'server' ? 'active' : ''}`}
            onClick={() => setTab('server')}
          >
            <Server size={14} /> Virtueller Server
          </button>
          <button
            className={`modal-tab ${activeTab === 'instances' ? 'active' : ''}`}
            onClick={() => setTab('instances')}
          >
            <Layers size={14} /> Server-Instanzen ({state.virtualServers.length})
          </button>
          <button
            className={`modal-tab ${activeTab === 'tsdns' ? 'active' : ''}`}
            onClick={() => setTab('tsdns')}
          >
            <Globe size={14} /> TSDNS-Routing ({state.tsdnsRecords.length})
          </button>
        </div>
      )}

      <div className="modal-body">
        {activeTab === 'audio' && (
          <AudioAndSpatialSettingsTab state={state} caps={caps} />
        )}
        {activeTab === 'server' && caps.canManageServer && (
          <VirtualServerSettingsTab state={state} />
        )}
        {activeTab === 'instances' && caps.canManageServer && (
          <VirtualServerInstancesTab state={state} />
        )}
        {activeTab === 'tsdns' && caps.canManageServer && (
          <TsdnsRoutingTab state={state} />
        )}
      </div>
    </div>
  );
};
