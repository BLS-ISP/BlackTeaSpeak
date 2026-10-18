import React from 'react';
import { Key, X } from 'lucide-react';
import { useBlackTeaStore } from '../../store/useBlackTeaStore';
import { GroupManagementPane } from './groups-tokens/GroupManagementPane';
import { PrivilegeKeysPane } from './groups-tokens/PrivilegeKeysPane';

export const GroupsAndTokensModal: React.FC<{ onClose: () => void }> = ({
  onClose,
}) => {
  const state = useBlackTeaStore();

  return (
    <div className="modal-card wide">
      <div className="modal-header">
        <div className="modal-title">
          <Key size={18} />
          <span>Servergruppen, Channelgruppen &amp; Privilege Keys (Tokens)</span>
        </div>
        <button className="btn-icon-sm" onClick={onClose}>
          <X size={16} />
        </button>
      </div>

      <div className="modal-body split-two">
        <GroupManagementPane state={state} />
        <PrivilegeKeysPane state={state} />
      </div>
    </div>
  );
};
