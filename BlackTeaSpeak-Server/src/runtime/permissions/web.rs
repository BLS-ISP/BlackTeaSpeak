use super::*;

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

    pub(crate) fn client_permission_target(&self, server_id: u32, client_database_id: u64) -> Option<&ClientPermissionTarget> {
        self.store
            .client_permissions
            .iter()
            .find(|target| target.server_id == server_id && target.client_database_id == client_database_id)
    }

    pub(crate) fn ensure_client_permission_target_mut(
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

    pub(crate) fn web_change_server_group_client(
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
                self.persist_state_best_effort();
                return Ok(());
            }

            let updated_groups = {
                let Some(client) = self.online_client_by_cldbid_mut(server_id, client_database_id)
                else {
                    return Err(WebServerGroupMutationError::InvalidClient);
                };
                if !client.server_groups.contains(&group_id) {
                    client.server_groups.push(group_id);
                    client.server_groups.sort_unstable();
                }
                client.server_groups.clone()
            };
            self.persist_web_client_server_groups(client_database_id, &updated_groups);
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
            self.persist_state_best_effort();
            return Ok(());
        }

        let updated_groups = {
            let Some(client) = self.online_client_by_cldbid_mut(server_id, client_database_id)
            else {
                return Err(WebServerGroupMutationError::InvalidClient);
            };
            client
                .server_groups
                .retain(|existing_group_id| *existing_group_id != group_id);
            if client.server_groups.is_empty() {
                client.server_groups.push(8);
            }
            client.server_groups.sort_unstable();
            client.server_groups.dedup();
            client.server_groups.clone()
        };
        self.persist_web_client_server_groups(client_database_id, &updated_groups);
        Ok(())
    }

    pub(crate) fn ensure_web_server_group_assignment_permission_basis(&mut self) {
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

    pub(crate) fn effective_channel_group(
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

    pub fn effective_channel_group_id_for_client(
        &self,
        _server_id: u32,
        channel_id: u32,
        client_database_id: u64,
    ) -> u32 {
        self.effective_channel_group(channel_id, client_database_id)
            .map(|group| group.id)
            .or_else(|| self.default_channel_group_id())
            .unwrap_or(8)
    }

    pub(crate) fn default_channel_group_id(&self) -> Option<u32> {
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
}
