use super::*;
use std::fs;

use crate::models::PermissionGroupSpec;

pub(super) const ERROR_INSUFFICIENT_PERMISSIONS: u32 = 0xA08;

const TEAWEB_PERMISSION_GROUP_ENDS: &[u32] = &[
    0, 7, 13, 18, 21, 21, 34, 48, 82, 82, 89, 113, 133, 140, 157, 157, 173, 175, 199, 201, 201,
    275, 303, 323, 342, 360,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct PermissionActorContext {
    pub(super) server_id: u32,
    pub(super) channel_id: u32,
    pub(super) client_database_id: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PermissionAssignment {
    pub(crate) value: i64,
    pub(crate) negated: bool,
    pub(crate) skipped: bool,
}

#[derive(Debug, Clone)]
pub(super) struct ParsedPermissionAssignment {
    pub(super) name: String,
    pub(super) assignment: PermissionAssignment,
}

#[derive(Debug, Clone)]
struct WebPermissionLayout {
    ids_by_name: BTreeMap<String, u32>,
    group_markers: Vec<u32>,
}

pub(super) fn session_has_permission_actor(session: &QuerySessionState) -> bool {
    session.authenticated_login.is_some() || session.actor_client_database_id_override.is_some()
}

pub(super) fn build_permission_map(
    group: Option<&PermissionGroupSpec>,
) -> BTreeMap<String, PermissionAssignment> {
    group
        .map(|group| {
            group
                .permissions
                .iter()
                .map(|permission| {
                    (
                        permission.name.clone(),
                        PermissionAssignment {
                            value: permission.value,
                            negated: permission.negated != 0,
                            skipped: permission.skipped != 0,
                        },
                    )
                })
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default()
}

pub(super) fn build_named_permission_map(
    specs: &FoundationSpecs,
    group_name: &str,
    target_name: &str,
) -> BTreeMap<String, PermissionAssignment> {
    build_permission_map(
        specs
            .permission_groups
            .iter()
            .find(|group| group.name == group_name && group.target_name == target_name),
    )
}

pub(super) fn build_permission_catalog(
    specs: &FoundationSpecs,
) -> BTreeMap<String, PermissionCatalogEntry> {
    specs
        .permission_catalog
        .iter()
        .map(|entry| (entry.name.clone(), entry.clone()))
        .collect()
}

pub(super) fn load_blackteaweb_permission_ids(workspace_root: &Path) -> BTreeMap<String, u32> {
    let path = workspace_root
        .join("BlackTeaWeb")
        .join("shared")
        .join("js")
        .join("permission")
        .join("PermissionType.ts");
    parse_blackteaweb_permission_ids(&path).unwrap_or_default()
}

fn parse_blackteaweb_permission_ids(path: &Path) -> Option<BTreeMap<String, u32>> {
    let content = fs::read_to_string(path).ok()?;
    let mut ids_by_name = BTreeMap::new();

    for line in content.lines() {
        let Some(id_start) = line.find("Permission ID:") else {
            continue;
        };
        let segments = line.split('"').collect::<Vec<_>>();
        if segments.len() < 3 {
            continue;
        }

        let permission_name = segments[1].trim();
        if permission_name.is_empty() {
            continue;
        }

        let Some(id_text) = line[id_start + "Permission ID:".len()..]
            .split("*/")
            .next()
            .map(str::trim)
        else {
            continue;
        };
        let Some(permission_id) = id_text.parse::<u32>().ok() else {
            continue;
        };
        ids_by_name.insert(permission_name.to_string(), permission_id);
    }

    Some(ids_by_name)
}

fn build_web_group_row(
    id_key: &str,
    id: u32,
    name: &str,
    group_type: u32,
    icon_id: i64,
    save_db: bool,
    required_member_add_power: i64,
    required_member_remove_power: i64,
    required_modify_power: i64,
) -> BTreeMap<String, String> {
    let mut row = BTreeMap::new();
    row.insert(String::from(id_key), id.to_string());
    row.insert(String::from("name"), String::from(name));
    row.insert(String::from("type"), group_type.to_string());
    row.insert(String::from("iconid"), icon_id.to_string());
    row.insert(
        String::from("savedb"),
        if save_db {
            String::from("1")
        } else {
            String::from("0")
        },
    );
    row.insert(String::from("sortid"), String::from("0"));
    row.insert(String::from("namemode"), String::from("0"));
    row.insert(
        String::from("n_member_addp"),
        required_member_add_power.to_string(),
    );
    row.insert(
        String::from("n_member_removep"),
        required_member_remove_power.to_string(),
    );
    row.insert(String::from("n_modifyp"), required_modify_power.to_string());
    row
}

pub(super) fn permission_value_or_default(
    permissions: &BTreeMap<String, PermissionAssignment>,
    candidate_names: &[&str],
) -> i64 {
    candidate_names
        .iter()
        .find_map(|permission_name| {
            permissions
                .get(*permission_name)
                .map(|assignment| assignment.value)
        })
        .unwrap_or(0)
}

fn permission_power_max_or_default(
    permissions: &BTreeMap<String, PermissionAssignment>,
    candidate_names: &[&str],
) -> i64 {
    candidate_names
        .iter()
        .filter_map(|permission_name| {
            permissions
                .get(*permission_name)
                .map(|assignment| assignment.value)
        })
        .max()
        .unwrap_or(0)
}

pub(super) fn check_server_group_membership_change(
    actor_permissions: &BTreeMap<String, PermissionAssignment>,
    target_permissions: &BTreeMap<String, PermissionAssignment>,
    self_target: bool,
    add_group: bool,
) -> std::result::Result<(), &'static str> {
    let (actor_power_names, needed_power_names, failed_permission_name) =
        match (add_group, self_target) {
            (true, true) => (
                &["i_server_group_self_add_power", "i_group_member_add_power"][..],
                &[
                    "i_server_group_needed_member_add_power",
                    "i_group_needed_member_add_power",
                ][..],
                "i_server_group_self_add_power",
            ),
            (true, false) => (
                &[
                    "i_server_group_member_add_power",
                    "i_group_member_add_power",
                ][..],
                &[
                    "i_server_group_needed_member_add_power",
                    "i_group_needed_member_add_power",
                ][..],
                "i_group_member_add_power",
            ),
            (false, true) => (
                &[
                    "i_server_group_self_remove_power",
                    "i_group_member_remove_power",
                ][..],
                &[
                    "i_server_group_needed_member_remove_power",
                    "i_group_needed_member_remove_power",
                ][..],
                "i_server_group_self_remove_power",
            ),
            (false, false) => (
                &[
                    "i_server_group_member_remove_power",
                    "i_group_member_remove_power",
                ][..],
                &[
                    "i_server_group_needed_member_remove_power",
                    "i_group_needed_member_remove_power",
                ][..],
                "i_group_member_remove_power",
            ),
        };

    if permission_power_max_or_default(actor_permissions, actor_power_names)
        < permission_power_max_or_default(target_permissions, needed_power_names)
    {
        return Err(failed_permission_name);
    }

    Ok(())
}

pub(super) fn check_permission_edit_allowed(
    actor_permissions: &BTreeMap<String, PermissionAssignment>,
    target_permissions: &BTreeMap<String, PermissionAssignment>,
    target_needed_modify_names: &[&str],
) -> std::result::Result<(), &'static str> {
    check_group_modify_allowed(
        actor_permissions,
        target_permissions,
        target_needed_modify_names,
    )?;

    if permission_power_max_or_default(actor_permissions, &["b_permission_modify_power_ignore"]) > 0
    {
        return Ok(());
    }

    if permission_power_max_or_default(
        actor_permissions,
        &[
            "i_permission_modify_power",
            "i_channel_permission_modify_power",
            "i_client_permission_modify_power",
        ],
    ) < 1
    {
        return Err("i_permission_modify_power");
    }

    Ok(())
}

pub(super) fn check_group_modify_allowed(
    actor_permissions: &BTreeMap<String, PermissionAssignment>,
    target_permissions: &BTreeMap<String, PermissionAssignment>,
    target_needed_modify_names: &[&str],
) -> std::result::Result<(), &'static str> {
    if permission_power_max_or_default(
        actor_permissions,
        &[
            "i_group_modify_power",
            "i_server_group_modify_power",
            "i_channel_group_modify_power",
        ],
    ) < permission_power_max_or_default(target_permissions, target_needed_modify_names)
    {
        return Err("i_group_modify_power");
    }

    Ok(())
}

pub(super) fn check_required_permission(
    actor_permissions: &BTreeMap<String, PermissionAssignment>,
    candidate_names: &[&str],
    failed_permission_name: &'static str,
) -> std::result::Result<(), &'static str> {
    if permission_power_max_or_default(actor_permissions, candidate_names) < 1 {
        return Err(failed_permission_name);
    }

    Ok(())
}

pub(super) fn check_channel_group_membership_change(
    actor_permissions: &BTreeMap<String, PermissionAssignment>,
    target_permissions: &BTreeMap<String, PermissionAssignment>,
    add_group: bool,
) -> std::result::Result<(), &'static str> {
    let (actor_power_names, needed_power_names, failed_permission_name) = if add_group {
        (
            &["i_group_member_add_power"][..],
            &[
                "i_channel_group_needed_member_add_power",
                "i_group_needed_member_add_power",
            ][..],
            "i_group_member_add_power",
        )
    } else {
        (
            &["i_group_member_remove_power"][..],
            &[
                "i_channel_group_needed_member_remove_power",
                "i_group_needed_member_remove_power",
            ][..],
            "i_group_member_remove_power",
        )
    };

    if permission_power_max_or_default(actor_permissions, actor_power_names)
        < permission_power_max_or_default(target_permissions, needed_power_names)
    {
        return Err(failed_permission_name);
    }

    Ok(())
}

pub(super) fn check_channel_modify_power_allowed(
    actor_permissions: &BTreeMap<String, PermissionAssignment>,
    target_permissions: &BTreeMap<String, PermissionAssignment>,
) -> std::result::Result<(), &'static str> {
    if permission_power_max_or_default(actor_permissions, &["i_channel_modify_power"])
        < permission_power_max_or_default(target_permissions, &["i_channel_needed_modify_power"])
    {
        return Err("i_channel_modify_power");
    }

    Ok(())
}

pub(super) fn check_channel_delete_power_allowed(
    actor_permissions: &BTreeMap<String, PermissionAssignment>,
    target_permissions: &BTreeMap<String, PermissionAssignment>,
) -> std::result::Result<(), &'static str> {
    if permission_power_max_or_default(actor_permissions, &["i_channel_delete_power"])
        < permission_power_max_or_default(target_permissions, &["i_channel_needed_delete_power"])
    {
        return Err("i_channel_delete_power");
    }

    Ok(())
}

fn check_playlist_power_allowed(
    actor_permissions: &BTreeMap<String, PermissionAssignment>,
    target_permissions: &BTreeMap<String, PermissionAssignment>,
    actor_power_names: &[&str],
    needed_power_names: &[&str],
    failed_permission_name: &'static str,
) -> std::result::Result<(), &'static str> {
    if permission_power_max_or_default(actor_permissions, actor_power_names)
        < permission_power_max_or_default(target_permissions, needed_power_names)
    {
        return Err(failed_permission_name);
    }

    Ok(())
}



impl BaselineRuntime {
    pub fn web_permission_rows(&self) -> Vec<BTreeMap<String, String>> {
        let layout = self.build_web_permission_layout();
        let mut permission_names = layout.ids_by_name.keys().cloned().collect::<Vec<_>>();
        permission_names.sort_by(|left_name, right_name| {
            layout
                .ids_by_name
                .get(left_name)
                .copied()
                .unwrap_or(u32::MAX)
                .cmp(
                    &layout
                        .ids_by_name
                        .get(right_name)
                        .copied()
                        .unwrap_or(u32::MAX),
                )
                .then_with(|| left_name.cmp(right_name))
        });

        let mut rows = permission_names
            .into_iter()
            .map(|permission_name| {
                let mut row = BTreeMap::new();
                row.insert(
                    String::from("permid"),
                    layout
                        .ids_by_name
                        .get(&permission_name)
                        .copied()
                        .unwrap_or_else(|| self.permission_id_for_name(&permission_name))
                        .to_string(),
                );
                row.insert(String::from("permname"), permission_name.clone());
                row.insert(
                    String::from("permdesc"),
                    self.permission_description_for_name(&permission_name),
                );
                row
            })
            .collect::<Vec<_>>();

        for group_id_end in layout.group_markers {
            let mut group_row = BTreeMap::new();
            group_row.insert(String::from("group_id_end"), group_id_end.to_string());
            rows.push(group_row);
        }

        rows
    }

    pub fn web_server_group_rows(&self) -> Vec<BTreeMap<String, String>> {
        self.store
            .server_groups
            .values()
            .map(|group| {
                build_web_group_row(
                    "sgid",
                    group.id,
                    &group.name,
                    group.group_type,
                    group.icon_id,
                    group.save_db,
                    permission_value_or_default(
                        &group.permissions,
                        &[
                            "i_server_group_needed_member_add_power",
                            "i_group_needed_member_add_power",
                        ],
                    ),
                    permission_value_or_default(
                        &group.permissions,
                        &[
                            "i_server_group_needed_member_remove_power",
                            "i_group_needed_member_remove_power",
                        ],
                    ),
                    permission_value_or_default(
                        &group.permissions,
                        &[
                            "i_server_group_needed_modify_power",
                            "i_group_needed_modify_power",
                        ],
                    ),
                )
            })
            .collect()
    }

    pub fn web_channel_group_rows(&self) -> Vec<BTreeMap<String, String>> {
        self.store
            .channel_groups
            .values()
            .map(|group| {
                build_web_group_row(
                    "cgid",
                    group.id,
                    &group.name,
                    group.group_type,
                    group.icon_id,
                    group.save_db,
                    permission_value_or_default(
                        &group.permissions,
                        &[
                            "i_channel_group_needed_member_add_power",
                            "i_group_needed_member_add_power",
                        ],
                    ),
                    permission_value_or_default(
                        &group.permissions,
                        &[
                            "i_channel_group_needed_member_remove_power",
                            "i_group_needed_member_remove_power",
                        ],
                    ),
                    permission_value_or_default(
                        &group.permissions,
                        &[
                            "i_channel_group_needed_modify_power",
                            "i_group_needed_modify_power",
                        ],
                    ),
                )
            })
            .collect()
    }

    pub fn web_client_needed_permission_rows(
        &self,
        server_id: u32,
        channel_id: u32,
        client_database_id: u64,
    ) -> Option<Vec<BTreeMap<String, String>>> {
        let effective_permissions =
            self.effective_permissions_for_client(server_id, channel_id, client_database_id)?;
        Some(self.build_web_needed_permission_rows(&effective_permissions))
    }

    pub fn web_server_group_permission_rows(
        &self,
        group_id: u32,
    ) -> Option<Vec<BTreeMap<String, String>>> {
        let group = self.store.server_groups.get(&group_id)?;
        Some(self.build_web_permission_assignment_rows(
            &group.permissions,
            &[("sgid", group_id.to_string())],
        ))
    }

    pub fn web_channel_group_permission_rows(
        &self,
        group_id: u32,
    ) -> Option<Vec<BTreeMap<String, String>>> {
        let group = self.store.channel_groups.get(&group_id)?;
        Some(self.build_web_permission_assignment_rows(
            &group.permissions,
            &[("cgid", group_id.to_string())],
        ))
    }

    pub fn web_channel_permission_rows(
        &self,
        server_id: u32,
        channel_id: u32,
    ) -> Option<Vec<BTreeMap<String, String>>> {
        let channel = self.channel_by_id(server_id, channel_id)?;
        Some(self.build_web_permission_assignment_rows(
            &channel.permissions,
            &[("cid", channel_id.to_string())],
        ))
    }

    fn client_permission_target(&self, server_id: u32, client_database_id: u64) -> Option<&ClientPermissionTarget> {
        self.store
            .client_permissions
            .iter()
            .find(|target| target.server_id == server_id && target.client_database_id == client_database_id)
    }

    fn ensure_client_permission_target_mut(
        &mut self,
        server_id: u32,
        client_database_id: u64,
    ) -> &mut ClientPermissionTarget {
        let resolved_identity = self.lookup_client_identity_by_dbid(server_id, client_database_id);
        let target_index = if let Some(index) = self
            .store
            .client_permissions
            .iter()
            .position(|target| target.client_database_id == client_database_id)
        {
            index
        } else {
            self.store.client_permissions.push(ClientPermissionTarget {
                server_id,
                client_database_id,
                client_unique_identifier: String::new(),
                client_nickname: String::new(),
                permissions: BTreeMap::new(),
            });
            self.store.client_permissions.len() - 1
        };

        let target = &mut self.store.client_permissions[target_index];
        if let Some((client_unique_identifier, _, client_nickname)) = resolved_identity {
            if target.client_unique_identifier.is_empty() {
                target.client_unique_identifier = client_unique_identifier;
            }
            if target.client_nickname.is_empty() {
                target.client_nickname = client_nickname;
            }
        }
        target
    }

    pub fn web_client_permission_rows(
        &self,
        server_id: u32,
        client_database_id: u64,
    ) -> Option<Vec<BTreeMap<String, String>>> {
        if !self.client_database_id_exists(client_database_id) {
            return None;
        }

        Some(
            self.query_account_by_cldbid(client_database_id)
                .map(|account| {
                    self.build_web_permission_assignment_rows(
                        &account.permissions,
                        &[("cldbid", client_database_id.to_string())],
                    )
                })
                .or_else(|| {
                    self.client_permission_target(server_id, client_database_id)
                        .map(|target| {
                            self.build_web_permission_assignment_rows(
                                &target.permissions,
                                &[("cldbid", client_database_id.to_string())],
                            )
                        })
                })
                .unwrap_or_else(|| {
                    let mut row = BTreeMap::new();
                    row.insert(String::from("cldbid"), client_database_id.to_string());
                    vec![row]
                }),
        )
    }

    pub fn web_channel_client_permission_rows(
        &self,
        server_id: u32,
        channel_id: u32,
        client_database_id: u64,
    ) -> Option<Vec<BTreeMap<String, String>>> {
        if self.channel_by_id(server_id, channel_id).is_none()
            || !self.client_database_id_exists(client_database_id)
        {
            return None;
        }

        Some(
            self.store
                .channel_client_permissions
                .iter()
                .find(|target| {
                    target.channel_id == channel_id
                        && target.client_database_id == client_database_id
                })
                .map(|target| {
                    self.build_web_permission_assignment_rows(
                        &target.permissions,
                        &[
                            ("cid", channel_id.to_string()),
                            ("cldbid", client_database_id.to_string()),
                        ],
                    )
                })
                .unwrap_or_else(|| {
                    let mut row = BTreeMap::new();
                    row.insert(String::from("cid"), channel_id.to_string());
                    row.insert(String::from("cldbid"), client_database_id.to_string());
                    vec![row]
                }),
        )
    }

    pub fn web_playlist_permission_rows(
        &self,
        server_id: u32,
        playlist_id: u32,
    ) -> Option<Vec<BTreeMap<String, String>>> {
        let bot_id = self.music_bot_id_by_playlist_id(server_id, playlist_id)?;
        let bot = self.store.music_bots.get(&bot_id)?;
        Some(self.build_web_permission_assignment_rows(
            &bot.permissions,
            &[("playlist_id", playlist_id.to_string())],
        ))
    }

    pub fn web_playlist_client_permission_rows(
        &self,
        server_id: u32,
        playlist_id: u32,
        client_database_id: u64,
    ) -> Option<Vec<BTreeMap<String, String>>> {
        if !self.client_database_id_exists(client_database_id) {
            return None;
        }

        let bot_id = self.music_bot_id_by_playlist_id(server_id, playlist_id)?;
        let bot = self.store.music_bots.get(&bot_id)?;
        Some(
            bot.client_permissions
                .iter()
                .find(|target| target.client_database_id == client_database_id)
                .map(|target| {
                    self.build_web_permission_assignment_rows(
                        &target.permissions,
                        &[
                            ("playlist_id", playlist_id.to_string()),
                            ("cldbid", client_database_id.to_string()),
                        ],
                    )
                })
                .unwrap_or_else(|| {
                    let mut row = BTreeMap::new();
                    row.insert(String::from("playlist_id"), playlist_id.to_string());
                    row.insert(String::from("cldbid"), client_database_id.to_string());
                    vec![row]
                }),
        )
    }

    pub fn web_playlist_client_rows(
        &self,
        server_id: u32,
        playlist_id: u32,
    ) -> Option<Vec<BTreeMap<String, String>>> {
        let bot_id = self.music_bot_id_by_playlist_id(server_id, playlist_id)?;
        let bot = self.store.music_bots.get(&bot_id)?;
        Some(
            if bot.client_permissions.is_empty() {
                let mut row = BTreeMap::new();
                row.insert(String::from("playlist_id"), playlist_id.to_string());
                vec![row]
            } else {
                bot.client_permissions
                    .iter()
                    .map(|target| {
                        let mut row = BTreeMap::new();
                        row.insert(String::from("playlist_id"), playlist_id.to_string());
                        row.insert(String::from("cldbid"), target.client_database_id.to_string());
                        row
                    })
                    .collect()
            },
        )
    }

    pub fn web_server_group_client_rows(
        &self,
        group_id: u32,
    ) -> Option<Vec<BTreeMap<String, String>>> {
        if !self.store.server_groups.contains_key(&group_id) {
            return None;
        }

        let mut rows_by_client_database_id = BTreeMap::new();
        for account in self
            .store
            .query_accounts
            .values()
            .filter(|account| account.server_groups.contains(&group_id))
        {
            let Some(client_database_id) = account.client_database_id else {
                continue;
            };

            let mut row = BTreeMap::new();
            row.insert(String::from("sgid"), group_id.to_string());
            row.insert(String::from("cldbid"), client_database_id.to_string());
            row.insert(String::from("client_nickname"), account.login_name.clone());
            row.insert(
                String::from("client_unique_identifier"),
                format!("query-account-{}", client_database_id),
            );
            rows_by_client_database_id.insert(client_database_id, row);
        }

        for client in self
            .store
            .online_clients
            .values()
            .filter(|client| client.server_groups.contains(&group_id))
        {
            let mut row = BTreeMap::new();
            row.insert(String::from("sgid"), group_id.to_string());
            row.insert(String::from("cldbid"), client.database_id.to_string());
            row.insert(String::from("client_nickname"), client.nickname.clone());
            row.insert(
                String::from("client_unique_identifier"),
                client.unique_identifier.clone(),
            );
            rows_by_client_database_id.insert(client.database_id, row);
        }

        if rows_by_client_database_id.is_empty() {
            let mut row = BTreeMap::new();
            row.insert(String::from("sgid"), group_id.to_string());
            return Some(vec![row]);
        }

        Some(rows_by_client_database_id.into_values().collect())
    }

    pub fn web_server_groups_by_client_rows(
        &self,
        server_id: u32,
        client_database_id: u64,
    ) -> Option<Vec<BTreeMap<String, String>>> {
        let group_ids = if let Some(account) = self.query_account_by_cldbid(client_database_id) {
            account
                .server_groups
                .iter()
                .copied()
                .collect::<BTreeSet<_>>()
        } else if let Some(client) = self.online_client_by_cldbid(server_id, client_database_id) {
            client
                .server_groups
                .iter()
                .copied()
                .collect::<BTreeSet<_>>()
        } else {
            return None;
        };

        Some(
            group_ids
                .into_iter()
                .filter_map(|group_id| {
                    self.store.server_groups.get(&group_id).map(|group| {
                        let mut row = BTreeMap::new();
                        row.insert(String::from("name"), group.name.clone());
                        row.insert(String::from("sgid"), group.id.to_string());
                        row.insert(String::from("cldbid"), client_database_id.to_string());
                        row
                    })
                })
                .collect(),
        )
    }

    pub fn web_add_server_group_client(
        &mut self,
        server_id: u32,
        actor_channel_id: u32,
        actor_client_database_id: u64,
        group_id: u32,
        client_database_id: u64,
    ) -> Result<(), WebServerGroupMutationError> {
        self.web_change_server_group_client(
            server_id,
            actor_channel_id,
            actor_client_database_id,
            group_id,
            client_database_id,
            true,
        )
    }

    pub fn web_del_server_group_client(
        &mut self,
        server_id: u32,
        actor_channel_id: u32,
        actor_client_database_id: u64,
        group_id: u32,
        client_database_id: u64,
    ) -> Result<(), WebServerGroupMutationError> {
        self.web_change_server_group_client(
            server_id,
            actor_channel_id,
            actor_client_database_id,
            group_id,
            client_database_id,
            false,
        )
    }

    fn web_change_server_group_client(
        &mut self,
        server_id: u32,
        actor_channel_id: u32,
        actor_client_database_id: u64,
        group_id: u32,
        client_database_id: u64,
        add_group: bool,
    ) -> Result<(), WebServerGroupMutationError> {
        let Some(target_group_permissions) = self
            .store
            .server_groups
            .get(&group_id)
            .map(|group| group.permissions.clone())
        else {
            return Err(WebServerGroupMutationError::InvalidGroup);
        };

        let Some(actor_permissions) = self.effective_permissions_for_client(
            server_id,
            actor_channel_id,
            actor_client_database_id,
        ) else {
            return Err(WebServerGroupMutationError::InvalidClient);
        };

        if let Err(permission_name) = check_server_group_membership_change(
            &actor_permissions,
            &target_group_permissions,
            actor_client_database_id == client_database_id,
            add_group,
        ) {
            return Err(WebServerGroupMutationError::PermissionDenied {
                failed_permission_id: self.permission_id_for_name(permission_name),
            });
        }

        if add_group {
            if let Some(account) = self.query_account_by_cldbid_mut(client_database_id) {
                if !account.server_groups.contains(&group_id) {
                    account.server_groups.push(group_id);
                    account.server_groups.sort_unstable();
                }
                return Ok(());
            }

            let Some(client) = self.online_client_by_cldbid_mut(server_id, client_database_id)
            else {
                return Err(WebServerGroupMutationError::InvalidClient);
            };
            if !client.server_groups.contains(&group_id) {
                client.server_groups.push(group_id);
                client.server_groups.sort_unstable();
            }
            return Ok(());
        }

        let has_admin_group = self.store.server_groups.contains_key(&6);
        let has_guest_group = self.store.server_groups.contains_key(&7);

        if let Some(account) = self.query_account_by_cldbid_mut(client_database_id) {
            account
                .server_groups
                .retain(|existing_group_id| *existing_group_id != group_id);
            if account.server_groups.is_empty() {
                account.server_groups = default_server_groups_for_login_with_availability(
                    &account.login_name,
                    has_admin_group,
                    has_guest_group,
                );
            }
            account.server_groups.sort_unstable();
            account.server_groups.dedup();
            return Ok(());
        }

        let Some(client) = self.online_client_by_cldbid_mut(server_id, client_database_id) else {
            return Err(WebServerGroupMutationError::InvalidClient);
        };
        client
            .server_groups
            .retain(|existing_group_id| *existing_group_id != group_id);
        client.server_groups.sort_unstable();
        client.server_groups.dedup();
        Ok(())
    }

    pub(super) fn ensure_web_server_group_assignment_permission_basis(&mut self) {
        let group_id = if self.store.server_groups.contains_key(&8) {
            Some(8)
        } else {
            self.store
                .server_groups
                .iter()
                .find(|(_, group)| group.name == "Normal")
                .map(|(group_id, _)| *group_id)
        };
        let Some(group_id) = group_id else {
            return;
        };
        let Some(group) = self.store.server_groups.get_mut(&group_id) else {
            return;
        };

        for permission_name in [
            "i_server_group_member_add_power",
            "i_server_group_member_remove_power",
        ] {
            group
                .permissions
                .entry(permission_name.to_string())
                .or_insert(PermissionAssignment {
                    value: 100,
                    negated: false,
                    skipped: false,
                });
        }
    }

    pub(super) fn effective_channel_group(
        &self,
        channel_id: u32,
        client_database_id: u64,
    ) -> Option<&ChannelGroup> {
        if let Some(group_id) = self
            .store
            .channel_group_assignments
            .iter()
            .find(|assignment| {
                assignment.channel_id == channel_id
                    && assignment.client_database_id == client_database_id
            })
            .map(|assignment| assignment.channel_group_id)
        {
            return self.store.channel_groups.get(&group_id);
        }

        self.default_channel_group_id()
            .and_then(|group_id| self.store.channel_groups.get(&group_id))
    }

    fn default_channel_group_id(&self) -> Option<u32> {
        self.store
            .channel_groups
            .values()
            .filter_map(|group| {
                let auto_update_type = group.permissions.get("i_group_auto_update_type")?.value;
                Some((group.id, auto_update_type))
            })
            .find(|(_, auto_update_type)| *auto_update_type == 10)
            .map(|(group_id, _)| group_id)
            .or_else(|| {
                self.store
                    .channel_groups
                    .values()
                    .find(|group| group.name == "Guest")
                    .map(|group| group.id)
            })
            .or_else(|| self.store.channel_groups.keys().copied().next())
    }

    pub(super) fn query_permission_actor_context(
        &self,
        session: &QuerySessionState,
    ) -> std::result::Result<PermissionActorContext, QueryResponse> {
        if let Some(client_database_id) = session.actor_client_database_id_override {
            let server_id = session
                .selected_virtual_server_id
                .or_else(|| {
                    self.store
                        .online_clients
                        .values()
                        .find(|client| client.database_id == client_database_id)
                        .map(|client| client.server_id)
                })
                .or_else(|| self.store.virtual_servers.keys().next().copied())
                .unwrap_or(1);
            let channel_id = session
                .current_channel_id
                .filter(|channel_id| self.channel_exists(server_id, *channel_id))
                .or_else(|| {
                    self.online_client_by_cldbid(server_id, client_database_id)
                        .map(|client| client.channel_id)
                })
                .or_else(|| self.default_channel_id_for_server(server_id))
                .unwrap_or(1);

            return Ok(PermissionActorContext {
                server_id,
                channel_id,
                client_database_id,
            });
        }

        let Some(login_name) = session.authenticated_login.as_ref() else {
            return Err(QueryResponse::error(521, "login required"));
        };
        let Some(account) = self.store.query_accounts.get(login_name) else {
            return Err(QueryResponse::error(768, "query account not found"));
        };
        let Some(client_database_id) = account.client_database_id else {
            return Err(QueryResponse::error(768, "query account not found"));
        };

        let server_id = session
            .selected_virtual_server_id
            .or(account.server_id)
            .or_else(|| self.store.virtual_servers.keys().next().copied())
            .unwrap_or(1);
        let channel_id = session
            .current_channel_id
            .filter(|channel_id| self.channel_exists(server_id, *channel_id))
            .or_else(|| self.default_channel_id_for_server(server_id))
            .unwrap_or(1);

        Ok(PermissionActorContext {
            server_id,
            channel_id,
            client_database_id,
        })
    }

    pub(super) fn query_actor_effective_permissions(
        &self,
        session: &QuerySessionState,
    ) -> std::result::Result<
        (
            PermissionActorContext,
            BTreeMap<String, PermissionAssignment>,
        ),
        QueryResponse,
    > {
        let actor = self.query_permission_actor_context(session)?;
        let Some(actor_permissions) = self.effective_permissions_for_client(
            actor.server_id,
            actor.channel_id,
            actor.client_database_id,
        ) else {
            return Err(QueryResponse::error(768, "query account not found"));
        };

        Ok((actor, actor_permissions))
    }

    pub(super) fn insufficient_permission_response(&self, permission_name: &str) -> QueryResponse {
        QueryResponse::error_with_fields(
            ERROR_INSUFFICIENT_PERMISSIONS,
            "insufficient client permissions",
            [(
                "failed_permid",
                self.permission_id_for_name(permission_name).to_string(),
            )],
        )
    }

    pub(super) fn ensure_playlist_view_allowed(
        &self,
        actor_permissions: &BTreeMap<String, PermissionAssignment>,
        target_permissions: &BTreeMap<String, PermissionAssignment>,
    ) -> std::result::Result<(), QueryResponse> {
        check_playlist_power_allowed(
            actor_permissions,
            target_permissions,
            &["i_playlist_view_power", "i_client_music_play_power"],
            &["i_playlist_needed_view_power"],
            "i_playlist_view_power",
        )
        .map_err(|permission_name| self.insufficient_permission_response(permission_name))
    }

    pub(super) fn ensure_playlist_modify_allowed(
        &self,
        actor_permissions: &BTreeMap<String, PermissionAssignment>,
        target_permissions: &BTreeMap<String, PermissionAssignment>,
    ) -> std::result::Result<(), QueryResponse> {
        check_playlist_power_allowed(
            actor_permissions,
            target_permissions,
            &["i_playlist_modify_power", "i_client_music_play_power"],
            &["i_playlist_needed_modify_power"],
            "i_playlist_modify_power",
        )
        .map_err(|permission_name| self.insufficient_permission_response(permission_name))
    }

    pub(super) fn ensure_playlist_permission_modify_allowed(
        &self,
        actor_permissions: &BTreeMap<String, PermissionAssignment>,
        target_permissions: &BTreeMap<String, PermissionAssignment>,
    ) -> std::result::Result<(), QueryResponse> {
        if permission_power_max_or_default(actor_permissions, &["b_permission_modify_power_ignore"])
            > 0
        {
            return Ok(());
        }

        check_playlist_power_allowed(
            actor_permissions,
            target_permissions,
            &[
                "i_playlist_permission_modify_power",
                "i_permission_modify_power",
                "i_client_music_play_power",
            ],
            &["i_playlist_needed_permission_modify_power"],
            "i_playlist_permission_modify_power",
        )
        .map_err(|permission_name| self.insufficient_permission_response(permission_name))
    }

    pub(super) fn ensure_playlist_song_add_allowed(
        &self,
        actor_permissions: &BTreeMap<String, PermissionAssignment>,
        target_permissions: &BTreeMap<String, PermissionAssignment>,
    ) -> std::result::Result<(), QueryResponse> {
        check_playlist_power_allowed(
            actor_permissions,
            target_permissions,
            &["i_playlist_song_add_power", "i_client_music_play_power"],
            &["i_playlist_song_needed_add_power"],
            "i_playlist_song_add_power",
        )
        .map_err(|permission_name| self.insufficient_permission_response(permission_name))
    }

    pub(super) fn ensure_playlist_song_move_allowed(
        &self,
        actor_permissions: &BTreeMap<String, PermissionAssignment>,
        target_permissions: &BTreeMap<String, PermissionAssignment>,
    ) -> std::result::Result<(), QueryResponse> {
        check_playlist_power_allowed(
            actor_permissions,
            target_permissions,
            &["i_playlist_song_move_power", "i_client_music_play_power"],
            &["i_playlist_song_needed_move_power"],
            "i_playlist_song_move_power",
        )
        .map_err(|permission_name| self.insufficient_permission_response(permission_name))
    }

    pub(super) fn ensure_playlist_song_remove_allowed(
        &self,
        actor_permissions: &BTreeMap<String, PermissionAssignment>,
        target_permissions: &BTreeMap<String, PermissionAssignment>,
    ) -> std::result::Result<(), QueryResponse> {
        check_playlist_power_allowed(
            actor_permissions,
            target_permissions,
            &["i_playlist_song_remove_power", "i_client_music_play_power"],
            &["i_playlist_song_needed_remove_power"],
            "i_playlist_song_remove_power",
        )
        .map_err(|permission_name| self.insufficient_permission_response(permission_name))
    }

    pub(super) fn ensure_playlist_permission_list_allowed(
        &self,
        actor_permissions: &BTreeMap<String, PermissionAssignment>,
        target_permissions: &BTreeMap<String, PermissionAssignment>,
    ) -> std::result::Result<(), QueryResponse> {
        if permission_power_max_or_default(
            actor_permissions,
            &["b_virtualserver_playlist_permission_list"],
        ) > 0
        {
            return Ok(());
        }

        self.ensure_playlist_view_allowed(actor_permissions, target_permissions)
    }

    pub(super) fn parse_permission_assignments(
        &self,
        request: &CommandRequest,
        global_keys: &[&str],
    ) -> std::result::Result<Vec<ParsedPermissionAssignment>, QueryResponse> {
        let groups = self.permission_option_groups(request, global_keys);
        let mut parsed_assignments = Vec::new();

        for group in groups {
            let Some(permission_name) = self.resolve_permission_name_from_args(&group) else {
                return Err(QueryResponse::error(512, "permid or permsid is required"));
            };
            let Some(value) = group
                .get("permvalue")
                .and_then(|value| value.parse::<i64>().ok())
            else {
                return Err(QueryResponse::error(512, "permvalue is required"));
            };
            let negated = group
                .get("permnegated")
                .and_then(|value| parse_query_bool(value))
                .unwrap_or(false);
            let skipped = group
                .get("permskip")
                .and_then(|value| parse_query_bool(value))
                .unwrap_or(false);
            parsed_assignments.push(ParsedPermissionAssignment {
                name: permission_name,
                assignment: PermissionAssignment {
                    value,
                    negated,
                    skipped,
                },
            });
        }

        if parsed_assignments.is_empty() {
            return Err(QueryResponse::error(
                512,
                "no permission assignments provided",
            ));
        }

        Ok(parsed_assignments)
    }

    pub(super) fn parse_requested_permission_names(
        &self,
        request: &CommandRequest,
        global_keys: &[&str],
    ) -> std::result::Result<Vec<String>, QueryResponse> {
        let groups = self.permission_option_groups(request, global_keys);
        let mut permission_names = Vec::new();

        for group in groups {
            let Some(permission_name) = self.resolve_permission_name_from_args(&group) else {
                return Err(QueryResponse::error(512, "permid or permsid is required"));
            };
            permission_names.push(permission_name);
        }

        if permission_names.is_empty() {
            return Err(QueryResponse::error(512, "no permissions requested"));
        }

        Ok(permission_names)
    }

    fn resolve_permission_name_from_args(&self, args: &BTreeMap<String, String>) -> Option<String> {
        if let Some(permission_name) = args.get("permsid") {
            return Some(permission_name.clone());
        }

        args.get("permid")
            .and_then(|value| value.parse::<u32>().ok())
            .and_then(|permission_id| self.permission_name_for_id(permission_id))
    }

    pub(super) fn permission_id_for_name(&self, permission_name: &str) -> u32 {
        self.permission_catalog
            .get(permission_name)
            .map(|entry| entry.id)
            .unwrap_or_else(|| synthetic_permission_id(permission_name))
    }

    pub(super) fn permission_description_for_name(&self, permission_name: &str) -> String {
        self.permission_catalog
            .get(permission_name)
            .map(|entry| entry.description.clone())
            .unwrap_or_else(|| describe_permission_name(permission_name))
    }

    fn build_web_permission_layout(&self) -> WebPermissionLayout {
        let known_permission_names = self.all_known_permission_names();
        if self.web_permission_base_ids.is_empty() {
            let ids_by_name = known_permission_names
                .into_iter()
                .map(|permission_name| {
                    let permission_id = self.permission_id_for_name(&permission_name);
                    (permission_name, permission_id)
                })
                .collect::<BTreeMap<_, _>>();
            let group_markers = ids_by_name
                .values()
                .copied()
                .max()
                .map(|max_id| vec![max_id])
                .unwrap_or_default();
            return WebPermissionLayout {
                ids_by_name,
                group_markers,
            };
        }

        let mut ids_by_name = known_permission_names
            .iter()
            .filter_map(|permission_name| {
                self.web_permission_base_ids
                    .get(permission_name)
                    .copied()
                    .map(|permission_id| (permission_name.clone(), permission_id))
            })
            .collect::<BTreeMap<_, _>>();

        let mut next_id = self
            .web_permission_base_ids
            .values()
            .copied()
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        let mut fallback_permissions = known_permission_names
            .into_iter()
            .filter(|permission_name| !ids_by_name.contains_key(permission_name))
            .collect::<Vec<_>>();
        fallback_permissions.sort_by(|left_name, right_name| {
            self.permission_id_for_name(left_name)
                .cmp(&self.permission_id_for_name(right_name))
                .then_with(|| left_name.cmp(right_name))
        });

        for permission_name in fallback_permissions {
            ids_by_name.insert(permission_name, next_id);
            next_id = next_id.saturating_add(1);
        }

        let max_id = ids_by_name.values().copied().max().unwrap_or(0);
        let mut group_markers = TEAWEB_PERMISSION_GROUP_ENDS
            .iter()
            .copied()
            .filter(|group_id_end| *group_id_end <= max_id)
            .collect::<Vec<_>>();
        if max_id > 0 && group_markers.last().copied() != Some(max_id) {
            group_markers.push(max_id);
        }

        WebPermissionLayout {
            ids_by_name,
            group_markers,
        }
    }

    pub(super) fn permission_name_for_id(&self, permission_id: u32) -> Option<String> {
        self.permission_catalog
            .iter()
            .find_map(|(permission_name, entry)| {
                (entry.id == permission_id).then(|| permission_name.clone())
            })
            .or_else(|| {
                self.store
                    .server_groups
                    .values()
                    .flat_map(|group| group.permissions.keys())
                    .chain(self.store.channels.values().flat_map(|channels| {
                        channels
                            .iter()
                            .flat_map(|channel| channel.permissions.keys())
                    }))
                    .chain(
                        self.store
                            .channel_groups
                            .values()
                            .flat_map(|group| group.permissions.keys()),
                    )
                    .chain(
                        self.store
                            .channel_client_permissions
                            .iter()
                            .flat_map(|target| target.permissions.keys()),
                    )
                    .chain(
                        self.store
                            .query_accounts
                            .values()
                            .flat_map(|account| account.permissions.keys()),
                    )
                    .chain(
                        self.store
                            .music_bots
                            .values()
                            .flat_map(|bot| bot.permissions.keys()),
                    )
                    .chain(
                        self.store.music_bots.values().flat_map(|bot| {
                            bot.client_permissions
                                .iter()
                                .flat_map(|target| target.permissions.keys())
                        }),
                    )
                    .find(|permission_name| {
                        synthetic_permission_id(permission_name) == permission_id
                    })
                    .cloned()
            })
    }

    pub(super) fn all_known_permission_names(&self) -> Vec<String> {
        let mut permission_names = self
            .permission_catalog
            .keys()
            .cloned()
            .collect::<BTreeSet<_>>();
        permission_names.extend(
            self.store
                .server_groups
                .values()
                .flat_map(|group| group.permissions.keys().cloned()),
        );
        permission_names.extend(
            self.store
                .channel_groups
                .values()
                .flat_map(|group| group.permissions.keys().cloned()),
        );
        permission_names.extend(self.store.channels.values().flat_map(|channels| {
            channels
                .iter()
                .flat_map(|channel| channel.permissions.keys().cloned())
        }));
        permission_names.extend(
            self.store
                .channel_client_permissions
                .iter()
                .flat_map(|target| target.permissions.keys().cloned()),
        );
        permission_names.extend(
            self.store
                .query_accounts
                .values()
                .flat_map(|account| account.permissions.keys().cloned()),
        );
        permission_names.extend(
            self.store
                .music_bots
                .values()
                .flat_map(|bot| bot.permissions.keys().cloned()),
        );
        permission_names.extend(self.store.music_bots.values().flat_map(|bot| {
            bot.client_permissions
                .iter()
                .flat_map(|target| target.permissions.keys().cloned())
        }));
        permission_names.into_iter().collect()
    }

    pub(super) fn knows_permission_name(&self, permission_name: &str) -> bool {
        self.permission_catalog.contains_key(permission_name)
            || self
                .store
                .server_groups
                .values()
                .any(|group| group.permissions.contains_key(permission_name))
            || self
                .store
                .channel_groups
                .values()
                .any(|group| group.permissions.contains_key(permission_name))
            || self.store.channels.values().any(|channels| {
                channels
                    .iter()
                    .any(|channel| channel.permissions.contains_key(permission_name))
            })
            || self
                .store
                .channel_client_permissions
                .iter()
                .any(|target| target.permissions.contains_key(permission_name))
            || self
                .store
                .query_accounts
                .values()
                .any(|account| account.permissions.contains_key(permission_name))
            || self
                .store
                .music_bots
                .values()
                .any(|bot| bot.permissions.contains_key(permission_name))
            || self.store.music_bots.values().any(|bot| {
                bot.client_permissions
                    .iter()
                    .any(|target| target.permissions.contains_key(permission_name))
            })
    }

    pub(super) fn effective_permissions_for_account(
        &self,
        account: &QueryAccount,
    ) -> BTreeMap<String, PermissionAssignment> {
        let mut effective_permissions = BTreeMap::<String, PermissionAssignment>::new();

        for group_id in &account.server_groups {
            if let Some(group) = self.store.server_groups.get(group_id) {
                for (permission_name, assignment) in &group.permissions {
                    let should_replace = match effective_permissions.get(permission_name) {
                        Some(current_assignment) => assignment.value >= current_assignment.value,
                        None => true,
                    };
                    if should_replace {
                        effective_permissions.insert(permission_name.clone(), assignment.clone());
                    }
                }
            }
        }

        for (permission_name, assignment) in &account.permissions {
            effective_permissions.insert(permission_name.clone(), assignment.clone());
        }

        effective_permissions
    }

    pub(super) fn effective_permissions_for_client(
        &self,
        server_id: u32,
        channel_id: u32,
        client_database_id: u64,
    ) -> Option<BTreeMap<String, PermissionAssignment>> {
        let resolved_channel_id = if self.query_account_by_cldbid(client_database_id).is_some()
        {
            self.online_client_by_cldbid(server_id, client_database_id)
                .map(|client| client.channel_id)
                .unwrap_or(channel_id)
        } else if let Some(client) = self.online_client_by_cldbid(server_id, client_database_id) {
            client.channel_id
        } else if self.client_permission_target(server_id, client_database_id).is_some() {
            channel_id
        } else {
            return None;
        };

        self.effective_permissions_for_client_in_channel_context(
            server_id,
            resolved_channel_id,
            client_database_id,
        )
    }

    pub(super) fn effective_permissions_for_client_in_channel_context(
        &self,
        server_id: u32,
        channel_id: u32,
        client_database_id: u64,
    ) -> Option<BTreeMap<String, PermissionAssignment>> {
        let mut effective_permissions = BTreeMap::<String, PermissionAssignment>::new();

        // 1. Server Groups (Tier 1)
        let mut server_group_perms = Vec::new();
        if let Some(account) = self.query_account_by_cldbid(client_database_id) {
            for group_id in &account.server_groups {
                if let Some(group) = self.store.server_groups.get(group_id) {
                    server_group_perms.push(&group.permissions);
                }
            }
        } else if let Some(client) = self.online_client_by_cldbid(server_id, client_database_id) {
            for group_id in &client.server_groups {
                if let Some(group) = self.store.server_groups.get(group_id) {
                    server_group_perms.push(&group.permissions);
                }
            }
        }

        // Merge Server Groups
        let mut combined_sg_perms: BTreeMap<String, Vec<&PermissionAssignment>> = BTreeMap::new();
        for group_perms in server_group_perms {
            for (name, assignment) in group_perms {
                combined_sg_perms.entry(name.clone()).or_default().push(assignment);
            }
        }
        for (name, assignments) in combined_sg_perms {
            let is_negated = assignments.iter().any(|a| a.negated);
            let is_skipped = assignments.iter().any(|a| a.skipped);
            let value = if is_negated {
                assignments.iter().map(|a| a.value).min().unwrap()
            } else {
                assignments.iter().map(|a| a.value).max().unwrap()
            };
            effective_permissions.insert(name, PermissionAssignment {
                value,
                negated: is_negated,
                skipped: is_skipped,
            });
        }

        // 2. Client Permissions (Tier 2)
        if let Some(account) = self.query_account_by_cldbid(client_database_id) {
            for (permission_name, assignment) in &account.permissions {
                effective_permissions.insert(permission_name.clone(), assignment.clone());
            }
        } else if let Some(target) = self.client_permission_target(server_id, client_database_id) {
            for (permission_name, assignment) in &target.permissions {
                effective_permissions.insert(permission_name.clone(), assignment.clone());
            }
        }

        // Check b_client_skip_channelgroup_permissions
        let global_skip = effective_permissions
            .get("b_client_skip_channelgroup_permissions")
            .map(|a| a.value != 0)
            .unwrap_or(false);

        if !global_skip {
            // 3. Channel Permissions (Tier 3)
            if let Some(channel) = self.channel_by_id(server_id, channel_id) {
                for (permission_name, assignment) in &channel.permissions {
                    if let Some(existing) = effective_permissions.get(permission_name) {
                        if existing.skipped {
                            continue; // Tier 1/2 skip flag prevents override
                        }
                    }
                    effective_permissions.insert(permission_name.clone(), assignment.clone());
                }
            }

            // 4. Channel Groups (Tier 4)
            if let Some(group) = self.effective_channel_group(channel_id, client_database_id) {
                for (permission_name, assignment) in &group.permissions {
                    if let Some(existing) = effective_permissions.get(permission_name) {
                        if existing.skipped {
                            continue; // Tier 1/2 skip flag prevents override
                        }
                    }
                    effective_permissions.insert(permission_name.clone(), assignment.clone());
                }
            }

            // 5. Channel Client Permissions (Tier 5)
            if let Some(target) = self.store.channel_client_permissions.iter().find(|target| {
                target.channel_id == channel_id && target.client_database_id == client_database_id
            }) {
                for (permission_name, assignment) in &target.permissions {
                    if let Some(existing) = effective_permissions.get(permission_name) {
                        if existing.skipped {
                            continue; // Tier 1/2 skip flag prevents override
                        }
                    }
                    effective_permissions.insert(permission_name.clone(), assignment.clone());
                }
            }
        }

        Some(effective_permissions)
    }

    pub fn web_client_can_view_channel(
        &self,
        server_id: u32,
        channel_id: u32,
        client_database_id: u64,
    ) -> bool {
        let Some(effective_permissions) = self.effective_permissions_for_client_in_channel_context(
            server_id,
            channel_id,
            client_database_id,
        ) else {
            return false;
        };

        let has_explicit_info_view = effective_permissions.contains_key("b_channel_info_view");
        let has_explicit_view_power = effective_permissions.contains_key("b_channel_ignore_view_power")
            || effective_permissions.contains_key("i_channel_view_power")
            || effective_permissions.contains_key("i_channel_needed_view_power");

        if !has_explicit_info_view && !has_explicit_view_power {
            return true;
        }

        if has_explicit_info_view
            && permission_value_or_default(&effective_permissions, &["b_channel_info_view"]) < 1
        {
            return false;
        }

        if permission_value_or_default(&effective_permissions, &["b_channel_ignore_view_power"]) > 0 {
            return true;
        }

        permission_power_max_or_default(&effective_permissions, &["i_channel_view_power"])
            >= permission_value_or_default(&effective_permissions, &["i_channel_needed_view_power"])
    }

    fn build_web_needed_permission_rows(
        &self,
        permissions: &BTreeMap<String, PermissionAssignment>,
    ) -> Vec<BTreeMap<String, String>> {
        let layout = self.build_web_permission_layout();
        let mut sorted_permissions = permissions.iter().collect::<Vec<_>>();
        sorted_permissions.sort_by(|(left_name, _), (right_name, _)| {
            layout
                .ids_by_name
                .get(*left_name)
                .copied()
                .unwrap_or_else(|| self.permission_id_for_name(left_name))
                .cmp(
                    &layout
                        .ids_by_name
                        .get(*right_name)
                        .copied()
                        .unwrap_or_else(|| self.permission_id_for_name(right_name)),
                )
                .then_with(|| left_name.cmp(right_name))
        });

        let mut rows = sorted_permissions
            .into_iter()
            .map(|(permission_name, assignment)| {
                self.render_permission_row_with_id(
                    layout
                        .ids_by_name
                        .get(permission_name)
                        .copied()
                        .unwrap_or_else(|| self.permission_id_for_name(permission_name)),
                    permission_name,
                    assignment,
                    false,
                )
            })
            .collect::<Vec<_>>();

        if let Some(first_row) = rows.first_mut() {
            first_row.insert(String::from("relative"), String::from("0"));
        } else {
            let mut row = BTreeMap::new();
            row.insert(String::from("relative"), String::from("0"));
            rows.push(row);
        }

        rows
    }

    pub(super) fn render_permission_row(
        &self,
        permission_name: &str,
        assignment: &PermissionAssignment,
        include_permission_name: bool,
    ) -> BTreeMap<String, String> {
        self.render_permission_row_with_id(
            self.permission_id_for_name(permission_name),
            permission_name,
            assignment,
            include_permission_name,
        )
    }

    fn render_permission_row_with_id(
        &self,
        permission_id: u32,
        permission_name: &str,
        assignment: &PermissionAssignment,
        include_permission_name: bool,
    ) -> BTreeMap<String, String> {
        let mut row = BTreeMap::new();
        row.insert(String::from("permid"), permission_id.to_string());
        row.insert(String::from("permvalue"), assignment.value.to_string());
        row.insert(
            String::from("permnegated"),
            if assignment.negated {
                String::from("1")
            } else {
                String::from("0")
            },
        );
        row.insert(
            String::from("permskip"),
            if assignment.skipped {
                String::from("1")
            } else {
                String::from("0")
            },
        );
        if include_permission_name {
            row.insert(String::from("permsid"), permission_name.to_string());
        }
        row
    }

    fn build_web_permission_assignment_rows(
        &self,
        permissions: &BTreeMap<String, PermissionAssignment>,
        subject_fields: &[(&str, String)],
    ) -> Vec<BTreeMap<String, String>> {
        let layout = self.build_web_permission_layout();
        let mut sorted_permissions = permissions.iter().collect::<Vec<_>>();
        sorted_permissions.sort_by(|(left_name, _), (right_name, _)| {
            layout
                .ids_by_name
                .get(*left_name)
                .copied()
                .unwrap_or_else(|| self.permission_id_for_name(left_name))
                .cmp(
                    &layout
                        .ids_by_name
                        .get(*right_name)
                        .copied()
                        .unwrap_or_else(|| self.permission_id_for_name(right_name)),
                )
                .then_with(|| left_name.cmp(right_name))
        });

        let mut rows = sorted_permissions
            .into_iter()
            .map(|(permission_name, assignment)| {
                let mut row = self.render_permission_row_with_id(
                    layout
                        .ids_by_name
                        .get(permission_name)
                        .copied()
                        .unwrap_or_else(|| self.permission_id_for_name(permission_name)),
                    permission_name,
                    assignment,
                    false,
                );
                for (key, value) in subject_fields {
                    row.insert((*key).to_string(), value.clone());
                }
                row
            })
            .collect::<Vec<_>>();

        if rows.is_empty() {
            let mut row = BTreeMap::new();
            for (key, value) in subject_fields {
                row.insert((*key).to_string(), value.clone());
            }
            rows.push(row);
        }

        rows
    }

    pub(super) fn render_permoverview_row(
        &self,
        target_type: u32,
        id1: u64,
        id2: u64,
        permission_name: &str,
        assignment: &PermissionAssignment,
    ) -> (u32, u64, u64, u32, BTreeMap<String, String>) {
        let permission_id = self.permission_id_for_name(permission_name);
        let mut row = BTreeMap::new();
        row.insert(String::from("t"), target_type.to_string());
        row.insert(String::from("id1"), id1.to_string());
        row.insert(String::from("id2"), id2.to_string());
        row.insert(String::from("p"), permission_id.to_string());
        row.insert(String::from("v"), assignment.value.to_string());
        row.insert(
            String::from("n"),
            if assignment.negated {
                String::from("1")
            } else {
                String::from("0")
            },
        );
        row.insert(
            String::from("s"),
            if assignment.skipped {
                String::from("1")
            } else {
                String::from("0")
            },
        );

        (target_type, id1, id2, permission_id, row)
    }
}

impl BaselineRuntime {
    pub(super) fn handle_clientaddperm(
        &mut self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }

        let Some(client_database_id) = request
            .named_args
            .get("cldbid")
            .and_then(|value| value.parse::<u64>().ok())
        else {
            return QueryResponse::error(512, "cldbid is required");
        };
        let parsed_assignments = match self.parse_permission_assignments(request, &["cldbid"]) {
            Ok(parsed_assignments) => parsed_assignments,
            Err(response) => return response,
        };

        let (actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        if !self.client_database_id_exists(client_database_id) {
            return QueryResponse::error(768, "client not found");
        }
        let Some(target_permissions) = self.effective_permissions_for_client(
            actor.server_id,
            actor.channel_id,
            client_database_id,
        ) else {
            return QueryResponse::error(768, "client not found");
        };
        if let Err(permission_name) = check_permission_edit_allowed(
            &actor_permissions,
            &target_permissions,
            &[
                "i_client_needed_permission_modify_power",
                "i_group_needed_modify_power",
            ],
        ) {
            return self.insufficient_permission_response(permission_name);
        }

        if let Some(account) = self.query_account_by_cldbid_mut(client_database_id) {
            for parsed_assignment in &parsed_assignments {
                account.permissions.insert(
                    parsed_assignment.name.clone(),
                    parsed_assignment.assignment.clone(),
                );
            }
        } else {
            self.ensure_client_permission_target_mut(actor.server_id, client_database_id);
            let target = self.store.client_permissions.iter_mut().find(|t| t.client_database_id == client_database_id && t.server_id == actor.server_id).unwrap();
            for parsed_assignment in &parsed_assignments {
                target.permissions.insert(
                    parsed_assignment.name.clone(),
                    parsed_assignment.assignment.clone(),
                );
            }
            let _ = self.db.save_client_permission_target(target);
        }

        QueryResponse::ok()
    }

    pub(super) fn handle_clientdelperm(
        &mut self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }

        let Some(client_database_id) = request
            .named_args
            .get("cldbid")
            .and_then(|value| value.parse::<u64>().ok())
        else {
            return QueryResponse::error(512, "cldbid is required");
        };
        let permission_names = match self.parse_requested_permission_names(request, &["cldbid"]) {
            Ok(permission_names) => permission_names,
            Err(response) => return response,
        };

        let (actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        if !self.client_database_id_exists(client_database_id) {
            return QueryResponse::error(768, "client not found");
        }
        let Some(target_permissions) = self.effective_permissions_for_client(
            actor.server_id,
            actor.channel_id,
            client_database_id,
        ) else {
            return QueryResponse::error(768, "client not found");
        };
        if let Err(permission_name) = check_permission_edit_allowed(
            &actor_permissions,
            &target_permissions,
            &[
                "i_client_needed_permission_modify_power",
                "i_group_needed_modify_power",
            ],
        ) {
            return self.insufficient_permission_response(permission_name);
        }

        if let Some(account) = self.query_account_by_cldbid_mut(client_database_id) {
            for permission_name in &permission_names {
                account.permissions.remove(permission_name);
            }
        } else if let Some(target_index) = self
            .store
            .client_permissions
            .iter()
            .position(|target| target.client_database_id == client_database_id)
        {
            let remove_target = {
                let target = &mut self.store.client_permissions[target_index];
                for permission_name in &permission_names {
                    target.permissions.remove(permission_name);
                }
                let _ = self.db.save_client_permission_target(target);
                target.permissions.is_empty()
            };
            if remove_target {
                self.store
                    .client_permissions
                    .retain(|candidate| candidate.client_database_id != client_database_id);
            }
        }

        QueryResponse::ok()
    }

    pub(super) fn handle_clientpermlist(
        &self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if session.authenticated_login.is_none() {
            return QueryResponse::error(521, "login required");
        }
        let Some(server_id) = session.selected_virtual_server_id else {
            return QueryResponse::error(1024, "virtualserver is not selected");
        };

        let Some(client_database_id) = request
            .named_args
            .get("cldbid")
            .and_then(|value| value.parse::<u64>().ok())
        else {
            return QueryResponse::error(512, "cldbid is required");
        };
        let permissions = if let Some(account) = self.query_account_by_cldbid(client_database_id) {
            &account.permissions
        } else if let Some(target) = self.client_permission_target(server_id, client_database_id) {
            &target.permissions
        } else {
            return QueryResponse::error(768, "client not found");
        };

        let rows = permissions
            .iter()
            .map(|(permission_name, assignment)| {
                let mut row = self.render_permission_row(
                    permission_name,
                    assignment,
                    request.flags.contains("permsid"),
                );
                row.insert(String::from("cldbid"), client_database_id.to_string());
                row
            })
            .collect::<Vec<_>>();

        QueryResponse::ok_rows(rows)
    }

    pub(super) fn handle_channelclientaddperm(
        &mut self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }

        let Some(server_id) = session.selected_virtual_server_id else {
            return QueryResponse::error(522, "virtual server selection required");
        };
        let Some(channel_id) = request
            .named_args
            .get("cid")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return QueryResponse::error(512, "cid is required");
        };
        if !self.channel_exists(server_id, channel_id) {
            return QueryResponse::error(768, "channel not found");
        }
        let Some(client_database_id) = request
            .named_args
            .get("cldbid")
            .and_then(|value| value.parse::<u64>().ok())
        else {
            return QueryResponse::error(512, "cldbid is required");
        };
        if !self.client_database_id_exists(client_database_id) {
            return QueryResponse::error(768, "client not found");
        }
        let parsed_assignments =
            match self.parse_permission_assignments(request, &["cid", "cldbid"]) {
                Ok(parsed_assignments) => parsed_assignments,
                Err(response) => return response,
            };

        let (actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        let Some(target_permissions) =
            self.effective_permissions_for_client(actor.server_id, channel_id, client_database_id)
        else {
            return QueryResponse::error(768, "client not found");
        };
        if let Err(permission_name) = check_permission_edit_allowed(
            &actor_permissions,
            &target_permissions,
            &[
                "i_client_needed_permission_modify_power",
                "i_group_needed_modify_power",
            ],
        ) {
            return self.insufficient_permission_response(permission_name);
        }

        let target_index = self
            .store
            .channel_client_permissions
            .iter()
            .position(|target| {
                target.channel_id == channel_id && target.client_database_id == client_database_id
            });
        let target = if let Some(target_index) = target_index {
            &mut self.store.channel_client_permissions[target_index]
        } else {
            self.store
                .channel_client_permissions
                .push(ChannelClientPermissionTarget {
                    channel_id,
                    client_database_id,
                    permissions: BTreeMap::new(),
                });
            self.store
                .channel_client_permissions
                .last_mut()
                .expect("newly inserted channel-client permission target should exist")
        };

        for parsed_assignment in parsed_assignments {
            target
                .permissions
                .insert(parsed_assignment.name, parsed_assignment.assignment);
        }
        let _ = self.db.save_channel_client_permission_target(target);

        QueryResponse::ok()
    }

    pub(super) fn handle_playlistpermlist(
        &self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }

        let Some(server_id) = session.selected_virtual_server_id else {
            return QueryResponse::error(522, "virtual server selection required");
        };
        let Some(playlist_id) = request
            .named_args
            .get("playlist_id")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return QueryResponse::error(512, "playlist_id is required");
        };
        let Some(bot_id) = self.music_bot_id_by_playlist_id(server_id, playlist_id) else {
            return QueryResponse::error(768, "playlist not found");
        };
        let (actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        let Some(target_permissions) = self
            .store
            .music_bots
            .get(&bot_id)
            .map(|bot| bot.permissions.clone())
        else {
            return QueryResponse::error(768, "playlist not found");
        };
        if let Err(response) = self.ensure_playlist_permission_list_allowed(
            &actor_permissions,
            &target_permissions,
        ) {
            return response;
        }

        let mut permissions = target_permissions.iter().collect::<Vec<_>>();
        permissions.sort_by(|(left_name, _), (right_name, _)| {
            self.permission_id_for_name(left_name)
                .cmp(&self.permission_id_for_name(right_name))
                .then_with(|| left_name.cmp(right_name))
        });

        let mut rows = permissions
            .into_iter()
            .map(|(permission_name, assignment)| {
                let mut row = self.render_permission_row(
                    permission_name,
                    assignment,
                    request.flags.contains("permsid"),
                );
                row.insert(String::from("playlist_id"), playlist_id.to_string());
                row
            })
            .collect::<Vec<_>>();
        if rows.is_empty() {
            let mut row = BTreeMap::new();
            row.insert(String::from("playlist_id"), playlist_id.to_string());
            rows.push(row);
        }

        let _ = actor;
        QueryResponse::ok_rows(rows)
    }

    pub(super) fn handle_playlistclientlist(
        &self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }

        let Some(server_id) = session.selected_virtual_server_id else {
            return QueryResponse::error(522, "virtual server selection required");
        };
        let Some(playlist_id) = request
            .named_args
            .get("playlist_id")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return QueryResponse::error(512, "playlist_id is required");
        };
        let Some(bot_id) = self.music_bot_id_by_playlist_id(server_id, playlist_id) else {
            return QueryResponse::error(768, "playlist not found");
        };
        let (_actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        let Some(bot) = self.store.music_bots.get(&bot_id) else {
            return QueryResponse::error(768, "playlist not found");
        };
        if let Err(response) = self.ensure_playlist_permission_list_allowed(
            &actor_permissions,
            &bot.permissions,
        ) {
            return response;
        }

        let mut rows = bot
            .client_permissions
            .iter()
            .map(|target| {
                let mut row = BTreeMap::new();
                row.insert(String::from("playlist_id"), playlist_id.to_string());
                row.insert(String::from("cldbid"), target.client_database_id.to_string());
                row
            })
            .collect::<Vec<_>>();
        if rows.is_empty() {
            let mut row = BTreeMap::new();
            row.insert(String::from("playlist_id"), playlist_id.to_string());
            rows.push(row);
        }

        QueryResponse::ok_rows(rows)
    }

    pub(super) fn handle_playlistclientpermlist(
        &self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }

        let Some(server_id) = session.selected_virtual_server_id else {
            return QueryResponse::error(522, "virtual server selection required");
        };
        let Some(playlist_id) = request
            .named_args
            .get("playlist_id")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return QueryResponse::error(512, "playlist_id is required");
        };
        let Some(client_database_id) = request
            .named_args
            .get("cldbid")
            .and_then(|value| value.parse::<u64>().ok())
        else {
            return QueryResponse::error(512, "cldbid is required");
        };
        if !self.client_database_id_exists(client_database_id) {
            return QueryResponse::error(768, "client not found");
        }
        let Some(bot_id) = self.music_bot_id_by_playlist_id(server_id, playlist_id) else {
            return QueryResponse::error(768, "playlist not found");
        };
        let (_actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        let Some(bot) = self.store.music_bots.get(&bot_id) else {
            return QueryResponse::error(768, "playlist not found");
        };
        if let Err(response) = self.ensure_playlist_permission_list_allowed(
            &actor_permissions,
            &bot.permissions,
        ) {
            return response;
        }

        let mut rows = if let Some(target) = bot
            .client_permissions
            .iter()
            .find(|target| target.client_database_id == client_database_id)
        {
            let mut permissions = target.permissions.iter().collect::<Vec<_>>();
            permissions.sort_by(|(left_name, _), (right_name, _)| {
                self.permission_id_for_name(left_name)
                    .cmp(&self.permission_id_for_name(right_name))
                    .then_with(|| left_name.cmp(right_name))
            });

            permissions
                .into_iter()
                .map(|(permission_name, assignment)| {
                    let mut row = self.render_permission_row(
                        permission_name,
                        assignment,
                        request.flags.contains("permsid"),
                    );
                    row.insert(String::from("playlist_id"), playlist_id.to_string());
                    row.insert(String::from("cldbid"), client_database_id.to_string());
                    row
                })
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        if rows.is_empty() {
            let mut row = BTreeMap::new();
            row.insert(String::from("playlist_id"), playlist_id.to_string());
            row.insert(String::from("cldbid"), client_database_id.to_string());
            rows.push(row);
        }

        QueryResponse::ok_rows(rows)
    }

    pub(super) fn handle_playlistaddperm(
        &mut self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }

        let Some(server_id) = session.selected_virtual_server_id else {
            return QueryResponse::error(522, "virtual server selection required");
        };
        let Some(playlist_id) = request
            .named_args
            .get("playlist_id")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return QueryResponse::error(512, "playlist_id is required");
        };
        let Some(bot_id) = self.music_bot_id_by_playlist_id(server_id, playlist_id) else {
            return QueryResponse::error(768, "playlist not found");
        };
        let parsed_assignments = match self.parse_permission_assignments(request, &["playlist_id"]) {
            Ok(parsed_assignments) => parsed_assignments,
            Err(response) => return response,
        };
        let (_actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        let Some(target_permissions) = self
            .store
            .music_bots
            .get(&bot_id)
            .map(|bot| bot.permissions.clone())
        else {
            return QueryResponse::error(768, "playlist not found");
        };
        if let Err(response) = self.ensure_playlist_permission_modify_allowed(
            &actor_permissions,
            &target_permissions,
        ) {
            return response;
        }

        let include_permission_name = request.flags.contains("permsid");
        let Some(bot) = self.store.music_bots.get_mut(&bot_id) else {
            return QueryResponse::error(768, "playlist not found");
        };
        for parsed_assignment in &parsed_assignments {
            bot.permissions.insert(
                parsed_assignment.name.clone(),
                parsed_assignment.assignment.clone(),
            );
        }
        BaselineRuntime::normalize_music_bot_queue(bot);

        let rows = parsed_assignments
            .iter()
            .map(|parsed_assignment| {
                let mut row = self.render_permission_row(
                &parsed_assignment.name,
                &parsed_assignment.assignment,
                    include_permission_name,
                );
                row.insert(String::from("playlist_id"), playlist_id.to_string());
                row
            })
            .collect::<Vec<_>>();

        QueryResponse::ok_rows(rows)
    }

    pub(super) fn handle_playlistclientaddperm(
        &mut self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }

        let Some(server_id) = session.selected_virtual_server_id else {
            return QueryResponse::error(522, "virtual server selection required");
        };
        let Some(playlist_id) = request
            .named_args
            .get("playlist_id")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return QueryResponse::error(512, "playlist_id is required");
        };
        let Some(client_database_id) = request
            .named_args
            .get("cldbid")
            .and_then(|value| value.parse::<u64>().ok())
        else {
            return QueryResponse::error(512, "cldbid is required");
        };
        if !self.client_database_id_exists(client_database_id) {
            return QueryResponse::error(768, "client not found");
        }
        let Some(bot_id) = self.music_bot_id_by_playlist_id(server_id, playlist_id) else {
            return QueryResponse::error(768, "playlist not found");
        };
        let parsed_assignments = match self.parse_permission_assignments(
            request,
            &["playlist_id", "cldbid"],
        ) {
            Ok(parsed_assignments) => parsed_assignments,
            Err(response) => return response,
        };
        let (_actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        let Some(target_permissions) = self
            .store
            .music_bots
            .get(&bot_id)
            .map(|bot| bot.permissions.clone())
        else {
            return QueryResponse::error(768, "playlist not found");
        };
        if let Err(response) = self.ensure_playlist_permission_modify_allowed(
            &actor_permissions,
            &target_permissions,
        ) {
            return response;
        }

        let include_permission_name = request.flags.contains("permsid");
        let Some(bot) = self.store.music_bots.get_mut(&bot_id) else {
            return QueryResponse::error(768, "playlist not found");
        };
        let target_index = bot
            .client_permissions
            .iter()
            .position(|target| target.client_database_id == client_database_id);
        let target = if let Some(target_index) = target_index {
            &mut bot.client_permissions[target_index]
        } else {
            bot.client_permissions.push(PlaylistClientPermissionTarget {
                client_database_id,
                permissions: BTreeMap::new(),
            });
            bot.client_permissions
                .last_mut()
                .expect("newly inserted playlist-client permission target should exist")
        };

        for parsed_assignment in &parsed_assignments {
            target.permissions.insert(
                parsed_assignment.name.clone(),
                parsed_assignment.assignment.clone(),
            );
        }
        BaselineRuntime::normalize_music_bot_queue(bot);

        let rows = parsed_assignments
            .iter()
            .map(|parsed_assignment| {
                let mut row = self.render_permission_row(
                &parsed_assignment.name,
                &parsed_assignment.assignment,
                    include_permission_name,
                );
                row.insert(String::from("playlist_id"), playlist_id.to_string());
                row.insert(String::from("cldbid"), client_database_id.to_string());
                row
            })
            .collect::<Vec<_>>();

        QueryResponse::ok_rows(rows)
    }

    pub(super) fn handle_channelclientdelperm(
        &mut self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }

        let Some(server_id) = session.selected_virtual_server_id else {
            return QueryResponse::error(522, "virtual server selection required");
        };
        let Some(channel_id) = request
            .named_args
            .get("cid")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return QueryResponse::error(512, "cid is required");
        };
        if !self.channel_exists(server_id, channel_id) {
            return QueryResponse::error(768, "channel not found");
        }
        let Some(client_database_id) = request
            .named_args
            .get("cldbid")
            .and_then(|value| value.parse::<u64>().ok())
        else {
            return QueryResponse::error(512, "cldbid is required");
        };
        if !self.client_database_id_exists(client_database_id) {
            return QueryResponse::error(768, "client not found");
        }
        let permission_names =
            match self.parse_requested_permission_names(request, &["cid", "cldbid"]) {
                Ok(permission_names) => permission_names,
                Err(response) => return response,
            };

        let (actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        let Some(target_permissions) =
            self.effective_permissions_for_client(actor.server_id, channel_id, client_database_id)
        else {
            return QueryResponse::error(768, "client not found");
        };
        if let Err(permission_name) = check_permission_edit_allowed(
            &actor_permissions,
            &target_permissions,
            &[
                "i_client_needed_permission_modify_power",
                "i_group_needed_modify_power",
            ],
        ) {
            return self.insufficient_permission_response(permission_name);
        }

        if let Some(target_index) =
            self.store
                .channel_client_permissions
                .iter()
                .position(|target| {
                    target.channel_id == channel_id
                        && target.client_database_id == client_database_id
                })
        {
            let remove_target = {
                let target = &mut self.store.channel_client_permissions[target_index];
                for permission_name in permission_names {
                    target.permissions.remove(&permission_name);
                }
                let _ = self.db.save_channel_client_permission_target(target);
                target.permissions.is_empty()
            };
            if remove_target {
                self.store.channel_client_permissions.remove(target_index);
            }
        }

        QueryResponse::ok()
    }

    pub(super) fn handle_channelclientpermlist(
        &self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if session.authenticated_login.is_none() {
            return QueryResponse::error(521, "login required");
        }

        let Some(server_id) = session.selected_virtual_server_id else {
            return QueryResponse::error(522, "virtual server selection required");
        };
        let Some(channel_id) = request
            .named_args
            .get("cid")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return QueryResponse::error(512, "cid is required");
        };
        if !self.channel_exists(server_id, channel_id) {
            return QueryResponse::error(768, "channel not found");
        }
        let Some(client_database_id) = request
            .named_args
            .get("cldbid")
            .and_then(|value| value.parse::<u64>().ok())
        else {
            return QueryResponse::error(512, "cldbid is required");
        };
        if !self.client_database_id_exists(client_database_id) {
            return QueryResponse::error(768, "client not found");
        }

        let Some(target) = self.store.channel_client_permissions.iter().find(|target| {
            target.channel_id == channel_id && target.client_database_id == client_database_id
        }) else {
            return QueryResponse::ok_rows(Vec::new());
        };

        let mut permissions = target.permissions.iter().collect::<Vec<_>>();
        permissions.sort_by(|(left_name, _), (right_name, _)| {
            self.permission_id_for_name(left_name)
                .cmp(&self.permission_id_for_name(right_name))
                .then_with(|| left_name.cmp(right_name))
        });

        let rows = permissions
            .into_iter()
            .map(|(permission_name, assignment)| {
                let mut row = self.render_permission_row(
                    permission_name,
                    assignment,
                    request.flags.contains("permsid"),
                );
                row.insert(String::from("cid"), channel_id.to_string());
                row.insert(String::from("cldbid"), client_database_id.to_string());
                row
            })
            .collect::<Vec<_>>();

        QueryResponse::ok_rows(rows)
    }

    pub(super) fn handle_channeladdperm(
        &mut self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }

        let Some(server_id) = session.selected_virtual_server_id else {
            return QueryResponse::error(522, "virtual server selection required");
        };
        let Some(channel_id) = request
            .named_args
            .get("cid")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return QueryResponse::error(512, "cid is required");
        };
        let parsed_assignments = match self.parse_permission_assignments(request, &["cid"]) {
            Ok(parsed_assignments) => parsed_assignments,
            Err(response) => return response,
        };
        let (actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        let Some(target_permissions) = self
            .store
            .channels
            .get(&server_id)
            .and_then(|channels| channels.iter().find(|channel| channel.id == channel_id))
            .map(|channel| channel.permissions.clone())
        else {
            return QueryResponse::error(768, "channel not found");
        };
        if let Err(permission_name) = check_permission_edit_allowed(
            &actor_permissions,
            &target_permissions,
            &[
                "i_channel_needed_permission_modify_power",
                "i_group_needed_modify_power",
            ],
        ) {
            return self.insufficient_permission_response(permission_name);
        }
        let Some(channel) = self
            .store
            .channels
            .get_mut(&server_id)
            .and_then(|channels| channels.iter_mut().find(|channel| channel.id == channel_id))
        else {
            return QueryResponse::error(768, "channel not found");
        };

        for parsed_assignment in parsed_assignments {
            channel
                .permissions
                .insert(parsed_assignment.name, parsed_assignment.assignment);
        }
        let _ = self.db.save_channel(actor.server_id, channel);

        QueryResponse::ok()
    }

    pub(super) fn handle_channeldelperm(
        &mut self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }

        let Some(server_id) = session.selected_virtual_server_id else {
            return QueryResponse::error(522, "virtual server selection required");
        };
        let Some(channel_id) = request
            .named_args
            .get("cid")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return QueryResponse::error(512, "cid is required");
        };
        let permission_names = match self.parse_requested_permission_names(request, &["cid"]) {
            Ok(permission_names) => permission_names,
            Err(response) => return response,
        };
        let (actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        let Some(target_permissions) = self
            .store
            .channels
            .get(&server_id)
            .and_then(|channels| channels.iter().find(|channel| channel.id == channel_id))
            .map(|channel| channel.permissions.clone())
        else {
            return QueryResponse::error(768, "channel not found");
        };
        if let Err(permission_name) = check_permission_edit_allowed(
            &actor_permissions,
            &target_permissions,
            &[
                "i_channel_needed_permission_modify_power",
                "i_group_needed_modify_power",
            ],
        ) {
            return self.insufficient_permission_response(permission_name);
        }
        let Some(channel) = self
            .store
            .channels
            .get_mut(&server_id)
            .and_then(|channels| channels.iter_mut().find(|channel| channel.id == channel_id))
        else {
            return QueryResponse::error(768, "channel not found");
        };

        for permission_name in permission_names {
            channel.permissions.remove(&permission_name);
        }
        let _ = self.db.save_channel(actor.server_id, channel);

        QueryResponse::ok()
    }

    pub(super) fn handle_permfind(
        &self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }

        let permission_names = match self.parse_requested_permission_names(request, &[]) {
            Ok(permission_names) => permission_names,
            Err(response) => return response,
        };
        let mut rows = Vec::new();

        for permission_name in permission_names {
            let permission_id = self.permission_id_for_name(&permission_name);
            for group in self.store.server_groups.values() {
                if group.permissions.contains_key(&permission_name) {
                    let mut row = BTreeMap::new();
                    row.insert(String::from("t"), String::from("0"));
                    row.insert(String::from("id1"), group.id.to_string());
                    row.insert(String::from("id2"), String::from("0"));
                    row.insert(String::from("p"), permission_id.to_string());
                    row.insert(String::from("permsid"), permission_name.clone());
                    rows.push((
                        0_u32,
                        u64::from(group.id),
                        0_u64,
                        permission_id,
                        permission_name.clone(),
                        row,
                    ));
                }
            }

            for channels in self.store.channels.values() {
                for channel in channels {
                    if channel.permissions.contains_key(&permission_name) {
                        let mut row = BTreeMap::new();
                        row.insert(String::from("t"), String::from("2"));
                        row.insert(String::from("id1"), channel.id.to_string());
                        row.insert(String::from("id2"), String::from("0"));
                        row.insert(String::from("p"), permission_id.to_string());
                        row.insert(String::from("permsid"), permission_name.clone());
                        rows.push((
                            2_u32,
                            u64::from(channel.id),
                            0_u64,
                            permission_id,
                            permission_name.clone(),
                            row,
                        ));
                    }
                }
            }

            for group in self.store.channel_groups.values() {
                if group.permissions.contains_key(&permission_name) {
                    let mut row = BTreeMap::new();
                    row.insert(String::from("t"), String::from("3"));
                    row.insert(String::from("id1"), String::from("0"));
                    row.insert(String::from("id2"), group.id.to_string());
                    row.insert(String::from("p"), permission_id.to_string());
                    row.insert(String::from("permsid"), permission_name.clone());
                    rows.push((
                        3_u32,
                        0_u64,
                        u64::from(group.id),
                        permission_id,
                        permission_name.clone(),
                        row,
                    ));
                }
            }

            for target in &self.store.channel_client_permissions {
                if target.permissions.contains_key(&permission_name) {
                    let mut row = BTreeMap::new();
                    row.insert(String::from("t"), String::from("4"));
                    row.insert(String::from("id1"), target.channel_id.to_string());
                    row.insert(String::from("id2"), target.client_database_id.to_string());
                    row.insert(String::from("p"), permission_id.to_string());
                    row.insert(String::from("permsid"), permission_name.clone());
                    rows.push((
                        4_u32,
                        u64::from(target.channel_id),
                        target.client_database_id,
                        permission_id,
                        permission_name.clone(),
                        row,
                    ));
                }
            }

            for account in self.store.query_accounts.values() {
                if account.permissions.contains_key(&permission_name)
                    && let Some(client_database_id) = account.client_database_id
                {
                    let mut row = BTreeMap::new();
                    row.insert(String::from("t"), String::from("1"));
                    row.insert(String::from("id1"), client_database_id.to_string());
                    row.insert(String::from("id2"), String::from("0"));
                    row.insert(String::from("p"), permission_id.to_string());
                    row.insert(String::from("permsid"), permission_name.clone());
                    rows.push((
                        1_u32,
                        client_database_id,
                        0_u64,
                        permission_id,
                        permission_name.clone(),
                        row,
                    ));
                }
            }

            for target in &self.store.client_permissions {
                if target.permissions.contains_key(&permission_name) {
                    let mut row = BTreeMap::new();
                    row.insert(String::from("t"), String::from("1"));
                    row.insert(String::from("id1"), target.client_database_id.to_string());
                    row.insert(String::from("id2"), String::from("0"));
                    row.insert(String::from("p"), permission_id.to_string());
                    row.insert(String::from("permsid"), permission_name.clone());
                    rows.push((
                        1_u32,
                        target.client_database_id,
                        0_u64,
                        permission_id,
                        permission_name.clone(),
                        row,
                    ));
                }
            }
        }

        rows.sort_by(|left, right| {
            left.0
                .cmp(&right.0)
                .then_with(|| left.1.cmp(&right.1))
                .then_with(|| left.2.cmp(&right.2))
                .then_with(|| left.3.cmp(&right.3))
                .then_with(|| left.4.cmp(&right.4))
        });

        QueryResponse::ok_rows(rows.into_iter().map(|(_, _, _, _, _, row)| row).collect())
    }

    pub(super) fn handle_permget(
        &self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        let Some(login_name) = session.authenticated_login.as_ref() else {
            return QueryResponse::error(521, "login required");
        };
        let permission_names = match self.parse_requested_permission_names(request, &[]) {
            Ok(permission_names) => permission_names,
            Err(response) => return response,
        };
        let Some(account) = self.store.query_accounts.get(login_name) else {
            return QueryResponse::error(768, "query account not found");
        };
        let effective_permissions = self.effective_permissions_for_account(account);
        let mut rows = Vec::new();

        for permission_name in permission_names {
            let Some(assignment) = effective_permissions.get(&permission_name) else {
                return QueryResponse::error(
                    768,
                    format!("permission {} not assigned", permission_name),
                );
            };
            let mut row = self.render_permission_row(&permission_name, assignment, true);
            row.remove("permnegated");
            row.remove("permskip");
            rows.push(row);
        }

        QueryResponse::ok_rows(rows)
    }

    pub(super) fn handle_permidgetbyname(
        &self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if session.authenticated_login.is_none() {
            return QueryResponse::error(521, "login required");
        }

        let permission_names = match self.parse_requested_permission_names(request, &[]) {
            Ok(permission_names) => permission_names,
            Err(response) => return response,
        };
        let mut rows = Vec::new();

        for permission_name in permission_names {
            if !self.knows_permission_name(&permission_name) {
                return QueryResponse::error(
                    768,
                    format!("permission {} not found", permission_name),
                );
            }

            let mut row = BTreeMap::new();
            row.insert(String::from("permsid"), permission_name.clone());
            row.insert(
                String::from("permid"),
                self.permission_id_for_name(&permission_name).to_string(),
            );
            rows.push(row);
        }

        QueryResponse::ok_rows(rows)
    }

    pub(super) fn handle_permissionlist(&self, session: &QuerySessionState) -> QueryResponse {
        if session.authenticated_login.is_none() {
            return QueryResponse::error(521, "login required");
        }

        QueryResponse::ok_rows(self.build_permission_rows())
    }

    pub(super) fn handle_permoverview(
        &self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if session.authenticated_login.is_none() {
            return QueryResponse::error(521, "login required");
        }

        let Some(server_id) = session.selected_virtual_server_id else {
            return QueryResponse::error(522, "virtual server selection required");
        };
        let Some(channel_id) = request
            .named_args
            .get("cid")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return QueryResponse::error(512, "cid is required");
        };
        if !self.channel_exists(server_id, channel_id) {
            return QueryResponse::error(768, "channel not found");
        }
        let Some(client_database_id) = request
            .named_args
            .get("cldbid")
            .and_then(|value| value.parse::<u64>().ok())
        else {
            return QueryResponse::error(512, "cldbid is required");
        };
        let (server_group_ids, client_permissions) = if let Some(account) =
            self.query_account_by_cldbid(client_database_id)
        {
            (account.server_groups.clone(), account.permissions.clone())
        } else if let Some(client) = self.online_client_by_cldbid(server_id, client_database_id) {
            (
                client.server_groups.clone(),
                self.client_permission_target(server_id, client_database_id)
                    .map(|target| target.permissions.clone())
                    .unwrap_or_default(),
            )
        } else if let Some(target) = self.client_permission_target(server_id, client_database_id) {
            (Vec::new(), target.permissions.clone())
        } else {
            return QueryResponse::error(768, "client not found");
        };
        let permission_filter = match self.permoverview_requested_permissions(request) {
            Ok(permission_filter) => permission_filter,
            Err(response) => return response,
        };

        let mut rows = Vec::new();
        if let Some(channel) = self.channel_by_id(server_id, channel_id) {
            for (permission_name, assignment) in &channel.permissions {
                if !self.permission_filter_matches(&permission_filter, permission_name) {
                    continue;
                }
                rows.push(self.render_permoverview_row(
                    2,
                    u64::from(channel.id),
                    0,
                    permission_name,
                    assignment,
                ));
            }
        }

        for group_id in &server_group_ids {
            if let Some(group) = self.store.server_groups.get(group_id) {
                for (permission_name, assignment) in &group.permissions {
                    if !self.permission_filter_matches(&permission_filter, permission_name) {
                        continue;
                    }
                    rows.push(self.render_permoverview_row(
                        0,
                        u64::from(group.id),
                        0,
                        permission_name,
                        assignment,
                    ));
                }
            }
        }

        for (permission_name, assignment) in &client_permissions {
            if !self.permission_filter_matches(&permission_filter, permission_name) {
                continue;
            }
            rows.push(self.render_permoverview_row(
                1,
                client_database_id,
                0,
                permission_name,
                assignment,
            ));
        }

        if let Some(group) = self.effective_channel_group(channel_id, client_database_id) {
            for (permission_name, assignment) in &group.permissions {
                if !self.permission_filter_matches(&permission_filter, permission_name) {
                    continue;
                }
                rows.push(self.render_permoverview_row(
                    3,
                    0,
                    u64::from(group.id),
                    permission_name,
                    assignment,
                ));
            }
        }

        if let Some(target) = self.store.channel_client_permissions.iter().find(|target| {
            target.channel_id == channel_id && target.client_database_id == client_database_id
        }) {
            for (permission_name, assignment) in &target.permissions {
                if !self.permission_filter_matches(&permission_filter, permission_name) {
                    continue;
                }
                rows.push(self.render_permoverview_row(
                    4,
                    u64::from(channel_id),
                    client_database_id,
                    permission_name,
                    assignment,
                ));
            }
        }

        rows.sort_by(|left, right| {
            left.0
                .cmp(&right.0)
                .then_with(|| left.1.cmp(&right.1))
                .then_with(|| left.2.cmp(&right.2))
                .then_with(|| left.3.cmp(&right.3))
        });

        QueryResponse::ok_rows(rows.into_iter().map(|(_, _, _, _, row)| row).collect())
    }

    pub(super) fn handle_channelpermlist(
        &self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if session.authenticated_login.is_none() {
            return QueryResponse::error(521, "login required");
        }

        let Some(server_id) = session.selected_virtual_server_id else {
            return QueryResponse::error(522, "virtual server selection required");
        };
        let Some(channel_id) = request
            .named_args
            .get("cid")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return QueryResponse::error(512, "cid is required");
        };
        let Some(channel) = self.channel_by_id(server_id, channel_id) else {
            return QueryResponse::error(768, "channel not found");
        };

        let mut permissions = channel.permissions.iter().collect::<Vec<_>>();
        permissions.sort_by(|(left_name, _), (right_name, _)| {
            self.permission_id_for_name(left_name)
                .cmp(&self.permission_id_for_name(right_name))
                .then_with(|| left_name.cmp(right_name))
        });

        let rows = permissions
            .into_iter()
            .map(|(permission_name, assignment)| {
                let mut row = self.render_permission_row(
                    permission_name,
                    assignment,
                    request.flags.contains("permsid"),
                );
                row.insert(String::from("cid"), channel_id.to_string());
                row
            })
            .collect::<Vec<_>>();

        QueryResponse::ok_rows(rows)
    }
}

impl BaselineRuntime {
    pub(super) fn handle_servergrouplist(&self, session: &QuerySessionState) -> QueryResponse {
        if session.authenticated_login.is_none() {
            return QueryResponse::error(521, "login required");
        }

        let rows = self
            .store
            .server_groups
            .values()
            .map(|group| {
                let mut row = BTreeMap::new();
                row.insert(String::from("sgid"), group.id.to_string());
                row.insert(String::from("name"), group.name.clone());
                row.insert(String::from("type"), group.group_type.to_string());
                row.insert(String::from("iconid"), group.icon_id.to_string());
                row.insert(
                    String::from("savedb"),
                    if group.save_db {
                        String::from("1")
                    } else {
                        String::from("0")
                    },
                );
                row
            })
            .collect::<Vec<_>>();

        QueryResponse::ok_rows(rows)
    }

    pub(super) fn handle_servergroupsbyclientid(
        &self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if session.authenticated_login.is_none() {
            return QueryResponse::error(521, "login required");
        }

        let Some(client_database_id) = request
            .named_args
            .get("cldbid")
            .and_then(|value| value.parse::<u64>().ok())
        else {
            return QueryResponse::error(512, "cldbid is required");
        };
        let actor = match self.query_permission_actor_context(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };

        let Some(rows) = self.web_server_groups_by_client_rows(actor.server_id, client_database_id) else {
            return QueryResponse::error(768, "client not found");
        };

        QueryResponse::ok_rows(rows)
    }

    pub(super) fn handle_servergroupclientlist(
        &self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if session.authenticated_login.is_none() {
            return QueryResponse::error(521, "login required");
        }

        let Some(group_id) = request
            .named_args
            .get("sgid")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return QueryResponse::error(512, "sgid is required");
        };
        if !self.store.server_groups.contains_key(&group_id) {
            return QueryResponse::error(768, "server group not found");
        }

        let rows = self
            .store
            .query_accounts
            .values()
            .filter(|account| account.server_groups.contains(&group_id))
            .filter_map(|account| {
                account.client_database_id.map(|client_database_id| {
                    let mut row = BTreeMap::new();
                    row.insert(String::from("cldbid"), client_database_id.to_string());
                    if request.flags.contains("names") {
                        row.insert(String::from("name"), account.login_name.clone());
                        row.insert(
                            String::from("client_unique_identifier"),
                            format!("query-account-{}", client_database_id),
                        );
                    }
                    row
                })
            })
            .collect::<Vec<_>>();

        QueryResponse::ok_rows(rows)
    }

    pub(super) fn handle_servergroupadd(
        &mut self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if session.authenticated_login.is_none() {
            return QueryResponse::error(521, "login required");
        }

        let Some(_server_id) = session.selected_virtual_server_id else {
            return QueryResponse::error(522, "virtual server selection required");
        };
        let (_actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        if let Err(permission_name) = check_required_permission(
            &actor_permissions,
            &["b_virtualserver_servergroup_create"],
            "b_virtualserver_servergroup_create",
        ) {
            return self.insufficient_permission_response(permission_name);
        }
        let Some(group_name) = request.named_args.get("name").cloned() else {
            return QueryResponse::error(512, "name is required");
        };
        let group_type = match request.named_args.get("type") {
            Some(value) => match value.parse::<u32>() {
                Ok(group_type) => group_type,
                Err(_) => return QueryResponse::error(512, "type must be an integer"),
            },
            None => 1,
        };

        let group_id = self.next_server_group_id();
        self.store.server_groups.insert(
            group_id,
            ServerGroup {
                id: group_id,
                name: group_name,
                group_type,
                icon_id: 0,
                save_db: true,
                permissions: BTreeMap::new(),
            },
        );

        let mut row = BTreeMap::new();
        row.insert(String::from("sgid"), group_id.to_string());
        QueryResponse::ok_row(row)
    }

    pub(super) fn handle_servergroupaddclient(
        &mut self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if session.authenticated_login.is_none() {
            return QueryResponse::error(521, "login required");
        }

        let Some(group_id) = request
            .named_args
            .get("sgid")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return QueryResponse::error(512, "sgid is required");
        };
        let Some(client_database_id) = request
            .named_args
            .get("cldbid")
            .and_then(|value| value.parse::<u64>().ok())
        else {
            return QueryResponse::error(512, "cldbid is required");
        };
        let (actor, _actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };

        match self.web_add_server_group_client(
            actor.server_id,
            actor.channel_id,
            actor.client_database_id,
            group_id,
            client_database_id,
        ) {
            Ok(()) => QueryResponse::ok(),
            Err(WebServerGroupMutationError::InvalidGroup) => {
                QueryResponse::error(768, "server group not found")
            }
            Err(WebServerGroupMutationError::InvalidClient) => {
                QueryResponse::error(768, "client not found")
            }
            Err(WebServerGroupMutationError::PermissionDenied {
                failed_permission_id,
            }) => QueryResponse::error_with_fields(
                ERROR_INSUFFICIENT_PERMISSIONS,
                "insufficient client permissions",
                [("failed_permid", failed_permission_id.to_string())],
            ),
        }
    }

    pub(super) fn handle_servergroupdelclient(
        &mut self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if session.authenticated_login.is_none() {
            return QueryResponse::error(521, "login required");
        }

        let Some(group_id) = request
            .named_args
            .get("sgid")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return QueryResponse::error(512, "sgid is required");
        };
        let Some(client_database_id) = request
            .named_args
            .get("cldbid")
            .and_then(|value| value.parse::<u64>().ok())
        else {
            return QueryResponse::error(512, "cldbid is required");
        };
        let (actor, _actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };

        match self.web_del_server_group_client(
            actor.server_id,
            actor.channel_id,
            actor.client_database_id,
            group_id,
            client_database_id,
        ) {
            Ok(()) => QueryResponse::ok(),
            Err(WebServerGroupMutationError::InvalidGroup) => {
                QueryResponse::error(768, "server group not found")
            }
            Err(WebServerGroupMutationError::InvalidClient) => {
                QueryResponse::error(768, "client not found")
            }
            Err(WebServerGroupMutationError::PermissionDenied {
                failed_permission_id,
            }) => QueryResponse::error_with_fields(
                ERROR_INSUFFICIENT_PERMISSIONS,
                "insufficient client permissions",
                [("failed_permid", failed_permission_id.to_string())],
            ),
        }
    }

    pub(super) fn handle_servergroupdel(
        &mut self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }

        let Some(_server_id) = session.selected_virtual_server_id else {
            return QueryResponse::error(522, "virtual server selection required");
        };
        let (_actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        if let Err(permission_name) = check_required_permission(
            &actor_permissions,
            &["b_virtualserver_servergroup_delete"],
            "b_virtualserver_servergroup_delete",
        ) {
            return self.insufficient_permission_response(permission_name);
        }
        let Some(group_id) = request
            .named_args
            .get("sgid")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return QueryResponse::error(512, "sgid is required");
        };
        let force = request
            .named_args
            .get("force")
            .and_then(|value| parse_query_bool(value))
            .unwrap_or(false);
        if !self.store.server_groups.contains_key(&group_id) {
            return QueryResponse::error(768, "server group not found");
        }
        if self.server_group_in_use(group_id) && !force {
            return QueryResponse::error(
                512,
                "server group has assigned clients; set force=1 to delete",
            );
        }

        self.store.server_groups.remove(&group_id);
        for account in self.store.query_accounts.values_mut() {
            account
                .server_groups
                .retain(|existing_group_id| *existing_group_id != group_id);
        }
        self.normalize_query_account_groups();
        self.normalize_online_client_groups();

        QueryResponse::ok()
    }

    pub(super) fn handle_servergroupaddperm(
        &mut self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }

        let Some(group_id) = request
            .named_args
            .get("sgid")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return QueryResponse::error(512, "sgid is required");
        };
        let parsed_assignments = match self.parse_permission_assignments(request, &["sgid"]) {
            Ok(parsed_assignments) => parsed_assignments,
            Err(response) => return response,
        };
        let (_actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        let Some(target_group_permissions) = self
            .store
            .server_groups
            .get(&group_id)
            .map(|group| group.permissions.clone())
        else {
            return QueryResponse::error(768, "server group not found");
        };
        if let Err(permission_name) = check_permission_edit_allowed(
            &actor_permissions,
            &target_group_permissions,
            &[
                "i_server_group_needed_modify_power",
                "i_group_needed_modify_power",
            ],
        ) {
            return self.insufficient_permission_response(permission_name);
        }
        let Some(group) = self.store.server_groups.get_mut(&group_id) else {
            return QueryResponse::error(768, "server group not found");
        };

        for parsed_assignment in parsed_assignments {
            group
                .permissions
                .insert(parsed_assignment.name, parsed_assignment.assignment);
        }
        let _ = self.db.save_server_group(0, group);

        QueryResponse::ok()
    }

    pub(super) fn handle_servergroupautoaddperm(
        &mut self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if session.authenticated_login.is_none() {
            return QueryResponse::error(521, "login required");
        }

        let Some(sgtype) = request
            .named_args
            .get("sgtype")
            .and_then(|value| value.parse::<i64>().ok())
        else {
            return QueryResponse::error(512, "sgtype is required");
        };
        let parsed_assignments = match self.parse_permission_assignments(request, &["sgtype"]) {
            Ok(parsed_assignments) => parsed_assignments,
            Err(response) => return response,
        };
        let target_group_ids = self.server_group_ids_by_auto_type(sgtype);

        let (_actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        if let Err(permission_name) = check_required_permission(
            &actor_permissions,
            &["b_permission_modify_power_ignore"],
            "b_permission_modify_power_ignore",
        ) {
            return self.insufficient_permission_response(permission_name);
        }

        for group_id in target_group_ids {
            if let Some(group) = self.store.server_groups.get_mut(&group_id) {
                for parsed_assignment in &parsed_assignments {
                    group.permissions.insert(
                        parsed_assignment.name.clone(),
                        parsed_assignment.assignment.clone(),
                    );
                }
            }
        }

        QueryResponse::ok()
    }

    pub(super) fn handle_servergroupautodelperm(
        &mut self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if session.authenticated_login.is_none() {
            return QueryResponse::error(521, "login required");
        }

        let Some(sgtype) = request
            .named_args
            .get("sgtype")
            .and_then(|value| value.parse::<i64>().ok())
        else {
            return QueryResponse::error(512, "sgtype is required");
        };
        let permission_names = match self.parse_requested_permission_names(request, &["sgtype"]) {
            Ok(permission_names) => permission_names,
            Err(response) => return response,
        };
        let target_group_ids = self.server_group_ids_by_auto_type(sgtype);

        let (_actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        if let Err(permission_name) = check_required_permission(
            &actor_permissions,
            &["b_permission_modify_power_ignore"],
            "b_permission_modify_power_ignore",
        ) {
            return self.insufficient_permission_response(permission_name);
        }

        for group_id in target_group_ids {
            if let Some(group) = self.store.server_groups.get_mut(&group_id) {
                for permission_name in &permission_names {
                    group.permissions.remove(permission_name);
                }
            }
        }

        QueryResponse::ok()
    }

    pub(super) fn handle_servergroupcopy(
        &mut self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if session.authenticated_login.is_none() {
            return QueryResponse::error(521, "login required");
        }

        let Some(_server_id) = session.selected_virtual_server_id else {
            return QueryResponse::error(522, "virtual server selection required");
        };
        let Some(source_group_id) = request
            .named_args
            .get("ssgid")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return QueryResponse::error(512, "ssgid is required");
        };
        let Some(target_group_id) = request
            .named_args
            .get("tsgid")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return QueryResponse::error(512, "tsgid is required");
        };
        let (_actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        if let Err(permission_name) = check_required_permission(
            &actor_permissions,
            &["b_virtualserver_servergroup_create"],
            "b_virtualserver_servergroup_create",
        ) {
            return self.insufficient_permission_response(permission_name);
        }
        let Some(source_group) = self.store.server_groups.get(&source_group_id).cloned() else {
            return QueryResponse::error(768, "server group not found");
        };
        if let Err(permission_name) = check_group_modify_allowed(
            &actor_permissions,
            &source_group.permissions,
            &[
                "i_server_group_needed_modify_power",
                "i_group_needed_modify_power",
            ],
        ) {
            return self.insufficient_permission_response(permission_name);
        }

        if target_group_id != 0 {
            let Some(target_group_permissions) = self
                .store
                .server_groups
                .get(&target_group_id)
                .map(|group| group.permissions.clone())
            else {
                return QueryResponse::error(768, "server group not found");
            };
            if let Err(permission_name) = check_group_modify_allowed(
                &actor_permissions,
                &target_group_permissions,
                &[
                    "i_server_group_needed_modify_power",
                    "i_group_needed_modify_power",
                ],
            ) {
                return self.insufficient_permission_response(permission_name);
            }
        }

        let resulting_group_id = if target_group_id == 0 {
            let Some(group_name) = request.named_args.get("name").cloned() else {
                return QueryResponse::error(512, "name is required when tsgid=0");
            };
            let group_type = request
                .named_args
                .get("type")
                .and_then(|value| value.parse::<u32>().ok())
                .unwrap_or(source_group.group_type);
            let new_group_id = self.next_server_group_id();
            self.store.server_groups.insert(
                new_group_id,
                ServerGroup {
                    id: new_group_id,
                    name: group_name,
                    group_type,
                    icon_id: source_group.icon_id,
                    save_db: source_group.save_db,
                    permissions: source_group.permissions.clone(),
                },
            );
            new_group_id
        } else {
            let Some(target_group) = self.store.server_groups.get_mut(&target_group_id) else {
                return QueryResponse::error(768, "server group not found");
            };
            target_group.icon_id = source_group.icon_id;
            target_group.save_db = source_group.save_db;
            target_group.permissions = source_group.permissions.clone();
            target_group_id
        };

        let mut row = BTreeMap::new();
        row.insert(String::from("sgid"), resulting_group_id.to_string());
        QueryResponse::ok_row(row)
    }

    pub(super) fn handle_servergroupdelperm(
        &mut self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }

        let Some(group_id) = request
            .named_args
            .get("sgid")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return QueryResponse::error(512, "sgid is required");
        };
        let permission_names = match self.parse_requested_permission_names(request, &["sgid"]) {
            Ok(permission_names) => permission_names,
            Err(response) => return response,
        };
        let (_actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        let Some(target_group_permissions) = self
            .store
            .server_groups
            .get(&group_id)
            .map(|group| group.permissions.clone())
        else {
            return QueryResponse::error(768, "server group not found");
        };
        if let Err(permission_name) = check_permission_edit_allowed(
            &actor_permissions,
            &target_group_permissions,
            &[
                "i_server_group_needed_modify_power",
                "i_group_needed_modify_power",
            ],
        ) {
            return self.insufficient_permission_response(permission_name);
        }
        let Some(group) = self.store.server_groups.get_mut(&group_id) else {
            return QueryResponse::error(768, "server group not found");
        };

        for permission_name in permission_names {
            group.permissions.remove(&permission_name);
        }
        let _ = self.db.save_server_group(0, group);

        QueryResponse::ok()
    }

    pub(super) fn handle_servergrouprename(
        &mut self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }

        let Some(group_id) = request
            .named_args
            .get("sgid")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return QueryResponse::error(512, "sgid is required");
        };
        let (_actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        let Some(target_group_permissions) = self
            .store
            .server_groups
            .get(&group_id)
            .map(|group| group.permissions.clone())
        else {
            return QueryResponse::error(768, "server group not found");
        };
        if let Err(permission_name) = check_group_modify_allowed(
            &actor_permissions,
            &target_group_permissions,
            &[
                "i_server_group_needed_modify_power",
                "i_group_needed_modify_power",
            ],
        ) {
            return self.insufficient_permission_response(permission_name);
        }
        let Some(group_name) = request.named_args.get("name").cloned() else {
            return QueryResponse::error(512, "name is required");
        };
        let Some(group) = self.store.server_groups.get_mut(&group_id) else {
            return QueryResponse::error(768, "server group not found");
        };

        group.name = group_name;
        QueryResponse::ok()
    }

    pub(super) fn handle_servergrouppermlist(
        &self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if session.authenticated_login.is_none() {
            return QueryResponse::error(521, "login required");
        }

        let Some(group_id) = request
            .named_args
            .get("sgid")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return QueryResponse::error(512, "sgid is required");
        };
        let Some(group) = self.store.server_groups.get(&group_id) else {
            return QueryResponse::error(768, "server group not found");
        };

        let mut permissions = group.permissions.iter().collect::<Vec<_>>();
        permissions.sort_by(|(left_name, _), (right_name, _)| {
            self.permission_id_for_name(left_name)
                .cmp(&self.permission_id_for_name(right_name))
                .then_with(|| left_name.cmp(right_name))
        });

        let rows = permissions
            .into_iter()
            .map(|(permission_name, assignment)| {
                self.render_permission_row(
                    permission_name,
                    assignment,
                    request.flags.contains("permsid"),
                )
            })
            .collect::<Vec<_>>();

        QueryResponse::ok_rows(rows)
    }
}

impl BaselineRuntime {
    pub(super) fn handle_channelgroupadd(
        &mut self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if session.authenticated_login.is_none() {
            return QueryResponse::error(521, "login required");
        }

        let Some(_server_id) = session.selected_virtual_server_id else {
            return QueryResponse::error(522, "virtual server selection required");
        };
        let (_actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        if let Err(permission_name) = check_required_permission(
            &actor_permissions,
            &["b_virtualserver_channelgroup_create"],
            "b_virtualserver_channelgroup_create",
        ) {
            return self.insufficient_permission_response(permission_name);
        }
        let Some(group_name) = request.named_args.get("name").cloned() else {
            return QueryResponse::error(512, "name is required");
        };
        let group_type = match request.named_args.get("type") {
            Some(value) => match value.parse::<u32>() {
                Ok(group_type) => group_type,
                Err(_) => return QueryResponse::error(512, "type must be an integer"),
            },
            None => 1,
        };

        let group_id = self.next_channel_group_id();
        self.store.channel_groups.insert(
            group_id,
            ChannelGroup {
                id: group_id,
                name: group_name,
                group_type,
                icon_id: 0,
                save_db: true,
                permissions: BTreeMap::new(),
            },
        );

        let mut row = BTreeMap::new();
        row.insert(String::from("cgid"), group_id.to_string());
        QueryResponse::ok_row(row)
    }

    pub(super) fn handle_channelgroupaddperm(
        &mut self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }

        let Some(group_id) = request
            .named_args
            .get("cgid")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return QueryResponse::error(512, "cgid is required");
        };
        let parsed_assignments = match self.parse_permission_assignments(request, &["cgid"]) {
            Ok(parsed_assignments) => parsed_assignments,
            Err(response) => return response,
        };
        let (_actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        let Some(target_group_permissions) = self
            .store
            .channel_groups
            .get(&group_id)
            .map(|group| group.permissions.clone())
        else {
            return QueryResponse::error(768, "channel group not found");
        };
        if let Err(permission_name) = check_permission_edit_allowed(
            &actor_permissions,
            &target_group_permissions,
            &[
                "i_channel_group_needed_modify_power",
                "i_group_needed_modify_power",
            ],
        ) {
            return self.insufficient_permission_response(permission_name);
        }
        let Some(group) = self.store.channel_groups.get_mut(&group_id) else {
            return QueryResponse::error(768, "channel group not found");
        };

        for parsed_assignment in parsed_assignments {
            group
                .permissions
                .insert(parsed_assignment.name, parsed_assignment.assignment);
        }
        let _ = self.db.save_channel_group(0, group);

        QueryResponse::ok()
    }

    pub(super) fn handle_channelgroupclientlist(
        &self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if session.authenticated_login.is_none() {
            return QueryResponse::error(521, "login required");
        }

        let Some(server_id) = session.selected_virtual_server_id else {
            return QueryResponse::error(522, "virtual server selection required");
        };
        let Some(channels) = self.store.channels.get(&server_id) else {
            return QueryResponse::error(768, "virtual server channels not found");
        };
        let valid_channel_ids = channels
            .iter()
            .map(|channel| channel.id)
            .collect::<BTreeSet<_>>();

        let channel_id_filter = request
            .named_args
            .get("cid")
            .and_then(|value| value.parse::<u32>().ok());
        let client_database_id_filter = request
            .named_args
            .get("cldbid")
            .and_then(|value| value.parse::<u64>().ok());
        let group_id_filter = request
            .named_args
            .get("cgid")
            .and_then(|value| value.parse::<u32>().ok());

        if let Some(channel_id) = channel_id_filter
            && !valid_channel_ids.contains(&channel_id)
        {
            return QueryResponse::error(768, "channel not found");
        }
        if let Some(group_id) = group_id_filter
            && !self.store.channel_groups.contains_key(&group_id)
        {
            return QueryResponse::error(768, "channel group not found");
        }

        let rows = self
            .store
            .channel_group_assignments
            .iter()
            .filter(|assignment| valid_channel_ids.contains(&assignment.channel_id))
            .filter(|assignment| {
                channel_id_filter.is_none_or(|channel_id| assignment.channel_id == channel_id)
            })
            .filter(|assignment| {
                client_database_id_filter.is_none_or(|client_database_id| {
                    assignment.client_database_id == client_database_id
                })
            })
            .filter(|assignment| {
                group_id_filter.is_none_or(|group_id| assignment.channel_group_id == group_id)
            })
            .map(|assignment| {
                let mut row = BTreeMap::new();
                row.insert(String::from("cid"), assignment.channel_id.to_string());
                row.insert(
                    String::from("cldbid"),
                    assignment.client_database_id.to_string(),
                );
                row.insert(
                    String::from("cgid"),
                    assignment.channel_group_id.to_string(),
                );
                row
            })
            .collect::<Vec<_>>();

        QueryResponse::ok_rows(rows)
    }

    pub(super) fn handle_channelgroupcopy(
        &mut self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if session.authenticated_login.is_none() {
            return QueryResponse::error(521, "login required");
        }

        let Some(_server_id) = session.selected_virtual_server_id else {
            return QueryResponse::error(522, "virtual server selection required");
        };
        let Some(source_group_id) = request
            .named_args
            .get("scgid")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return QueryResponse::error(512, "scgid is required");
        };
        let Some(target_group_id) = request
            .named_args
            .get("tcgid")
            .or_else(|| request.named_args.get("tsgid"))
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return QueryResponse::error(512, "tcgid or tsgid is required");
        };
        let (_actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        if let Err(permission_name) = check_required_permission(
            &actor_permissions,
            &["b_virtualserver_channelgroup_create"],
            "b_virtualserver_channelgroup_create",
        ) {
            return self.insufficient_permission_response(permission_name);
        }
        let Some(source_group) = self.store.channel_groups.get(&source_group_id).cloned() else {
            return QueryResponse::error(768, "channel group not found");
        };
        if let Err(permission_name) = check_group_modify_allowed(
            &actor_permissions,
            &source_group.permissions,
            &[
                "i_channel_group_needed_modify_power",
                "i_group_needed_modify_power",
            ],
        ) {
            return self.insufficient_permission_response(permission_name);
        }

        if target_group_id != 0 {
            let Some(target_group_permissions) = self
                .store
                .channel_groups
                .get(&target_group_id)
                .map(|group| group.permissions.clone())
            else {
                return QueryResponse::error(768, "channel group not found");
            };
            if let Err(permission_name) = check_group_modify_allowed(
                &actor_permissions,
                &target_group_permissions,
                &[
                    "i_channel_group_needed_modify_power",
                    "i_group_needed_modify_power",
                ],
            ) {
                return self.insufficient_permission_response(permission_name);
            }
        }

        let resulting_group_id = if target_group_id == 0 {
            let Some(group_name) = request.named_args.get("name").cloned() else {
                return QueryResponse::error(512, "name is required when tcgid=0");
            };
            let group_type = request
                .named_args
                .get("type")
                .and_then(|value| value.parse::<u32>().ok())
                .unwrap_or(source_group.group_type);
            let new_group_id = self.next_channel_group_id();
            self.store.channel_groups.insert(
                new_group_id,
                ChannelGroup {
                    id: new_group_id,
                    name: group_name,
                    group_type,
                    icon_id: source_group.icon_id,
                    save_db: source_group.save_db,
                    permissions: source_group.permissions.clone(),
                },
            );
            new_group_id
        } else {
            let Some(target_group) = self.store.channel_groups.get_mut(&target_group_id) else {
                return QueryResponse::error(768, "channel group not found");
            };
            target_group.icon_id = source_group.icon_id;
            target_group.save_db = source_group.save_db;
            target_group.permissions = source_group.permissions.clone();
            target_group_id
        };

        let mut row = BTreeMap::new();
        row.insert(String::from("cgid"), resulting_group_id.to_string());
        QueryResponse::ok_row(row)
    }

    pub(super) fn handle_channelgroupdel(
        &mut self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }

        let Some(_server_id) = session.selected_virtual_server_id else {
            return QueryResponse::error(522, "virtual server selection required");
        };
        let (_actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        if let Err(permission_name) = check_required_permission(
            &actor_permissions,
            &["b_virtualserver_channelgroup_delete"],
            "b_virtualserver_channelgroup_delete",
        ) {
            return self.insufficient_permission_response(permission_name);
        }
        let Some(group_id) = request
            .named_args
            .get("cgid")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return QueryResponse::error(512, "cgid is required");
        };
        let force = request
            .named_args
            .get("force")
            .and_then(|value| parse_query_bool(value))
            .unwrap_or(false);
        if !self.store.channel_groups.contains_key(&group_id) {
            return QueryResponse::error(768, "channel group not found");
        }
        if self.channel_group_in_use(group_id) && !force {
            return QueryResponse::error(
                512,
                "channel group has assigned clients; set force=1 to delete",
            );
        }

        self.store.channel_groups.remove(&group_id);
        self.store
            .channel_group_assignments
            .retain(|assignment| assignment.channel_group_id != group_id);
        self.normalize_channel_group_assignments();

        QueryResponse::ok()
    }

    pub(super) fn handle_channelgroupdelperm(
        &mut self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }

        let Some(group_id) = request
            .named_args
            .get("cgid")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return QueryResponse::error(512, "cgid is required");
        };
        let permission_names = match self.parse_requested_permission_names(request, &["cgid"]) {
            Ok(permission_names) => permission_names,
            Err(response) => return response,
        };
        let (_actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        let Some(target_group_permissions) = self
            .store
            .channel_groups
            .get(&group_id)
            .map(|group| group.permissions.clone())
        else {
            return QueryResponse::error(768, "channel group not found");
        };
        if let Err(permission_name) = check_permission_edit_allowed(
            &actor_permissions,
            &target_group_permissions,
            &[
                "i_channel_group_needed_modify_power",
                "i_group_needed_modify_power",
            ],
        ) {
            return self.insufficient_permission_response(permission_name);
        }
        let Some(group) = self.store.channel_groups.get_mut(&group_id) else {
            return QueryResponse::error(768, "channel group not found");
        };

        for permission_name in permission_names {
            group.permissions.remove(&permission_name);
        }
        let _ = self.db.save_channel_group(0, group);

        QueryResponse::ok()
    }

    pub(super) fn handle_channelgrouplist(&self, session: &QuerySessionState) -> QueryResponse {
        if session.authenticated_login.is_none() {
            return QueryResponse::error(521, "login required");
        }

        let Some(_server_id) = session.selected_virtual_server_id else {
            return QueryResponse::error(522, "virtual server selection required");
        };

        let rows = self
            .store
            .channel_groups
            .values()
            .map(|group| {
                let mut row = BTreeMap::new();
                row.insert(String::from("cgid"), group.id.to_string());
                row.insert(String::from("name"), group.name.clone());
                row.insert(String::from("type"), group.group_type.to_string());
                row.insert(String::from("iconid"), group.icon_id.to_string());
                row.insert(
                    String::from("savedb"),
                    if group.save_db {
                        String::from("1")
                    } else {
                        String::from("0")
                    },
                );
                row
            })
            .collect::<Vec<_>>();

        QueryResponse::ok_rows(rows)
    }

    pub(super) fn handle_channelgrouppermlist(
        &self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if session.authenticated_login.is_none() {
            return QueryResponse::error(521, "login required");
        }

        let Some(group_id) = request
            .named_args
            .get("cgid")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return QueryResponse::error(512, "cgid is required");
        };
        let Some(group) = self.store.channel_groups.get(&group_id) else {
            return QueryResponse::error(768, "channel group not found");
        };

        let mut permissions = group.permissions.iter().collect::<Vec<_>>();
        permissions.sort_by(|(left_name, _), (right_name, _)| {
            self.permission_id_for_name(left_name)
                .cmp(&self.permission_id_for_name(right_name))
                .then_with(|| left_name.cmp(right_name))
        });

        let rows = permissions
            .into_iter()
            .map(|(permission_name, assignment)| {
                self.render_permission_row(
                    permission_name,
                    assignment,
                    request.flags.contains("permsid"),
                )
            })
            .collect::<Vec<_>>();

        QueryResponse::ok_rows(rows)
    }

    pub(super) fn handle_channelgrouprename(
        &mut self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }

        let Some(group_id) = request
            .named_args
            .get("cgid")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return QueryResponse::error(512, "cgid is required");
        };
        let (_actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        let Some(target_group_permissions) = self
            .store
            .channel_groups
            .get(&group_id)
            .map(|group| group.permissions.clone())
        else {
            return QueryResponse::error(768, "channel group not found");
        };
        if let Err(permission_name) = check_group_modify_allowed(
            &actor_permissions,
            &target_group_permissions,
            &[
                "i_channel_group_needed_modify_power",
                "i_group_needed_modify_power",
            ],
        ) {
            return self.insufficient_permission_response(permission_name);
        }
        let Some(group_name) = request.named_args.get("name").cloned() else {
            return QueryResponse::error(512, "name is required");
        };
        let Some(group) = self.store.channel_groups.get_mut(&group_id) else {
            return QueryResponse::error(768, "channel group not found");
        };

        group.name = group_name;
        QueryResponse::ok()
    }

    pub(super) fn handle_setclientchannelgroup(
        &mut self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if session.authenticated_login.is_none() {
            return QueryResponse::error(521, "login required");
        }

        let Some(server_id) = session.selected_virtual_server_id else {
            return QueryResponse::error(522, "virtual server selection required");
        };
        let Some(group_id) = request
            .named_args
            .get("cgid")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return QueryResponse::error(512, "cgid is required");
        };
        let Some(channel_id) = request
            .named_args
            .get("cid")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return QueryResponse::error(512, "cid is required");
        };
        let Some(client_database_id) = request
            .named_args
            .get("cldbid")
            .and_then(|value| value.parse::<u64>().ok())
        else {
            return QueryResponse::error(512, "cldbid is required");
        };
        let (_actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        let Some(target_group_permissions) = self
            .store
            .channel_groups
            .get(&group_id)
            .map(|group| group.permissions.clone())
        else {
            return QueryResponse::error(768, "channel group not found");
        };

        let Some(channels) = self.store.channels.get(&server_id) else {
            return QueryResponse::error(768, "virtual server channels not found");
        };
        if !channels.iter().any(|channel| channel.id == channel_id) {
            return QueryResponse::error(768, "channel not found");
        }
        if !self.client_database_id_exists(client_database_id) {
            return QueryResponse::error(768, "client not found");
        }

        let current_group_id = self
            .store
            .channel_group_assignments
            .iter()
            .find(|assignment| {
                assignment.channel_id == channel_id
                    && assignment.client_database_id == client_database_id
            })
            .map(|assignment| assignment.channel_group_id);

        if current_group_id == Some(group_id) {
            return QueryResponse::ok();
        }

        if let Some(existing_group_id) = current_group_id {
            let Some(current_group_permissions) = self
                .store
                .channel_groups
                .get(&existing_group_id)
                .map(|group| group.permissions.clone())
            else {
                return QueryResponse::error(768, "channel group not found");
            };
            if let Err(permission_name) = check_channel_group_membership_change(
                &actor_permissions,
                &current_group_permissions,
                false,
            ) {
                return self.insufficient_permission_response(permission_name);
            }
        }
        if let Err(permission_name) = check_channel_group_membership_change(
            &actor_permissions,
            &target_group_permissions,
            true,
        ) {
            return self.insufficient_permission_response(permission_name);
        }

        if let Some(assignment) =
            self.store
                .channel_group_assignments
                .iter_mut()
                .find(|assignment| {
                    assignment.channel_id == channel_id
                        && assignment.client_database_id == client_database_id
                })
        {
            assignment.channel_group_id = group_id;
        } else {
            self.store
                .channel_group_assignments
                .push(ChannelGroupAssignment {
                    channel_id,
                    client_database_id,
                    channel_group_id: group_id,
                });
        }
        self.normalize_channel_group_assignments();

        QueryResponse::ok()
    }
}

pub(super) fn seed_store(
    admin_permissions: &BTreeMap<String, PermissionAssignment>,
    guest_permissions: &BTreeMap<String, PermissionAssignment>,
    server_admin_permissions: &BTreeMap<String, PermissionAssignment>,
    channel_admin_permissions: &BTreeMap<String, PermissionAssignment>,
    channel_operator_permissions: &BTreeMap<String, PermissionAssignment>,
    channel_guest_permissions: &BTreeMap<String, PermissionAssignment>,
) -> InMemoryStore {
    let mut query_accounts = BTreeMap::new();
    query_accounts.insert(
        String::from("serveradmin"),
        QueryAccount {
            login_name: String::from("serveradmin"),
            password: String::from("serveradmin"),
            server_id: Some(1),
            client_database_id: Some(1),
            server_groups: vec![6],
            permissions: BTreeMap::new(),
        },
    );

    let mut server_groups = BTreeMap::new();
    server_groups.insert(
        6,
        ServerGroup {
            id: 6,
            name: String::from("Admin Server Query"),
            group_type: 1,
            icon_id: 300,
            save_db: true,
            permissions: admin_permissions.clone(),
        },
    );
    server_groups.insert(
        7,
        ServerGroup {
            id: 7,
            name: String::from("Guest Server Query"),
            group_type: 1,
            icon_id: 0,
            save_db: true,
            permissions: guest_permissions.clone(),
        },
    );
    server_groups.insert(
        8,
        ServerGroup {
            id: 8,
            name: String::from("Normal"),
            group_type: 1,
            icon_id: 0,
            save_db: true,
            permissions: BTreeMap::new(),
        },
    );
    server_groups.insert(
        9,
        ServerGroup {
            id: 9,
            name: String::from("Server Admin"),
            group_type: 1,
            icon_id: server_admin_permissions
                .get("i_icon_id")
                .map(|assignment| assignment.value)
                .unwrap_or(300),
            save_db: true,
            permissions: server_admin_permissions.clone(),
        },
    );

    let mut channel_groups = BTreeMap::new();
    channel_groups.insert(
        8,
        ChannelGroup {
            id: 8,
            name: String::from("Channel Admin"),
            group_type: 2,
            icon_id: channel_admin_permissions
                .get("i_icon_id")
                .map(|assignment| assignment.value)
                .unwrap_or(100),
            save_db: true,
            permissions: channel_admin_permissions.clone(),
        },
    );
    channel_groups.insert(
        9,
        ChannelGroup {
            id: 9,
            name: String::from("Operator"),
            group_type: 1,
            icon_id: channel_operator_permissions
                .get("i_icon_id")
                .map(|assignment| assignment.value)
                .unwrap_or(200),
            save_db: true,
            permissions: channel_operator_permissions.clone(),
        },
    );
    channel_groups.insert(
        10,
        ChannelGroup {
            id: 10,
            name: String::from("Guest"),
            group_type: 2,
            icon_id: channel_guest_permissions
                .get("i_icon_id")
                .map(|assignment| assignment.value)
                .unwrap_or(0),
            save_db: true,
            permissions: channel_guest_permissions.clone(),
        },
    );

    let mut virtual_servers = BTreeMap::new();
    virtual_servers.insert(
        1,
        VirtualServer {
            id: 1,
            port: 9987,
            name: String::from("BlackTeaSpeak Compat"),
            unique_identifier: String::from("compat-baseline-uid"),
            welcome_message: String::from("Welcome to BlackTeaSpeak Compat"),
            host_message: String::new(),
            host_message_mode: 0,
            ask_for_privilegekey: 0,
            max_clients: 128,
            antiflood_points_tick_reduce: 10,
            antiflood_points_needed_command_block: 150,
            antiflood_points_needed_ip_block: 250,
            antiflood_ban_time: 300,
        },
    );

    let mut channels = BTreeMap::new();
    channels.insert(
        1,
        vec![
            Channel {
                id: 1,
                parent_id: 0,
                order: 0,
                kind: ChannelKind::Permanent,
                name: String::from("Default Channel"),
                topic: String::from("Default Channel has no topic"),
                description: String::new(),
                permissions: BTreeMap::new(),
            },
            Channel {
                id: 2,
                parent_id: 0,
                order: 1,
                kind: ChannelKind::Permanent,
                name: String::from("Music Lounge"),
                topic: String::from("Music bot staging area"),
                description: String::new(),
                permissions: BTreeMap::new(),
            },
        ],
    );

    let mut clients = BTreeMap::new();
    clients.insert(
        40,
        Client {
            database_id: 40,
            unique_identifier: String::from("compat-seed-user-40"),
            nickname: String::from("ScP"),
            description: String::new(),
            created_at: current_unix_timestamp(),
            last_connected_at: current_unix_timestamp(),
            total_connections: 1,
            month_bytes_uploaded: 0,
            month_bytes_downloaded: 0,
            total_bytes_uploaded: 0,
            total_bytes_downloaded: 0,
        },
    );
    clients.insert(
        41,
        Client {
            database_id: 41,
            unique_identifier: String::from("compat-seed-user-41"),
            nickname: String::from("Rabe85"),
            description: String::new(),
            created_at: current_unix_timestamp(),
            last_connected_at: current_unix_timestamp(),
            total_connections: 1,
            month_bytes_uploaded: 0,
            month_bytes_downloaded: 0,
            total_bytes_uploaded: 0,
            total_bytes_downloaded: 0,
        },
    );
    clients.insert(
        42,
        Client {
            database_id: 42,
            unique_identifier: String::from("compat-seed-user-42"),
            nickname: String::from("DJ Mix"),
            description: String::new(),
            created_at: current_unix_timestamp(),
            last_connected_at: current_unix_timestamp(),
            total_connections: 1,
            month_bytes_uploaded: 0,
            month_bytes_downloaded: 0,
            total_bytes_uploaded: 0,
            total_bytes_downloaded: 0,
        },
    );

    let mut online_clients = BTreeMap::new();
    online_clients.insert(
        10,
        OnlineClient {
            id: 10,
            database_id: 40,
            unique_identifier: String::from("compat-seed-user-40"),
            nickname: String::from("ScP"),
            last_seen_at: current_unix_timestamp(),
            away: false,
            away_message: String::new(),
            input_muted: false,
            output_muted: false,
            server_id: 1,
            channel_id: 1,
            client_type: 0,
            version: String::from("BlackTeaSpeak 1.5.6 compat-seed"),
            platform: String::from("Windows"),
            country: String::from("DE"),
            connection_ip: String::from("198.51.100.10"),
            server_groups: vec![8],
            connected_at: current_unix_timestamp(),
            extra_properties: BTreeMap::new(),
        },
    );
    online_clients.insert(
        11,
        OnlineClient {
            id: 11,
            database_id: 41,
            unique_identifier: String::from("compat-seed-user-41"),
            nickname: String::from("Rabe85"),
            last_seen_at: current_unix_timestamp(),
            away: false,
            away_message: String::new(),
            input_muted: false,
            output_muted: false,
            server_id: 1,
            channel_id: 1,
            client_type: 0,
            version: String::from("BlackTeaSpeak 1.5.6 compat-seed"),
            platform: String::from("Linux"),
            country: String::from("AT"),
            connection_ip: String::from("198.51.100.11"),
            server_groups: vec![8],
            connected_at: current_unix_timestamp(),
            extra_properties: BTreeMap::new(),
        },
    );
    online_clients.insert(
        12,
        OnlineClient {
            id: 12,
            database_id: 42,
            unique_identifier: String::from("compat-seed-user-42"),
            nickname: String::from("DJ Mix"),
            last_seen_at: current_unix_timestamp(),
            away: false,
            away_message: String::new(),
            input_muted: false,
            output_muted: false,
            server_id: 1,
            channel_id: 2,
            client_type: 0,
            version: String::from("BlackTeaSpeak 1.5.6 compat-seed"),
            platform: String::from("macOS"),
            country: String::from("CH"),
            connection_ip: String::from("198.51.100.12"),
            server_groups: vec![8],
            connected_at: current_unix_timestamp(),
            extra_properties: {
                let mut row = BTreeMap::new();
                row.insert(String::from("client_type_exact"), String::from("4"));
                row.insert(String::from("player_state"), String::from("4"));
                row.insert(String::from("player_volume"), String::from("1"));
                row.insert(String::from("client_playlist_id"), String::from("0"));
                row.insert(String::from("client_disabled"), String::from("0"));
                row.insert(
                    String::from("client_flag_notify_song_change"),
                    String::from("0"),
                );
                row.insert(String::from("client_bot_type"), String::from("0"));
                row.insert(String::from("client_uptime_mode"), String::from("0"));
                row
            },
        },
    );

    let channel_group_assignments = vec![
        ChannelGroupAssignment {
            channel_id: 1,
            client_database_id: 40,
            channel_group_id: 10,
        },
        ChannelGroupAssignment {
            channel_id: 1,
            client_database_id: 41,
            channel_group_id: 10,
        },
        ChannelGroupAssignment {
            channel_id: 2,
            client_database_id: 42,
            channel_group_id: 10,
        },
    ];

    let mut music_bots = BTreeMap::new();
    music_bots.insert(
        1,
        MusicBot {
            id: 1,
            server_id: 1,
            client_database_id: 42,
            linked_client_id: Some(12),
            playlist_id: 1,
            current_song_id: None,
            next_song_id: 1,
            state: MusicBotState::Stopped,
            player_volume: String::from("1"),
            playlist_title: String::new(),
            playlist_description: String::new(),
            playlist_flag_delete_played: false,
            playlist_flag_finished: false,
            playlist_replay_mode: 0,
            playlist_max_songs: 0,
            permissions: BTreeMap::new(),
            client_permissions: Vec::new(),
            current_song_started_at_millis: None,
            current_song_progress_millis: 0,
            queue: Vec::new(),
        },
    );

    InMemoryStore {
        query_accounts,
        server_groups,
        channel_groups,
        virtual_servers,
        channels,
        channel_group_assignments,
        channel_client_permissions: Vec::new(),
        client_permissions: Vec::new(),
        conversation_messages: BTreeMap::new(),
        private_messages: BTreeMap::new(),
        tokens: BTreeMap::new(),
        active_bans: BTreeMap::new(),
        online_clients,
        clients,
        music_bots,
        next_query_client_id: 1,
        next_client_database_id: 100,
        next_conversation_timestamp: 1,
        next_ban_id: 1,
        next_token_id: 1,
        next_token_action_id: 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn workspace_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .to_path_buf()
    }

    fn unique_temp_dir(name: &str) -> PathBuf {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after Unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "BlackTeaSpeak-Server-permissions-{name}-{}-{timestamp}",
            std::process::id()
        ))
    }

    fn create_test_runtime(label: &str) -> BaselineRuntime {
        let state_path = unique_temp_dir(label).join("runtime-state.json");
        create_baseline_runtime_with_state_path(workspace_root(), &state_path)
            .expect("runtime should load")
    }

    fn login_query_serveradmin(runtime: &mut BaselineRuntime, client_id: u64) -> QuerySessionState {
        let mut session = QuerySessionState {
            client_id,
            ..QuerySessionState::default()
        };
        assert!(
            runtime
                .execute("login serveradmin serveradmin", &mut session)
                .contains("error id=0 msg=ok")
        );
        assert!(
            runtime
                .execute("use sid=1", &mut session)
                .contains("error id=0 msg=ok")
        );
        session
    }

    fn extract_field<'a>(rendered: &'a str, field_name: &str) -> Option<&'a str> {
        let prefix = format!("{field_name}=");
        rendered
            .split(|character: char| matches!(character, ' ' | '|' | '\n' | '\r' | '\t'))
            .find_map(|segment| segment.strip_prefix(&prefix))
    }

    #[test]
    fn server_admin_group_is_seeded_and_reported_via_instanceinfo() {
        let mut runtime = create_test_runtime("server-admin-seeded");
        let mut session = login_query_serveradmin(&mut runtime, 9001);

        let instanceinfo = runtime.execute("instanceinfo", &mut session);
        assert!(instanceinfo.contains("serverinstance_template_serveradmin_group=9"));

        let groups = runtime.execute("servergrouplist", &mut session);
        assert!(groups.contains("sgid=9"));
        assert!(groups.contains(r"name=Server\sAdmin"));
    }

    #[test]
    fn privilegekeyuse_promotes_normal_online_client_to_server_admin() {
        let mut runtime = create_test_runtime("privilegekey-server-admin");
        let mut admin_session = login_query_serveradmin(&mut runtime, 9001);
        let server_admin_group_id = runtime
            .store
            .server_groups
            .iter()
            .find(|(_, group)| group.name == "Server Admin")
            .map(|(group_id, _)| *group_id)
            .expect("server admin group should be seeded");

        let mut client_session = QuerySessionState {
            client_id: 9002,
            actor_client_database_id_override: Some(40),
            selected_virtual_server_id: Some(1),
            ..QuerySessionState::default()
        };

        let denied_edit = runtime.execute(
            r"serveredit virtualserver_name=Denied\sPromotion",
            &mut client_session,
        );
        assert!(denied_edit.contains("error id=2568"));

        let created_key = runtime.execute(
            &format!(
                r"privilegekeyadd token_description=Server\sAdmin\sGrant token_max_uses=1 action_type=2 action_id1={}",
                server_admin_group_id
            ),
            &mut admin_session,
        );
        let token_value = extract_field(&created_key, "token")
            .unwrap_or_else(|| panic!("privilegekeyadd should expose token, got: {}", created_key))
            .to_string();

        let use_response = runtime.execute(
            &format!("privilegekeyuse token={token_value}"),
            &mut client_session,
        );
        assert!(use_response.contains("error id=0 msg=ok"));

        let groups_after_use = runtime.execute("servergroupsbyclientid cldbid=40", &mut admin_session);
        assert!(groups_after_use.contains(&format!("sgid={server_admin_group_id}")));

        let promoted_edit = runtime.execute(
            r"serveredit virtualserver_name=Promoted\sServer\sAdmin",
            &mut client_session,
        );
        assert!(promoted_edit.contains("error id=0 msg=ok"));

        let serverinfo = runtime.execute("serverinfo", &mut admin_session);
        assert!(serverinfo.contains(r"virtualserver_name=Promoted\sServer\sAdmin"));
    }

    #[test]
    fn persisted_state_backfills_server_admin_group_without_overwriting_existing_group_ids() {
        let state_path = unique_temp_dir("server-admin-backfill").join("runtime-state.json");

        {
            let mut runtime = create_baseline_runtime_with_state_path(workspace_root(), &state_path)
                .expect("runtime should load");
            let mut session = login_query_serveradmin(&mut runtime, 9001);
            assert!(runtime
                .execute(
                    r"serveredit virtualserver_name=Persisted\sServer\sAdmin\sProbe",
                    &mut session,
                )
                .contains("error id=0 msg=ok"));
        }

        let mut persisted: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(&state_path).expect("persisted state should exist"),
        )
        .expect("persisted state should parse");
        let groups = persisted["server_groups"]
            .as_object_mut()
            .expect("server_groups should be an object");
        let mut legacy_group = groups
            .remove("9")
            .expect("fresh state should persist server admin group under 9");
        legacy_group["name"] = serde_json::Value::String(String::from("Legacy Custom"));
        groups.insert(String::from("9"), legacy_group);
        fs::write(
            &state_path,
            serde_json::to_vec_pretty(&persisted).expect("persisted state should serialize"),
        )
        .expect("mutated state should write");

        let runtime = create_baseline_runtime_with_state_path(workspace_root(), &state_path)
            .expect("runtime should reload from mutated state");
        assert_eq!(
            runtime
                .store
                .server_groups
                .get(&9)
                .expect("legacy custom group should remain")
                .name,
            "Legacy Custom"
        );

        let server_admin_group = runtime
            .store
            .server_groups
            .iter()
            .find(|(_, group)| group.name == "Server Admin")
            .expect("server admin group should be backfilled");
        assert_ne!(*server_admin_group.0, 9);
        assert!(!server_admin_group.1.permissions.is_empty());
    }

    #[test]
    fn query_servergroup_commands_support_normal_online_clients() {
        let mut runtime = create_test_runtime("query-servergroup-online-client");
        let mut session = login_query_serveradmin(&mut runtime, 9001);

        let initial_groups = runtime.execute("servergroupsbyclientid cldbid=40", &mut session);
        assert!(initial_groups.contains("sgid=8"));
        assert!(!initial_groups.contains("query account not found"));

        let add_response = runtime.execute("servergroupaddclient sgid=6 cldbid=40", &mut session);
        assert!(add_response.contains("error id=0 msg=ok"));
        assert!(
            runtime
                .store
                .online_clients
                .values()
                .find(|client| client.database_id == 40)
                .expect("online client 40 should exist")
                .server_groups
                .contains(&6)
        );

        let groups_after_add = runtime.execute("servergroupsbyclientid cldbid=40", &mut session);
        assert!(groups_after_add.contains("sgid=6"));
        assert!(groups_after_add.contains("sgid=8"));

        let del_response = runtime.execute("servergroupdelclient sgid=6 cldbid=40", &mut session);
        assert!(del_response.contains("error id=0 msg=ok"));
        let client = runtime
            .store
            .online_clients
            .values()
            .find(|client| client.database_id == 40)
            .expect("online client 40 should exist after delete");
        assert!(!client.server_groups.contains(&6));
        assert!(client.server_groups.contains(&8));

        let groups_after_del = runtime.execute("servergroupsbyclientid cldbid=40", &mut session);
        assert!(!groups_after_del.contains("sgid=6"));
        assert!(groups_after_del.contains("sgid=8"));
        assert!(!groups_after_del.contains("query account not found"));
    }
}
