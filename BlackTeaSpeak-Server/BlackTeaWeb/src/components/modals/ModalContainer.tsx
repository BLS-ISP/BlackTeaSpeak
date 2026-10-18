import React from 'react';
import { blackTeaStore, useBlackTeaStore } from '../../store/useBlackTeaStore';
import { PermissionsModal } from './PermissionsModal';
import { GroupsAndTokensModal } from './GroupsAndTokensModal';
import { ModerationModal } from './ModerationModal';
import { FileBrowserModal } from './FileBrowserModal';
import { ServerAdminModal } from './ServerAdminModal';
import { ChannelDialogModal } from './ChannelDialogModal';
import { RedeemKeyModal } from './RedeemKeyModal';

export const ModalContainer: React.FC = () => {
  const state = useBlackTeaStore();
  if (!state.activeModal) return null;

  const close = () => blackTeaStore.patchState({ activeModal: null });

  return (
    <div className="modal-backdrop" onClick={close}>
      <div onClick={(e) => e.stopPropagation()}>
        {state.activeModal === 'redeem_key' && <RedeemKeyModal onClose={close} />}
        {state.activeModal === 'permissions' && <PermissionsModal onClose={close} />}
        {state.activeModal === 'groups_tokens' && <GroupsAndTokensModal onClose={close} />}
        {state.activeModal === 'moderation' && <ModerationModal onClose={close} />}
        {state.activeModal === 'files' && <FileBrowserModal onClose={close} />}
        {state.activeModal === 'server_admin' && <ServerAdminModal onClose={close} />}
        {state.activeModal === 'channel_create' && (
          <ChannelDialogModal mode="create" onClose={close} />
        )}
        {state.activeModal === 'channel_edit' && (
          <ChannelDialogModal mode="edit" onClose={close} />
        )}
      </div>
    </div>
  );
};
