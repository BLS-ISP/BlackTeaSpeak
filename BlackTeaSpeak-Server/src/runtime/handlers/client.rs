use crate::runtime::BaselineRuntime;
use crate::query::{CommandRequest, QueryResponse};
use crate::runtime::QuerySessionState;
use crate::runtime::*;

impl BaselineRuntime {
    pub(crate) fn handle_clientpoke(
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
        let Some(target_client_id) = request
            .named_args
            .get("clid")
            .and_then(|value| value.parse::<u64>().ok())
        else {
            return QueryResponse::error(512, "clid is required");
        };
        if target_client_id == session.client_id {
            return QueryResponse::error(512, "cannot poke yourself");
        }
        if self.online_client_identity(server_id, target_client_id).is_none() {
            return QueryResponse::error(768, "target client not found");
        }

        QueryResponse::ok()
    }

    pub(crate) fn handle_clientkick(
        &mut self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session)
            && session.actor_client_database_id_override.is_none()
        {
            return QueryResponse::error(521, "login required");
        }

        let Some(server_id) = session.selected_virtual_server_id else {
            return QueryResponse::error(522, "virtual server selection required");
        };
        let Some(target_client_id) = request
            .named_args
            .get("clid")
            .and_then(|value| value.parse::<u64>().ok())
        else {
            return QueryResponse::error(512, "clid is required");
        };
        let Some(reason_id) = request
            .named_args
            .get("reasonid")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return QueryResponse::error(512, "reasonid is required");
        };
        let reason_message = request.named_args.get("reasonmsg").cloned().unwrap_or_default();
        if reason_message.len() > 40 {
            return QueryResponse::error(512, "reasonmsg is too long");
        }

        let (_actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        let (target_snapshot, target_permissions) =
            match self.target_client_snapshot_and_permissions(server_id, target_client_id) {
                Ok(target) => target,
                Err(response) => return response,
            };

        match reason_id {
            4 => {
                if let Some(response) = self.check_target_client_power(
                    &actor_permissions,
                    &target_permissions,
                    &["i_client_kick_from_channel_power"],
                    &["i_client_needed_kick_from_channel_power"],
                    "i_client_kick_from_channel_power",
                ) {
                    return response;
                }

                let Some(default_channel_id) = self.default_channel_id_for_server(server_id) else {
                    return QueryResponse::error(768, "target channel not found");
                };
                let Some(target_client) = self.store.online_clients.get_mut(&target_client_id) else {
                    return QueryResponse::error(768, "target client not found");
                };
                target_client.channel_id = default_channel_id;
                QueryResponse::ok()
            }
            5 => {
                if let Some(response) = self.check_target_client_power(
                    &actor_permissions,
                    &target_permissions,
                    &["i_client_kick_from_server_power"],
                    &["i_client_needed_kick_from_server_power"],
                    "i_client_kick_from_server_power",
                ) {
                    return response;
                }

                self.remove_session_client(target_snapshot.id, 5, reason_message.clone());
                QueryResponse::ok()
            }
            _ => QueryResponse::error(512, "reasonid must be 4 or 5"),
        }
    }

    pub(crate) fn handle_clientfind(
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
        let Some(pattern) = request.named_args.get("pattern") else {
            return QueryResponse::error(512, "pattern is required");
        };
        let pattern = pattern.to_ascii_lowercase();

        let rows = self
            .store
            .online_clients
            .values()
            .filter(|client| client.server_id == server_id)
            .filter(|client| client.nickname.to_ascii_lowercase().contains(&pattern))
            .map(|client| {
                let mut row = BTreeMap::new();
                row.insert(String::from("clid"), client.id.to_string());
                row.insert(String::from("client_nickname"), client.nickname.clone());
                row
            })
            .collect::<Vec<_>>();

        QueryResponse::ok_rows(rows)
    }

    pub(crate) fn handle_clientgetids(
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
        let Some(client_uid) = request.named_args.get("cluid") else {
            return QueryResponse::error(512, "cluid is required");
        };

        let rows = self
            .store
            .online_clients
            .values()
            .filter(|client| {
                client.server_id == server_id && client.unique_identifier == *client_uid
            })
            .map(|client| {
                let mut row = BTreeMap::new();
                row.insert(String::from("clid"), client.id.to_string());
                row.insert(String::from("cluid"), client.unique_identifier.clone());
                row.insert(String::from("name"), client.nickname.clone());
                row
            })
            .collect::<Vec<_>>();

        QueryResponse::ok_rows(rows)
    }

    pub(crate) fn handle_clientgetdbidfromuid(
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
        let Some(client_uid) = request.named_args.get("cluid") else {
            return QueryResponse::error(512, "cluid is required");
        };
        let Some((resolved_uid, client_database_id, _)) =
            self.lookup_client_identity_by_uid(server_id, client_uid)
        else {
            return QueryResponse::error(768, "client not found");
        };

        let mut row = BTreeMap::new();
        row.insert(String::from("cldbid"), client_database_id.to_string());
        row.insert(String::from("cluid"), resolved_uid);
        QueryResponse::ok_row(row)
    }

    pub(crate) fn handle_clientgetnamefromdbid(
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
        let Some(client_database_id) = request
            .named_args
            .get("cldbid")
            .and_then(|value| value.parse::<u64>().ok())
        else {
            return QueryResponse::error(512, "cldbid is required");
        };
        let Some((client_uid, resolved_database_id, name)) =
            self.lookup_client_identity_by_dbid(server_id, client_database_id)
        else {
            return QueryResponse::error(768, "client not found");
        };

        let mut row = BTreeMap::new();
        row.insert(String::from("cldbid"), resolved_database_id.to_string());
        row.insert(String::from("cluid"), client_uid);
        row.insert(String::from("name"), name);
        QueryResponse::ok_row(row)
    }

    pub(crate) fn handle_clientgetnamefromuid(
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
        let Some(client_uid) = request.named_args.get("cluid") else {
            return QueryResponse::error(512, "cluid is required");
        };
        let Some((resolved_uid, client_database_id, name)) =
            self.lookup_client_identity_by_uid(server_id, client_uid)
        else {
            return QueryResponse::error(768, "client not found");
        };

        let mut row = BTreeMap::new();
        row.insert(String::from("cldbid"), client_database_id.to_string());
        row.insert(String::from("cluid"), resolved_uid);
        row.insert(String::from("name"), name);
        QueryResponse::ok_row(row)
    }

    pub(crate) fn handle_clientgetuidfromclid(
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
        let Some(client_id) = request
            .named_args
            .get("clid")
            .and_then(|value| value.parse::<u64>().ok())
        else {
            return QueryResponse::error(512, "clid is required");
        };
        let Some(client) = self.online_client_by_id_in_server(server_id, client_id) else {
            return QueryResponse::error(768, "client not found");
        };

        let mut row = BTreeMap::new();
        row.insert(String::from("clid"), client.id.to_string());
        row.insert(String::from("cluid"), client.unique_identifier.clone());
        row.insert(String::from("nickname"), client.nickname.clone());
        QueryResponse::ok_row(row)
    }

    pub(crate) fn handle_clientlist(
        &self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) && !session.is_desktop_client {
            return QueryResponse::error(521, "login required");
        }

        let Some(server_id) = session.selected_virtual_server_id else {
            return QueryResponse::error(522, "virtual server selection required");
        };

        let rows = self
            .store
            .online_clients
            .values()
            .filter(|client| client.server_id == server_id)
            .map(|client| self.render_client_row(client, request, false))
            .collect::<Vec<_>>();
        QueryResponse::ok_rows(rows)
    }

    pub(crate) fn handle_clientinfo(
        &self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }

        if session.selected_virtual_server_id.is_none() {
            return QueryResponse::error(522, "virtual server selection required");
        }

        let Some(client_id) = request
            .named_args
            .get("clid")
            .and_then(|value| value.parse::<u64>().ok())
        else {
            return QueryResponse::error(512, "clid is required");
        };
        let Some(client) = self.store.online_clients.get(&client_id) else {
            return QueryResponse::error(768, "client not found");
        };

        QueryResponse::ok_row(self.render_client_row(client, request, true))
    }

    pub(crate) fn handle_clientupdate(
        &mut self,
        request: &CommandRequest,
        session: &mut QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }

        if request.named_args.is_empty() {
            return QueryResponse::error(512, "at least one client property is required");
        }

        let mut applied = false;

        if let Some(client_nickname) = request.named_args.get("client_nickname") {
            session.client_nickname = client_nickname.clone();
            applied = true;
        }

        if let Some(client_away) = request.named_args.get("client_away") {
            let Some(client_away) = parse_query_bool(client_away) else {
                return QueryResponse::error(512, "client_away must be 0 or 1");
            };
            session.client_away = client_away;
            applied = true;
        }

        if let Some(client_away_message) = request.named_args.get("client_away_message") {
            session.client_away_message = client_away_message.clone();
            applied = true;
        }

        if let Some(client_input_muted) = request.named_args.get("client_input_muted") {
            let Some(client_input_muted) = parse_query_bool(client_input_muted) else {
                return QueryResponse::error(512, "client_input_muted must be 0 or 1");
            };
            session.client_input_muted = client_input_muted;
            applied = true;
        }

        if let Some(client_output_muted) = request.named_args.get("client_output_muted") {
            let Some(client_output_muted) = parse_query_bool(client_output_muted) else {
                return QueryResponse::error(512, "client_output_muted must be 0 or 1");
            };
            session.client_output_muted = client_output_muted;
            applied = true;
        }

        let mut avatar_changed = false;
        if let Some(client_flag_avatar) = request.named_args.get("client_flag_avatar") {
            if let Some(online_client) = self.store.online_clients.get(&session.client_id) {
                let db_id = online_client.database_id;
                if let Some(client) = self.store.clients.get_mut(&db_id) {
                    client.client_flag_avatar = client_flag_avatar.clone();
                    let _ = self.store.db.update_client_avatar(&client.unique_identifier, client_flag_avatar);
                }
            }
            avatar_changed = true;
            applied = true;
        }

        if !applied {
            return QueryResponse::error(512, "no supported client properties provided");
        }

        QueryResponse::ok()
    }

    pub(crate) fn handle_clientmove(
        &mut self,
        request: &CommandRequest,
        session: &mut QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session)
            && session.actor_client_database_id_override.is_none()
        {
            return QueryResponse::error(521, "login required");
        }

        let Some(server_id) = session.selected_virtual_server_id else {
            return QueryResponse::error(522, "virtual server selection required");
        };
        let Some(target_channel_id) = request
            .named_args
            .get("cid")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return QueryResponse::error(512, "cid is required");
        };

        if let Some(requested_client_id) = request
            .named_args
            .get("clid")
            .and_then(|value| value.parse::<u64>().ok())
            && requested_client_id != session.client_id
        {
            let (_actor, actor_permissions) = match self.query_actor_effective_permissions(session)
            {
                Ok(actor) => actor,
                Err(response) => return response,
            };
            let (_target_snapshot, target_permissions) =
                match self.target_client_snapshot_and_permissions(server_id, requested_client_id) {
                    Ok(target) => target,
                    Err(response) => return response,
                };
            if let Some(response) = self.check_target_client_power(
                &actor_permissions,
                &target_permissions,
                &["i_client_move_power"],
                &["i_client_needed_move_power"],
                "i_client_move_power",
            ) {
                return response;
            }

            let Some(target_client) = self.store.online_clients.get_mut(&requested_client_id) else {
                return QueryResponse::error(768, "target client not found");
            };
            target_client.channel_id = target_channel_id;
            return QueryResponse::ok();
        }

        let Some(channels) = self.store.channels.get(&server_id) else {
            return QueryResponse::error(768, "virtual server channels not found");
        };
        if !channels
            .iter()
            .any(|channel| channel.id == target_channel_id)
        {
            return QueryResponse::error(768, "target channel not found");
        }

        if let Some(target_client) = self.store.online_clients.get_mut(&session.client_id) {
            target_client.channel_id = target_channel_id;
        }
        session.current_channel_id = Some(target_channel_id);
        QueryResponse::ok()
    }

    pub(crate) fn handle_clientsetserverquerylogin(
        &mut self,
        request: &CommandRequest,
        session: &mut QuerySessionState,
    ) -> QueryResponse {
        let Some(current_login) = session.authenticated_login.clone() else {
            return QueryResponse::error(521, "login required");
        };
        let (_actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        if let Err(permission_name) = check_required_permission(
            &actor_permissions,
            &["b_client_create_modify_serverquery_login"],
            "b_client_create_modify_serverquery_login",
        ) {
            return self.insufficient_permission_response(permission_name);
        }
        let Some(new_login_name) = request.named_args.get("client_login_name").cloned() else {
            return QueryResponse::error(512, "client_login_name is required");
        };

        if self.store.query_accounts.contains_key(&new_login_name) {
            return QueryResponse::error(769, "target query account already exists");
        }

        let Some(mut account) = self.store.query_accounts.remove(&current_login) else {
            return QueryResponse::error(768, "current query account not found");
        };

        account.login_name = new_login_name.clone();
        account.password = format!("generated-{}", new_login_name);
        session.authenticated_login = Some(new_login_name.clone());
        self.store
            .query_accounts
            .insert(new_login_name.clone(), account.clone());
        if let Some(snapshot) = self.session_snapshots.remove(&current_login) {
            self.session_snapshots
                .insert(new_login_name.clone(), snapshot);
        }

        let mut row = BTreeMap::new();
        row.insert(String::from("client_login_name"), new_login_name);
        row.insert(String::from("client_login_password"), account.password);
        QueryResponse::ok_row(row)
    }

}
