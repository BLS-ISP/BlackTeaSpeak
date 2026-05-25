import React from 'react';
import { TokenManagerModal } from './TokenManagerModal';
import { ChannelEditModal } from './ChannelEditModal';
import { BanManagerModal } from './BanManagerModal';
import { GroupManagerModal } from './GroupManagerModal';
import { SettingsModal } from './SettingsModal';
import { PermissionEditorModal } from './PermissionEditorModal';
import { Identity } from './App';

interface ModalsContainerProps {
  identity: Identity;
  onIdentityUpdated: (identity: Identity) => void;
  
  isTokenManagerOpen: boolean;
  setIsTokenManagerOpen: (v: boolean) => void;
  
  channelEditTarget: {cid?: string, cpid?: string} | null;
  setChannelEditTarget: (v: null) => void;
  
  isBanManagerOpen: boolean;
  setIsBanManagerOpen: (v: boolean) => void;
  
  isGroupManagerOpen: boolean;
  setIsGroupManagerOpen: (v: boolean) => void;
  
  isSettingsOpen: boolean;
  setIsSettingsOpen: (v: boolean) => void;
  
  permissionTarget: {type: 'servergroup' | 'channelgroup' | 'client' | 'channel', targetId: string} | null;
  setPermissionTarget: (v: null) => void;
}

export function ModalsContainer({
  identity, onIdentityUpdated,
  isTokenManagerOpen, setIsTokenManagerOpen,
  channelEditTarget, setChannelEditTarget,
  isBanManagerOpen, setIsBanManagerOpen,
  isGroupManagerOpen, setIsGroupManagerOpen,
  isSettingsOpen, setIsSettingsOpen,
  permissionTarget, setPermissionTarget
}: ModalsContainerProps) {
  return (
    <>
      {isTokenManagerOpen && <TokenManagerModal onClose={() => setIsTokenManagerOpen(false)} />}
      {channelEditTarget && <ChannelEditModal cid={channelEditTarget.cid} cpid={channelEditTarget.cpid} onClose={() => setChannelEditTarget(null)} />}
      {isBanManagerOpen && <BanManagerModal onClose={() => setIsBanManagerOpen(false)} />}
      {isGroupManagerOpen && <GroupManagerModal onClose={() => setIsGroupManagerOpen(false)} />}
      {isSettingsOpen && <SettingsModal onClose={() => setIsSettingsOpen(false)} identity={identity} onIdentityUpdated={onIdentityUpdated} />}
      {permissionTarget && <PermissionEditorModal targetType={permissionTarget.type} targetId={permissionTarget.targetId} onClose={() => setPermissionTarget(null)} />}
    </>
  );
}
