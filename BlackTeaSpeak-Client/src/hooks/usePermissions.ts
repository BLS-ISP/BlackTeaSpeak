import { useState, useCallback, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { parseTs3Response } from '../ts3parser';
import { Permission } from '../types';

export function usePermissions(currentChannelId?: string) {
  const [permissions, setPermissions] = useState<Record<string, number>>({});

  const fetchPermissions = useCallback(async () => {
    try {
      const response = await invoke<string>('send_command', { command: 'desktopgetmyperms' });
      const parsedRows = parseTs3Response(response);
      
      const newPerms: Record<string, number> = {};
      for (const row of parsedRows) {
        if (row.command === 'desktopgetmyperms' || !row.command.startsWith('error')) {
          if (row.args.permname && row.args.permvalue) {
            newPerms[row.args.permname] = parseInt(row.args.permvalue, 10);
          }
        }
      }
      setPermissions(newPerms);
    } catch (e) {
      console.error("Failed to fetch permissions", e);
    }
  }, []);

  useEffect(() => {
    // Fetch permissions on mount, and whenever the channel changes
    fetchPermissions();
    
    // Also re-fetch if we receive a notification that our groups changed
    const unlistenGroup = listen<string>('notifyclientupdated', (event) => {
      // clientupdate might mean our servergroups changed
      fetchPermissions();
    });
    
    return () => {
      unlistenGroup.then(u => u());
    };
  }, [currentChannelId, fetchPermissions]);

  const hasPermission = useCallback((permName: string, requiredPower: number = 1) => {
    // If the server grants b_client_ignore_permissions, we have all boolean permissions
    if (permissions['b_client_ignore_permissions'] === 1 && permName.startsWith('b_')) {
      return true;
    }
    const val = permissions[permName];
    if (val === undefined) return false; // Permission not set
    return val >= requiredPower;
  }, [permissions]);

  return { permissions, hasPermission, fetchPermissions };
}
