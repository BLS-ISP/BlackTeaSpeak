import { ServerGroup } from '../../types/protocol';
import { BlackTeaState } from '../state/blackTeaStateTypes';

export interface ClientCapabilities {
  isConnected: boolean;
  isAdmin: boolean;
  isModerator: boolean;
  roleLabel: string;
  selfGroups: ServerGroup[];
  canManagePermissions: boolean;
  canManageGroupsAndKeys: boolean;
  canModerate: boolean;
  canManageServer: boolean;
  canCreateChannel: boolean;
  canEditChannel: boolean;
  canDeleteChannel: boolean;
  canAssignGroups: boolean;
  canManageMusicBots: boolean;
}

export function getClientCapabilities(state: BlackTeaState): ClientCapabilities {
  const isConnected = state.connectionState === 'connected';
  const selfClient = state.clients.find((c) => c.clid === state.selfClientId);
  const groupIds = selfClient?.client_servergroups?.length
    ? selfClient.client_servergroups
    : [8];

  const selfGroups = groupIds
    .map((sgid) => state.serverGroups.find((g) => g.sgid === sgid))
    .filter((g): g is ServerGroup => Boolean(g));

  const permIdByName = new Map<string, number>();
  for (const entry of state.permissionCatalog) {
    if (entry.permsid && entry.permid > 0) {
      permIdByName.set(entry.permsid, entry.permid);
    }
  }
  const permVal = (name: string): number => {
    const id = permIdByName.get(name);
    if (!id) return 0;
    return state.selfNeededPermissions[id] ?? 0;
  };
  const hasPerm = (name: string, minVal = 1): boolean => permVal(name) >= minVal;

  const hasAdminGroup =
    groupIds.includes(6) ||
    groupIds.includes(9) ||
    selfGroups.some((g) => g.name.toLowerCase().includes('admin')) ||
    hasPerm('b_permission_modify_power_ignore') ||
    hasPerm('i_permission_modify_power', 70);

  const hasModeratorGroup =
    hasAdminGroup ||
    selfGroups.some((g) => {
      const lower = g.name.toLowerCase();
      return lower.includes('mod') || lower.includes('operator');
    }) ||
    hasPerm('i_client_kick_from_server_power', 50) ||
    hasPerm('i_client_ban_power', 50);

  const channelGroupId = selfClient?.client_channel_group_id ?? 8;
  const selfChannelGroup = state.channelGroups.find((cg) => cg.cgid === channelGroupId);
  const isChannelAdmin =
    channelGroupId === 5 ||
    Boolean(selfChannelGroup?.name.toLowerCase().includes('admin'));

  const isAdmin = isConnected && hasAdminGroup;
  const isModerator = isConnected && (hasModeratorGroup || isChannelAdmin);

  let roleLabel = 'Normal';
  if (hasAdminGroup) {
    roleLabel = 'Server Admin';
  } else if (hasModeratorGroup) {
    roleLabel = 'Moderator';
  } else if (isChannelAdmin) {
    roleLabel = 'Channel Admin';
  } else if (selfGroups.length > 0) {
    roleLabel = selfGroups[0].name;
  }

  return {
    isConnected,
    isAdmin,
    isModerator,
    roleLabel,
    selfGroups,
    canManagePermissions:
      isConnected &&
      (isAdmin || hasPerm('i_permission_modify_power', 1) || hasPerm('i_group_modify_power', 1)),
    canManageGroupsAndKeys:
      isConnected &&
      (isAdmin || hasPerm('b_virtualserver_token_add') || hasPerm('i_group_modify_power', 50)),
    canModerate:
      isConnected &&
      (isModerator ||
        hasPerm('i_client_kick_from_channel_power', 25) ||
        hasPerm('i_client_kick_from_server_power', 25) ||
        hasPerm('i_client_ban_power', 25)),
    canManageServer:
      isConnected && (isAdmin || hasPerm('b_virtualserver_modify_name')),
    canCreateChannel:
      isConnected &&
      (isModerator ||
        hasPerm('b_channel_create_permanent') ||
        hasPerm('b_channel_create_semi_permanent') ||
        hasPerm('b_channel_create_temporary')),
    canEditChannel:
      isConnected &&
      (isModerator ||
        hasPerm('b_channel_modify_name') ||
        hasPerm('i_channel_modify_power', 25)),
    canDeleteChannel:
      isConnected &&
      (isModerator ||
        hasPerm('b_channel_delete_permanent') ||
        hasPerm('b_channel_delete_temporary') ||
        hasPerm('i_channel_delete_power', 25)),
    canAssignGroups:
      isConnected &&
      (isAdmin || isChannelAdmin || hasPerm('i_group_member_add_power', 25)),
    canManageMusicBots:
      isConnected && (isModerator || hasPerm('i_channel_modify_power', 25)),
  };
}
