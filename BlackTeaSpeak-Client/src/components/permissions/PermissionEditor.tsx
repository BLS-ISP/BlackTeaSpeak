import React, { useState, useEffect, useMemo } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Permission } from '../../types';
import { escapeTs3String } from '../../ts3parser';
import { eventBus } from '../../EventBus';
import { Toast } from '../../ui/Toast';
import { PermissionGroup } from './PermissionGroup';

interface PermissionEditorProps {
  targetType: 'servergroup' | 'channelgroup' | 'client' | 'channel';
  targetId: string;
}

export interface GlobalPermission {
  permid: string;
  permname: string;
  permdesc: string;
}

export interface PermGroup {
  name: string;
  permissions: GlobalPermission[];
  subGroups: Record<string, PermGroup>;
}

export function PermissionEditor({ targetType, targetId }: PermissionEditorProps) {
  const [globalCatalog, setGlobalCatalog] = useState<GlobalPermission[]>([]);
  const [assignedPerms, setAssignedPerms] = useState<Record<string, Permission>>({});
  const [searchQuery, setSearchQuery] = useState("");
  const [expandedGroups, setExpandedGroups] = useState<Set<string>>(new Set());

  useEffect(() => {
    const unsubscribe = eventBus.subscribe((rows) => {
      for (const row of rows) {
        // Handle global catalog
        if (row.command === 'permissionlist' || (row.command === 'unknown' && row.args.permdesc !== undefined)) {
          setGlobalCatalog(prev => {
            const existing = prev.findIndex(p => p.permid === row.args.permid);
            const newPerm = {
              permid: row.args.permid || '',
              permname: row.args.permname || `id_${row.args.permid}`,
              permdesc: row.args.permdesc?.replace(/\\s/g, ' ') || ''
            };
            if (existing !== -1) {
              const copy = [...prev];
              copy[existing] = newPerm;
              return copy;
            }
            return [...prev, newPerm];
          });
        }

        // Handle assigned permissions
        const isAssignedMatch = 
          (targetType === 'servergroup' && row.command === 'servergrouppermlist') ||
          (targetType === 'channelgroup' && row.command === 'channelgrouppermlist') ||
          (targetType === 'client' && row.command === 'clientpermlist') ||
          (targetType === 'channel' && row.command === 'channelpermlist') ||
          (row.command === 'unknown' && row.args.permvalue !== undefined);

        if (isAssignedMatch) {
          setAssignedPerms(prev => {
            const copy = { ...prev };
            const permname = row.args.permsid || row.args.permname || `id_${row.args.permid}`;
            copy[permname] = {
              permid: row.args.permid || '',
              permname: permname,
              permvalue: row.args.permvalue || '0',
              permskip: row.args.permskip === '1',
              permnegated: row.args.permnegated === '1'
            };
            return copy;
          });
        }
      }
    });

    // Request global catalog
    invoke('send_command', { command: 'permissionlist' });

    // Request assigned perms
    if (targetType === 'servergroup') invoke('send_command', { command: `servergrouppermlist sgid=${targetId}` });
    if (targetType === 'channelgroup') invoke('send_command', { command: `channelgrouppermlist cgid=${targetId}` });
    if (targetType === 'client') invoke('send_command', { command: `clientpermlist cldbid=${targetId}` });
    if (targetType === 'channel') invoke('send_command', { command: `channelpermlist cid=${targetId}` });

    return () => {
      unsubscribe();
    };
  }, [targetType, targetId]);

  const tree = useMemo(() => {
    const root: PermGroup = { name: "Root", permissions: [], subGroups: {} };
    
    // Filter by search
    const filtered = globalCatalog.filter(p => 
      p.permname.toLowerCase().includes(searchQuery.toLowerCase()) || 
      p.permdesc.toLowerCase().includes(searchQuery.toLowerCase())
    );

    for (const p of filtered) {
      const parts = p.permname.split('_');
      // e.g. b_virtualserver_modify_name -> [b, virtualserver, modify, name]
      // We will group by the 2nd part (e.g. virtualserver, channel, client)
      const groupName = parts.length > 1 ? parts[1] : "General";
      
      if (!root.subGroups[groupName]) {
        root.subGroups[groupName] = { name: groupName, permissions: [], subGroups: {} };
      }
      root.subGroups[groupName].permissions.push(p);
    }
    return root;
  }, [globalCatalog, searchQuery]);

  const toggleGroup = (groupName: string) => {
    const next = new Set(expandedGroups);
    if (next.has(groupName)) next.delete(groupName);
    else next.add(groupName);
    setExpandedGroups(next);
  };

  const handleUpdatePerm = (permname: string, value: string, skip: boolean, negated: boolean) => {
    const escName = escapeTs3String(permname);
    
    if (value === "" || (value === "0" && !skip && !negated)) {
      // Delete permission if it's practically empty
      if (targetType === 'servergroup') invoke('send_command', { command: `servergroupdelperm sgid=${targetId} permsid=${escName}` });
      if (targetType === 'channelgroup') invoke('send_command', { command: `channelgroupdelperm cgid=${targetId} permsid=${escName}` });
      if (targetType === 'client') invoke('send_command', { command: `clientdelperm cldbid=${targetId} permsid=${escName}` });
      if (targetType === 'channel') invoke('send_command', { command: `channeldelperm cid=${targetId} permsid=${escName}` });
      
      setAssignedPerms(prev => {
        const copy = { ...prev };
        delete copy[permname];
        return copy;
      });
      Toast.success(`Removed ${permname}`);
    } else {
      // Add or Edit permission
      const v = value || "0";
      const s = skip ? 1 : 0;
      const n = negated ? 1 : 0;
      
      if (targetType === 'servergroup') invoke('send_command', { command: `servergroupaddperm sgid=${targetId} permsid=${escName} permvalue=${v} permnegated=${n} permskip=${s}` });
      if (targetType === 'channelgroup') invoke('send_command', { command: `channelgroupaddperm cgid=${targetId} permsid=${escName} permvalue=${v} permnegated=${n} permskip=${s}` });
      if (targetType === 'client') invoke('send_command', { command: `clientaddperm cldbid=${targetId} permsid=${escName} permvalue=${v} permnegated=${n} permskip=${s}` });
      if (targetType === 'channel') invoke('send_command', { command: `channeladdperm cid=${targetId} permsid=${escName} permvalue=${v}` });
      
      setAssignedPerms(prev => ({
        ...prev,
        [permname]: { permid: "", permname, permvalue: v, permskip: skip, permnegated: negated }
      }));
      Toast.success(`Saved ${permname}`);
    }
  };

  return (
    <div className="permission-editor-embedded" style={{ flexGrow: 1, display: 'flex', flexDirection: 'column', height: '100%' }}>
      <div className="info-header" style={{ marginBottom: '12px' }}>
        <h3 style={{ margin: 0, marginBottom: '8px' }}>Permissions - {targetType} ({targetId})</h3>
        <input 
          type="text" 
          placeholder="Search permissions..." 
          value={searchQuery}
          onChange={e => setSearchQuery(e.target.value)}
          style={{ width: '100%', padding: '8px', background: '#111', border: '1px solid #333', color: '#fff', borderRadius: '4px' }}
        />
      </div>

      <div className="tree-table-header" style={{ display: 'flex', borderBottom: '2px solid #444', padding: '8px', fontWeight: 'bold', fontSize: '12px', color: '#aaa' }}>
        <div style={{ flex: 1 }}>Permission Name</div>
        <div style={{ width: '100px', textAlign: 'center' }}>Value</div>
        <div style={{ width: '60px', textAlign: 'center' }}>Skip</div>
        <div style={{ width: '60px', textAlign: 'center' }}>Negate</div>
      </div>

      <div className="tree-view" style={{ flexGrow: 1, overflowY: 'auto', background: '#1a1a1a' }}>
        {Object.values(tree.subGroups).map(group => (
          <PermissionGroup 
            key={group.name} 
            group={group} 
            expanded={expandedGroups.has(group.name)} 
            onToggle={() => toggleGroup(group.name)} 
            searchQuery={searchQuery} 
            assignedPerms={assignedPerms} 
            onUpdatePerm={handleUpdatePerm} 
          />
        ))}
        {globalCatalog.length === 0 && <div style={{ padding: '20px', textAlign: 'center', color: '#888' }}>Loading Permission Catalog...</div>}
      </div>
    </div>
  );
}
