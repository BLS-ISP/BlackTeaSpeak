import React from 'react';
import { PermissionEditor } from './components/permissions/PermissionEditor';

export interface PermissionEditorModalProps {
  targetType: 'servergroup' | 'channelgroup' | 'client' | 'channel';
  targetId: string;
  onClose: () => void;
}

export function PermissionEditorModal({ targetType, targetId, onClose }: PermissionEditorModalProps) {
  return (
    <div className="modal-overlay" style={{ zIndex: 1000, display: 'flex', alignItems: 'center', justifyContent: 'center' }}>
      <div className="modal-content" style={{ width: '80vw', height: '80vh', display: 'flex', flexDirection: 'column' }}>
        <PermissionEditor targetType={targetType} targetId={targetId} />
        <div className="form-actions" style={{ marginTop: '16px' }}>
          <button className="btn-primary" onClick={onClose}>Close</button>
        </div>
      </div>
    </div>
  );
}
