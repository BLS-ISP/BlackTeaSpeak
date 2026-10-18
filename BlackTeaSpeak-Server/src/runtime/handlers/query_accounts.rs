use crate::runtime::BaselineRuntime;
use crate::query::{CommandRequest, QueryResponse};
use crate::runtime::QuerySessionState;
use crate::runtime::*;

impl BaselineRuntime {
    pub(crate) fn handle_querylist(
        &self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }

        let current_login = session.authenticated_login.as_deref();
        let current_actor_database_id = session.actor_client_database_id_override;

        let requested_server_id = match request.named_args.get("server_id") {
            Some(value) => match value.parse::<i32>() {
                Ok(server_id) => server_id,
                Err(_) => return QueryResponse::error(512, "server_id must be an integer"),
            },
            None => -1,
        };
        let list_all_servers = requested_server_id < 0;

        let rows = self
            .store
            .query_accounts
            .values()
            .filter(|account| {
                list_all_servers || account.server_id == Some(requested_server_id as u32)
            })
            .map(|account| {
                let mut row = BTreeMap::new();
                row.insert(
                    String::from("client_bounded_server"),
                    account.server_id.unwrap_or(0).to_string(),
                );
                row.insert(
                    String::from("client_login_name"),
                    account.login_name.clone(),
                );
                row.insert(
                    String::from("client_unique_identifier"),
                    self.query_account_unique_identifier(account),
                );
                row.insert(
                    String::from("flag_all"),
                    if list_all_servers {
                        String::from("1")
                    } else {
                        String::from("0")
                    },
                );
                row.insert(
                    String::from("flag_own"),
                    if current_login.is_some_and(|login| account.login_name == login)
                        || current_actor_database_id
                            .is_some_and(|database_id| {
                                account.client_database_id == Some(database_id)
                            })
                    {
                        String::from("1")
                    } else {
                        String::from("0")
                    },
                );
                row.insert(String::from("server_id"), requested_server_id.to_string());
                row
            })
            .collect::<Vec<_>>();

        QueryResponse::ok_rows(rows)
    }

    pub(crate) fn handle_querycreate(
        &mut self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }
        let (actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };

        let Some(login_name) = request.named_args.get("client_login_name").cloned() else {
            return QueryResponse::error(512, "client_login_name is required");
        };

        if self.store.query_accounts.contains_key(&login_name) {
            return QueryResponse::error(769, "query account already exists");
        }

        let password = request
            .named_args
            .get("client_login_password")
            .cloned()
            .unwrap_or_else(|| format!("generated-{}", login_name));
        let server_id = request
            .named_args
            .get("server_id")
            .and_then(|value| value.parse::<u32>().ok());
        let client_database_id = request
            .named_args
            .get("cldbid")
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or_else(|| self.allocate_client_database_id());
        let creating_own_identity = client_database_id == actor.client_database_id;
        let required_permission_name = if creating_own_identity {
            match check_required_permission(
                &actor_permissions,
                &["b_client_query_create", "b_client_query_create_own"],
                "b_client_query_create_own",
            ) {
                Ok(()) => None,
                Err(permission_name) => Some(permission_name),
            }
        } else {
            match check_required_permission(
                &actor_permissions,
                &["b_client_query_create"],
                "b_client_query_create",
            ) {
                Ok(()) => None,
                Err(permission_name) => Some(permission_name),
            }
        };
        if let Some(permission_name) = required_permission_name {
            return self.insufficient_permission_response(permission_name);
        }

        self.store.query_accounts.insert(
            login_name.clone(),
            QueryAccount {
                login_name: login_name.clone(),
                password: password.clone(),
                server_id,
                client_database_id: Some(client_database_id),
                server_groups: self.default_server_groups_for_new_query_account(),
                permissions: BTreeMap::new(),
            },
        );
        let now = current_unix_timestamp();
        let client_record = Client {
            database_id: client_database_id,
            unique_identifier: login_name.clone(),
            nickname: login_name.clone(),
            description: String::new(),
            created_at: now,
            last_connected_at: now,
            total_connections: 1,
            month_bytes_uploaded: 0,
            month_bytes_downloaded: 0,
            total_bytes_uploaded: 0,
            total_bytes_downloaded: 0,
            client_flag_avatar: String::new(),
        };
        let _ = self.db.save_client(&client_record);
        self.store
            .clients
            .entry(client_database_id)
            .or_insert(client_record);

        let mut row = BTreeMap::new();
        row.insert(String::from("client_login_name"), login_name);
        row.insert(String::from("client_login_password"), password);
        row.insert(String::from("cldbid"), client_database_id.to_string());
        if let Some(server_id) = server_id {
            row.insert(String::from("client_bounded_server"), server_id.to_string());
        }
        QueryResponse::ok_row(row)
    }

    pub(crate) fn handle_queryrename(
        &mut self,
        request: &CommandRequest,
        session: &mut QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }
        let (actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };

        let Some(login_name) = request.named_args.get("client_login_name").cloned() else {
            return QueryResponse::error(512, "client_login_name is required");
        };
        let Some(new_login_name) = request.named_args.get("client_new_login_name").cloned() else {
            return QueryResponse::error(512, "client_new_login_name is required");
        };

        if self.store.query_accounts.contains_key(&new_login_name) {
            return QueryResponse::error(769, "target query account already exists");
        }

        let Some(target_account) = self.store.query_accounts.get(&login_name) else {
            return QueryResponse::error(768, "query account not found");
        };
        let renaming_own_identity =
            target_account.client_database_id == Some(actor.client_database_id);
        let required_permission_name = if renaming_own_identity {
            match check_required_permission(
                &actor_permissions,
                &["b_client_query_rename", "b_client_query_rename_own"],
                "b_client_query_rename_own",
            ) {
                Ok(()) => None,
                Err(permission_name) => Some(permission_name),
            }
        } else {
            match check_required_permission(
                &actor_permissions,
                &["b_client_query_rename"],
                "b_client_query_rename",
            ) {
                Ok(()) => None,
                Err(permission_name) => Some(permission_name),
            }
        };
        if let Some(permission_name) = required_permission_name {
            return self.insufficient_permission_response(permission_name);
        }

        let Some(mut account) = self.store.query_accounts.remove(&login_name) else {
            return QueryResponse::error(768, "query account not found");
        };
        account.login_name = new_login_name.clone();
        self.store
            .query_accounts
            .insert(new_login_name.clone(), account);
        if let Some(snapshot) = self.session_snapshots.remove(&login_name) {
            self.session_snapshots
                .insert(new_login_name.clone(), snapshot);
        }

        let mut row = BTreeMap::new();
        row.insert(String::from("client_login_name"), new_login_name.clone());
        if session.authenticated_login.as_deref() == Some(login_name.as_str()) {
            session.authenticated_login = Some(new_login_name.clone());
            row.insert(String::from("renamed_current_login"), String::from("1"));
        }
        QueryResponse::ok_row(row)
    }

    pub(crate) fn handle_querychangepassword(
        &mut self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }
        let (actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };

        let Some(login_name) = request.named_args.get("client_login_name").cloned() else {
            return QueryResponse::error(512, "client_login_name is required");
        };

        let Some(target_account) = self.store.query_accounts.get(&login_name) else {
            return QueryResponse::error(768, "query account not found");
        };
        let changing_own_identity =
            target_account.client_database_id == Some(actor.client_database_id);
        let required_permission_name = if changing_own_identity {
            match check_required_permission(
                &actor_permissions,
                &[
                    "b_client_query_change_password",
                    "b_client_query_change_own_password",
                ],
                "b_client_query_change_own_password",
            ) {
                Ok(()) => None,
                Err(permission_name) => Some(permission_name),
            }
        } else {
            match check_required_permission(
                &actor_permissions,
                &["b_client_query_change_password"],
                "b_client_query_change_password",
            ) {
                Ok(()) => None,
                Err(permission_name) => Some(permission_name),
            }
        };
        if let Some(permission_name) = required_permission_name {
            return self.insufficient_permission_response(permission_name);
        }

        let Some(account) = self.store.query_accounts.get_mut(&login_name) else {
            return QueryResponse::error(768, "query account not found");
        };

        let next_secret = request
            .named_args
            .get("client_login_password")
            .cloned()
            .unwrap_or_else(|| format!("generated-{}", login_name));
        account.password = next_secret.clone();

        let mut row = BTreeMap::new();
        row.insert(String::from("client_login_name"), login_name);
        row.insert(String::from("client_login_password"), next_secret);
        QueryResponse::ok_row(row)
    }

    pub(crate) fn handle_querydelete(
        &mut self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }
        let (actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };

        let Some(login_name) = request.named_args.get("client_login_name").cloned() else {
            return QueryResponse::error(512, "client_login_name is required");
        };

        let Some(target_account) = self.store.query_accounts.get(&login_name) else {
            return QueryResponse::error(768, "query account not found");
        };
        let deleting_own_identity =
            target_account.client_database_id == Some(actor.client_database_id);
        let required_permission_name = if deleting_own_identity {
            match check_required_permission(
                &actor_permissions,
                &["b_client_query_delete", "b_client_query_delete_own"],
                "b_client_query_delete_own",
            ) {
                Ok(()) => None,
                Err(permission_name) => Some(permission_name),
            }
        } else {
            match check_required_permission(
                &actor_permissions,
                &["b_client_query_delete"],
                "b_client_query_delete",
            ) {
                Ok(()) => None,
                Err(permission_name) => Some(permission_name),
            }
        };
        if let Some(permission_name) = required_permission_name {
            return self.insufficient_permission_response(permission_name);
        }

        self.session_snapshots.remove(&login_name);
        match self.store.query_accounts.remove(&login_name) {
            Some(account) => {
                if let Some(client_database_id) = account.client_database_id {
                    self.store
                        .channel_group_assignments
                        .retain(|assignment| assignment.client_database_id != client_database_id);
                    self.store
                        .channel_client_permissions
                        .retain(|target| target.client_database_id != client_database_id);
                }
                QueryResponse::ok()
            }
            None => QueryResponse::error(768, "query account not found"),
        }
    }


}
