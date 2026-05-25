import React from 'react';
import { GlobalPermission } from './PermissionEditor';
import { Permission } from '../../types';

interface PermissionRowProps {
  permission: GlobalPermission;
  assigned?: Permission;
  onUpdate: (permname: string, value: string, skip: boolean, negated: boolean) => void;
}

export function PermissionRow({ permission, assigned, onUpdate }: PermissionRowProps) {
  const isBoolean = permission.permname.startsWith('b_');
  
  const value = assigned ? assigned.permvalue : (isBoolean ? "0" : "");
  const skip = assigned ? (assigned.permskip || false) : false;
  const negated = assigned ? (assigned.permnegated || false) : false;

  return (
    <div className="tree-row perm-row" style={{ display: 'flex', borderBottom: '1px solid #333', padding: '6px 8px', alignItems: 'center' }}>
      <div style={{ flex: 1, paddingLeft: '24px' }}>
        <div style={{ fontWeight: assigned ? 'bold' : 'normal', color: assigned ? '#fff' : '#aaa' }}>{permission.permname}</div>
        <div style={{ fontSize: '11px', color: '#888' }}>{permission.permdesc}</div>
      </div>
      <div style={{ width: '100px', display: 'flex', justifyContent: 'center' }}>
        {isBoolean ? (
          <input 
            type="checkbox" 
            checked={value === "1"} 
            onChange={(e) => onUpdate(permission.permname, e.target.checked ? "1" : "0", skip, negated)} 
          />
        ) : (
          <input 
            type="text" 
            value={value} 
            onChange={(e) => onUpdate(permission.permname, e.target.value, skip, negated)} 
            style={{ width: '60px', background: '#222', border: '1px solid #444', color: '#fff', padding: '2px 4px' }} 
            placeholder="Value" 
          />
        )}
      </div>
      <div style={{ width: '60px', display: 'flex', justifyContent: 'center' }}>
        <input 
          type="checkbox" 
          checked={skip} 
          onChange={(e) => onUpdate(permission.permname, value, e.target.checked, negated)} 
        />
      </div>
      <div style={{ width: '60px', display: 'flex', justifyContent: 'center' }}>
        <input 
          type="checkbox" 
          checked={negated} 
          onChange={(e) => onUpdate(permission.permname, value, skip, e.target.checked)} 
        />
      </div>
    </div>
  );
}
