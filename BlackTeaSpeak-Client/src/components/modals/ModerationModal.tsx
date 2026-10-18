import React, { useState } from 'react';
import {
  AlertTriangle,
  Ban,
  Gavel,
  Mail,
  RefreshCw,
  X,
} from 'lucide-react';
import { blackTeaStore, useBlackTeaStore } from '../../store/useBlackTeaStore';
import { BanListTab } from './moderation/BanListTab';
import { ComplaintsTab } from './moderation/ComplaintsTab';
import { OfflineMessagesTab } from './moderation/OfflineMessagesTab';

export const ModerationModal: React.FC<{ onClose: () => void }> = ({ onClose }) => {
  const state = useBlackTeaStore();
  const [tab, setTab] = useState<'bans' | 'complaints' | 'messages'>('bans');

  return (
    <div className="modal-card wide">
      <div className="modal-header">
        <div className="modal-title">
          <Gavel size={18} />
          <span>Moderation: Bannliste, Beschwerden &amp; Offline-Nachrichten</span>
        </div>
        <div style={{ display: 'flex', gap: '6px' }}>
          <button
            className="btn-secondary-xs"
            onClick={() => void blackTeaStore.refreshModerationLists()}
            title="Listen vom Server neu laden"
          >
            <RefreshCw size={13} />
          </button>
          <button className="btn-icon-sm" onClick={onClose}>
            <X size={16} />
          </button>
        </div>
      </div>

      <div className="modal-tabs">
        <button
          className={`modal-tab ${tab === 'bans' ? 'active' : ''}`}
          onClick={() => setTab('bans')}
        >
          <Ban size={14} /> Bannliste ({state.bans.length})
        </button>
        <button
          className={`modal-tab ${tab === 'complaints' ? 'active' : ''}`}
          onClick={() => setTab('complaints')}
        >
          <AlertTriangle size={14} /> Beschwerden ({state.complaints.length})
        </button>
        <button
          className={`modal-tab ${tab === 'messages' ? 'active' : ''}`}
          onClick={() => setTab('messages')}
        >
          <Mail size={14} /> Offline-Nachrichten ({state.offlineMessages.length})
        </button>
      </div>

      <div className="modal-body">
        {tab === 'bans' && <BanListTab state={state} />}
        {tab === 'complaints' && <ComplaintsTab state={state} />}
        {tab === 'messages' && <OfflineMessagesTab state={state} />}
      </div>
    </div>
  );
};
