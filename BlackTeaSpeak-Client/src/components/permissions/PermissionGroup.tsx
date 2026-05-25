import React from 'react';
import { PermGroup } from './PermissionEditor';
import { PermissionRow } from './PermissionRow';
import { Permission } from '../../types';

interface PermissionGroupProps {
  group: PermGroup;
  expanded: boolean;
  onToggle: () => void;
  searchQuery: string;
  assignedPerms: Record<string, Permission>;
  onUpdatePerm: (permname: string, value: string, skip: boolean, negated: boolean) => void;
}

export function PermissionGroup({ group, expanded, onToggle, searchQuery, assignedPerms, onUpdatePerm }: PermissionGroupProps) {
  const isExpanded = expanded || searchQuery.length > 0;
  
  return (
    <div className="tree-group">
      <div 
        className="tree-row group-row" 
        onClick={onToggle}
        style={{ padding: '8px', background: '#222', cursor: 'pointer', borderBottom: '1px solid #333', display: 'flex', alignItems: 'center' }}
      >
        <span style={{ 
          marginRight: '8px', 
          transform: isExpanded ? 'rotate(90deg)' : 'none', 
          display: 'inline-block', 
          transition: 'transform 0.1s' 
        }}>▶</span>
        <strong style={{ textTransform: 'capitalize' }}>{group.name}</strong>
      </div>
      
      {isExpanded && (
        <div className="tree-group-content">
          {group.permissions.map(p => (
            <PermissionRow 
              key={p.permname} 
              permission={p} 
              assigned={assignedPerms[p.permname]} 
              onUpdate={onUpdatePerm} 
            />
          ))}
        </div>
      )}
    </div>
  );
}
