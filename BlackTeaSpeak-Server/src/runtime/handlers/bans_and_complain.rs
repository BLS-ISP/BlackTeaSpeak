use crate::runtime::BaselineRuntime;
use crate::query::{CommandRequest, QueryResponse};
use crate::runtime::QuerySessionState;
use crate::runtime::*;

impl BaselineRuntime {
    pub(crate) fn handle_banclient(
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
        let requested_ban_time = request
            .named_args
            .get("time")
            .and_then(|value| value.parse::<u32>().ok())
            .unwrap_or(0);
        let ban_reason = request.named_args.get("banreason").cloned().unwrap_or_default();
        if ban_reason.len() > 40 {
            return QueryResponse::error(512, "banreason is too long");
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

        if let Some(response) = self.check_target_client_power(
            &actor_permissions,
            &target_permissions,
            &["i_client_ban_power"],
            &["i_client_needed_ban_power"],
            "i_client_ban_power",
        ) {
            return response;
        }

        let max_ban_time =
            permission_value_or_default(&actor_permissions, &["i_client_ban_max_bantime"]);
        if max_ban_time > 0 && i64::from(requested_ban_time) > max_ban_time {
            return self.insufficient_permission_response("i_client_ban_max_bantime");
        }

        let ban_id = self.register_active_ban(&target_snapshot, requested_ban_time, ban_reason.clone());
        self.remove_session_client_without_left_event(target_snapshot.id);

        let mut row = BTreeMap::new();
        row.insert(String::from("banid"), ban_id.to_string());
        QueryResponse::ok_row(row)
    }

    pub(crate) fn handle_banlist(&mut self, session: &QuerySessionState) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }
        let Some(server_id) = session.selected_virtual_server_id else {
            return QueryResponse::error(522, "virtual server selection required");
        };

        if let Ok((_actor, actor_permissions)) = self.query_actor_effective_permissions(session) {
            if let Err(permission_name) = check_required_permission(&actor_permissions, &["b_client_ban_list"], "b_client_ban_list") {
                return self.insufficient_permission_response(permission_name);
            }
        } else {
            return QueryResponse::error(512, "insufficient permissions");
        }

        self.prune_expired_active_bans();

        let rows = self.store.active_bans.values()
            .filter(|ban| ban.server_id == server_id)
            .map(|ban| {
                let mut row = BTreeMap::new();
                row.insert("banid".to_string(), ban.id.to_string());
                row.insert("ip".to_string(), ban.ip.clone());
                row.insert("name".to_string(), ban.name.clone());
                row.insert("uid".to_string(), ban.unique_identifier.clone());
                row.insert("hwid".to_string(), ban.hardware_identifier.clone());
                row.insert("invokercldbid".to_string(), ban.invoker_database_id.to_string());
                row.insert("invokeruid".to_string(), ban.invoker_unique_identifier.clone());
                row.insert("invokername".to_string(), ban.invoker_name.clone());
                row.insert("created".to_string(), ban.created_at.to_string());
                row.insert("duration".to_string(), ban.duration_seconds.to_string());
                row.insert("banreason".to_string(), ban.reason.clone());
                row.insert("reason".to_string(), ban.reason.clone());
                row.insert("enforcements".to_string(), "0".to_string());
                row
            })
            .collect::<Vec<_>>();

        QueryResponse::ok_rows(rows)
    }

    pub(crate) fn handle_banadd(&mut self, request: &CommandRequest, session: &QuerySessionState) -> QueryResponse {
        if !session_has_permission_actor(session) && session.actor_client_database_id_override.is_none() {
            return QueryResponse::error(521, "login required");
        }
        let Some(server_id) = session.selected_virtual_server_id else {
            return QueryResponse::error(522, "virtual server selection required");
        };

        let ip = request.named_args.get("ip").cloned().unwrap_or_default();
        let name = request.named_args.get("name").cloned().unwrap_or_default();
        let uid = request.named_args.get("uid").cloned().unwrap_or_default();
        let hwid = request.named_args.get("hwid").cloned().unwrap_or_default();
        
        if ip.is_empty() && name.is_empty() && uid.is_empty() && hwid.is_empty() {
            return QueryResponse::error(512, "missing target definition");
        }

        let requested_ban_time = request.named_args.get("time").and_then(|v| v.parse::<u32>().ok()).unwrap_or(0);
        let ban_reason = request.named_args.get("banreason").cloned().unwrap_or_default();
        if ban_reason.len() > 40 {
            return QueryResponse::error(512, "banreason is too long");
        }

        let (actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };

        if let Err(permission_name) = check_required_permission(&actor_permissions, &["b_client_ban_create"], "b_client_ban_create") {
            return self.insufficient_permission_response(permission_name);
        }

        let max_ban_time = permission_value_or_default(&actor_permissions, &["i_client_ban_max_bantime"]);
        if max_ban_time > 0 && i64::from(requested_ban_time) > max_ban_time {
            return self.insufficient_permission_response("i_client_ban_max_bantime");
        }

        let (invoker_uid, invoker_name) = self.lookup_client_identity_by_dbid(server_id, actor.client_database_id)
            .map(|(uid, _, name)| (uid, name))
            .unwrap_or_else(|| (String::from("Unknown"), String::from("ServerQuery")));

        let banid = self.create_manual_active_ban(
            server_id,
            name,
            uid,
            hwid,
            ip,
            ban_reason,
            requested_ban_time,
            invoker_name,
            actor.client_database_id,
            invoker_uid,
        );
        
        let mut row = BTreeMap::new();
        row.insert("banid".to_string(), banid.to_string());
        QueryResponse::ok_row(row)
    }

    pub(crate) fn handle_bandel(&mut self, request: &CommandRequest, session: &QuerySessionState) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }
        let Some(server_id) = session.selected_virtual_server_id else {
            return QueryResponse::error(522, "virtual server selection required");
        };

        let Some(banid) = request.named_args.get("banid").and_then(|v| v.parse::<u32>().ok()) else {
            return QueryResponse::error(512, "banid is required");
        };

        if let Ok((_actor, actor_permissions)) = self.query_actor_effective_permissions(session) {
            if let Err(permission_name) = check_required_permission(&actor_permissions, &["b_client_ban_delete"], "b_client_ban_delete") {
                return self.insufficient_permission_response(permission_name);
            }
        } else {
            return QueryResponse::error(512, "insufficient permissions");
        }

        if self.remove_active_ban(banid, Some(server_id)) {
            QueryResponse::ok()
        } else {
            QueryResponse::error(1281, "invalid ban id")
        }
    }

    pub(crate) fn handle_bandelall(&mut self, session: &QuerySessionState) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }
        let Some(server_id) = session.selected_virtual_server_id else {
            return QueryResponse::error(522, "virtual server selection required");
        };

        if let Ok((_actor, actor_permissions)) = self.query_actor_effective_permissions(session) {
            if let Err(permission_name) = check_required_permission(&actor_permissions, &["b_client_ban_delete"], "b_client_ban_delete") {
                return self.insufficient_permission_response(permission_name);
            }
        } else {
            return QueryResponse::error(512, "insufficient permissions");
        }

        let ban_ids: Vec<u32> = self.store.active_bans.values()
            .filter(|ban| ban.server_id == server_id)
            .map(|ban| ban.id)
            .collect();
            
        for banid in ban_ids {
            self.remove_active_ban(banid, Some(server_id));
        }

        QueryResponse::ok()
    }

    pub(crate) fn handle_complainadd(
        &mut self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session)
            && session.actor_client_database_id_override.is_none()
        {
            return QueryResponse::error(521, "login required");
        }
        let server_id = session.selected_virtual_server_id.unwrap_or(1);

        let Some(tcldbid) = request
            .named_args
            .get("tcldbid")
            .and_then(|v| v.parse::<u64>().ok())
        else {
            return QueryResponse::error(512, "tcldbid is required");
        };
        let Some(message) = request
            .named_args
            .get("message")
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        else {
            return QueryResponse::error(512, "message is required");
        };

        let (actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        if tcldbid == actor.client_database_id {
            return QueryResponse::error(512, "cannot complain about yourself");
        }

        let Some((target_uid, _, target_name)) =
            self.lookup_client_identity_by_dbid(server_id, tcldbid)
        else {
            return QueryResponse::error(768, "target client not found");
        };

        let default_cid = self.default_channel_id_for_server(server_id).unwrap_or(1);
        let target_permissions = self
            .effective_permissions_for_client(server_id, default_cid, tcldbid)
            .unwrap_or_default();

        if let Some(response) = self.check_target_client_power(
            &actor_permissions,
            &target_permissions,
            &["i_client_complain_power"],
            &["i_client_needed_complain_power"],
            "i_client_complain_power",
        ) {
            return response;
        }

        if self.store.complaints.iter().any(|c| {
            c.server_id == server_id
                && c.target_database_id == tcldbid
                && c.from_database_id == actor.client_database_id
        }) {
            return QueryResponse::error(2051, "complaint already exists");
        }

        let from_participant = self.query_session_participant(session);
        self.store.complaints.push(ClientComplaint {
            server_id,
            target_database_id: tcldbid,
            target_name: target_name.clone(),
            from_database_id: actor.client_database_id,
            from_name: from_participant.nickname,
            message: message.to_string(),
            timestamp: current_unix_timestamp(),
        });

        let autoban_count = self
            .store
            .instance_properties
            .get(&format!(
                "vs:{server_id}:virtualserver_complain_autoban_count"
            ))
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(5);
        let autoban_time = self
            .store
            .instance_properties
            .get(&format!(
                "vs:{server_id}:virtualserver_complain_autoban_time"
            ))
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(1200);

        if autoban_count > 0 {
            let current_complaints = self
                .store
                .complaints
                .iter()
                .filter(|c| c.server_id == server_id && c.target_database_id == tcldbid)
                .count();
            if current_complaints >= autoban_count {
                self.store.complaints.retain(|c| {
                    !(c.server_id == server_id && c.target_database_id == tcldbid)
                });
                let online_target = self
                    .store
                    .online_clients
                    .values()
                    .find(|c| c.server_id == server_id && c.database_id == tcldbid)
                    .map(|c| (c.id, c.connection_ip.clone()));
                let target_ip = online_target
                    .as_ref()
                    .map(|(_, ip)| ip.clone())
                    .unwrap_or_default();
                self.create_manual_active_ban(
                    server_id,
                    target_name,
                    target_uid,
                    String::new(),
                    target_ip,
                    String::from("Too many complaints"),
                    autoban_time,
                    String::from("Server"),
                    0,
                    String::from("Server"),
                );
                if let Some((target_clid, _)) = online_target {
                    self.remove_session_client(
                        target_clid,
                        6,
                        String::from("Too many complaints"),
                    );
                }
            }
        }

        QueryResponse::ok()
    }

    pub(crate) fn handle_complainlist(
        &self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session)
            && session.actor_client_database_id_override.is_none()
        {
            return QueryResponse::error(521, "login required");
        }
        let server_id = session.selected_virtual_server_id.unwrap_or(1);

        let (_actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        if let Err(permission_name) = check_required_permission(
            &actor_permissions,
            &["b_client_complain_list"],
            "b_client_complain_list",
        ) {
            return self.insufficient_permission_response(permission_name);
        }

        let filter_tcldbid = match request.named_args.get("tcldbid") {
            Some(val) => match val.parse::<u64>() {
                Ok(id) => Some(id),
                Err(_) => return QueryResponse::error(512, "tcldbid must be an integer"),
            },
            None => None,
        };

        let rows = self
            .store
            .complaints
            .iter()
            .filter(|c| {
                c.server_id == server_id
                    && filter_tcldbid.is_none_or(|id| c.target_database_id == id)
            })
            .map(|c| {
                let mut row = BTreeMap::new();
                row.insert(String::from("tcldbid"), c.target_database_id.to_string());
                row.insert(String::from("tname"), c.target_name.clone());
                row.insert(String::from("fcldbid"), c.from_database_id.to_string());
                row.insert(String::from("fname"), c.from_name.clone());
                row.insert(String::from("message"), c.message.clone());
                row.insert(String::from("timestamp"), c.timestamp.to_string());
                row
            })
            .collect::<Vec<_>>();

        if rows.is_empty() {
            return QueryResponse::error(
                ERROR_DATABASE_EMPTY_RESULT,
                "database empty result set",
            );
        }

        QueryResponse::ok_rows(rows)
    }

    pub(crate) fn handle_complaindel(
        &mut self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session)
            && session.actor_client_database_id_override.is_none()
        {
            return QueryResponse::error(521, "login required");
        }
        let server_id = session.selected_virtual_server_id.unwrap_or(1);

        let Some(tcldbid) = request
            .named_args
            .get("tcldbid")
            .and_then(|v| v.parse::<u64>().ok())
        else {
            return QueryResponse::error(512, "tcldbid is required");
        };
        let Some(fcldbid) = request
            .named_args
            .get("fcldbid")
            .and_then(|v| v.parse::<u64>().ok())
        else {
            return QueryResponse::error(512, "fcldbid is required");
        };

        let (actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };

        if fcldbid == actor.client_database_id {
            if check_required_permission(
                &actor_permissions,
                &["b_client_complain_delete_own", "b_client_complain_delete"],
                "b_client_complain_delete_own",
            )
            .is_err()
            {
                return self.insufficient_permission_response("b_client_complain_delete_own");
            }
        } else if let Err(permission_name) = check_required_permission(
            &actor_permissions,
            &["b_client_complain_delete"],
            "b_client_complain_delete",
        ) {
            return self.insufficient_permission_response(permission_name);
        }

        let before_len = self.store.complaints.len();
        self.store.complaints.retain(|c| {
            !(c.server_id == server_id
                && c.target_database_id == tcldbid
                && c.from_database_id == fcldbid)
        });
        if self.store.complaints.len() == before_len {
            return QueryResponse::error(
                ERROR_DATABASE_EMPTY_RESULT,
                "database empty result set",
            );
        }

        QueryResponse::ok()
    }

    pub(crate) fn handle_complaindelall(
        &mut self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session)
            && session.actor_client_database_id_override.is_none()
        {
            return QueryResponse::error(521, "login required");
        }
        let server_id = session.selected_virtual_server_id.unwrap_or(1);

        let Some(tcldbid) = request
            .named_args
            .get("tcldbid")
            .and_then(|v| v.parse::<u64>().ok())
        else {
            return QueryResponse::error(512, "tcldbid is required");
        };

        let (_actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        if let Err(permission_name) = check_required_permission(
            &actor_permissions,
            &["b_client_complain_delete"],
            "b_client_complain_delete",
        ) {
            return self.insufficient_permission_response(permission_name);
        }

        let before_len = self.store.complaints.len();
        self.store
            .complaints
            .retain(|c| !(c.server_id == server_id && c.target_database_id == tcldbid));
        if self.store.complaints.len() == before_len {
            return QueryResponse::error(
                ERROR_DATABASE_EMPTY_RESULT,
                "database empty result set",
            );
        }

        QueryResponse::ok()
    }

    pub(crate) fn handle_bantriggerlist(
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

        let (_actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        if let Err(permission_name) = check_required_permission(
            &actor_permissions,
            &["b_client_ban_trigger_list", "b_client_ban_list"],
            "b_client_ban_list",
        ) {
            return self.insufficient_permission_response(permission_name);
        }

        let target_ban_ids = if let Some(banid) = request
            .named_args
            .get("banid")
            .and_then(|v| v.parse::<u32>().ok())
        {
            vec![banid]
        } else {
            self.store
                .active_bans
                .values()
                .filter(|b| b.server_id == server_id)
                .map(|b| b.id)
                .collect::<Vec<_>>()
        };

        let mut rows = Vec::new();
        for banid in target_ban_ids {
            let trigger_rows = self.web_ban_trigger_rows(banid, Some(server_id));
            if !trigger_rows.is_empty() {
                rows.extend(trigger_rows);
            } else if let Some(ban) = self
                .store
                .active_bans
                .get(&banid)
                .filter(|b| b.server_id == server_id)
            {
                let mut row = BTreeMap::new();
                row.insert(String::from("banid"), ban.id.to_string());
                row.insert(
                    String::from("client_unique_identifier"),
                    ban.unique_identifier.clone(),
                );
                row.insert(String::from("client_nickname"), ban.name.clone());
                row.insert(
                    String::from("client_hardware_identifier"),
                    ban.hardware_identifier.clone(),
                );
                row.insert(String::from("connection_client_ip"), ban.ip.clone());
                row.insert(String::from("timestamp"), ban.created_at.to_string());
                rows.push(row);
            }
        }

        if rows.is_empty() {
            return QueryResponse::error(
                ERROR_DATABASE_EMPTY_RESULT,
                "database empty result set",
            );
        }

        QueryResponse::ok_rows(rows)
    }

}
