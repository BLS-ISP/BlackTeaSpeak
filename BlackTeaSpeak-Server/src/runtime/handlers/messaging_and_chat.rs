use crate::runtime::BaselineRuntime;
use crate::query::{CommandRequest, QueryResponse};
use crate::runtime::QuerySessionState;
use crate::runtime::*;

impl BaselineRuntime {
    pub(crate) fn handle_sendtextmessage(
        &mut self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        match self.text_message_target(request, session) {
            Ok(target) => {
                let sender = self.query_session_participant(session);
                if target.target_mode == 1 {
                    let Some(target_client_id) = target.target_client_id else {
                        return QueryResponse::error(
                            512,
                            "target is required for private text messages",
                        );
                    };
                    let Some((target_unique_id, target_database_id, target_name)) =
                        self.online_client_identity(target.server_id, target_client_id)
                    else {
                        return QueryResponse::error(768, "target client not found");
                    };
                    self.record_private_message(
                        target.server_id,
                        sender,
                        ConversationParticipant {
                            database_id: target_database_id,
                            unique_identifier: target_unique_id,
                            nickname: target_name,
                        },
                        target.message.clone(),
                    );
                } else {
                    self.record_text_message(
                        &target,
                        sender.database_id,
                        sender.unique_identifier,
                        sender.nickname,
                    );
                }
                self.try_handle_musicbot_chat_command(
                    target.server_id,
                    session.client_id,
                    target.target_mode,
                    target.target_client_id,
                    &target.message,
                );
                QueryResponse::ok()
            }
            Err((error_id, message)) => QueryResponse::error(error_id, message),
        }
    }

    pub(crate) fn handle_messageadd(
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

        let (actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        if let Err(permission_name) = check_required_permission(
            &actor_permissions,
            &["b_client_offline_textmessage_send"],
            "b_client_offline_textmessage_send",
        ) {
            return self.insufficient_permission_response(permission_name);
        }

        let Some(cluid) = request
            .named_args
            .get("cluid")
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        else {
            return QueryResponse::error(512, "cluid is required");
        };
        let Some(subject) = request
            .named_args
            .get("subject")
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        else {
            return QueryResponse::error(512, "subject is required");
        };
        let Some(message) = request
            .named_args
            .get("message")
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        else {
            return QueryResponse::error(512, "message is required");
        };

        let (recipient_uid, recipient_dbid) =
            if let Some((uid, dbid, _)) = self.lookup_client_identity_by_uid(server_id, cluid) {
                (uid, dbid)
            } else {
                (cluid.to_string(), 0)
            };

        let sender_participant = self.query_session_participant(session);
        let msgid = self.store.next_offline_message_id.max(1);
        self.store.next_offline_message_id = msgid.saturating_add(1);

        self.store.offline_messages.insert(
            msgid,
            OfflineMessage {
                id: msgid,
                server_id,
                sender_database_id: actor.client_database_id,
                sender_unique_identifier: sender_participant.unique_identifier,
                sender_nickname: sender_participant.nickname,
                recipient_database_id: recipient_dbid,
                recipient_unique_identifier: recipient_uid,
                subject: subject.to_string(),
                message: message.to_string(),
                timestamp: current_unix_timestamp(),
                flag_read: false,
            },
        );

        let mut row = BTreeMap::new();
        row.insert(String::from("msgid"), msgid.to_string());
        QueryResponse::ok_row(row)
    }

    pub(crate) fn handle_messagelist(&self, session: &QuerySessionState) -> QueryResponse {
        if !session_has_permission_actor(session)
            && session.actor_client_database_id_override.is_none()
        {
            return QueryResponse::error(521, "login required");
        }
        let server_id = session.selected_virtual_server_id.unwrap_or(1);

        let participant = self.query_session_participant(session);
        let rows = self
            .store
            .offline_messages
            .values()
            .filter(|msg| {
                msg.server_id == server_id
                    && (msg.recipient_unique_identifier == participant.unique_identifier
                        || (participant.database_id != 0
                            && msg.recipient_database_id == participant.database_id))
            })
            .map(|msg| {
                let mut row = BTreeMap::new();
                row.insert(String::from("msgid"), msg.id.to_string());
                row.insert(String::from("cluid"), msg.sender_unique_identifier.clone());
                row.insert(String::from("subject"), msg.subject.clone());
                row.insert(String::from("timestamp"), msg.timestamp.to_string());
                row.insert(
                    String::from("flag_read"),
                    if msg.flag_read {
                        String::from("1")
                    } else {
                        String::from("0")
                    },
                );
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

    pub(crate) fn handle_messageget(
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
        let Some(msgid) = request
            .named_args
            .get("msgid")
            .and_then(|v| v.parse::<u64>().ok())
        else {
            return QueryResponse::error(512, "msgid is required");
        };

        let participant = self.query_session_participant(session);
        let is_admin = self
            .query_actor_effective_permissions(session)
            .ok()
            .is_some_and(|(_, perms)| {
                permission_value_or_default(&perms, &["b_permission_modify_power_ignore"]) > 0
            });

        let Some(msg) = self.store.offline_messages.get_mut(&msgid) else {
            return QueryResponse::error(
                ERROR_DATABASE_EMPTY_RESULT,
                "database empty result set",
            );
        };
        if msg.server_id != server_id
            || (!is_admin
                && msg.recipient_unique_identifier != participant.unique_identifier
                && (participant.database_id == 0
                    || msg.recipient_database_id != participant.database_id))
        {
            return QueryResponse::error(
                ERROR_DATABASE_EMPTY_RESULT,
                "database empty result set",
            );
        }

        msg.flag_read = true;

        let mut row = BTreeMap::new();
        row.insert(String::from("msgid"), msg.id.to_string());
        row.insert(String::from("cluid"), msg.sender_unique_identifier.clone());
        row.insert(String::from("subject"), msg.subject.clone());
        row.insert(String::from("message"), msg.message.clone());
        row.insert(String::from("timestamp"), msg.timestamp.to_string());
        row.insert(String::from("flag_read"), String::from("1"));
        QueryResponse::ok_row(row)
    }

    pub(crate) fn handle_messageupdateflag(
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
        let Some(msgid) = request
            .named_args
            .get("msgid")
            .and_then(|v| v.parse::<u64>().ok())
        else {
            return QueryResponse::error(512, "msgid is required");
        };
        let Some(flag_str) = request.named_args.get("flag") else {
            return QueryResponse::error(512, "flag is required");
        };
        let Some(flag_read) = parse_query_bool(flag_str) else {
            return QueryResponse::error(512, "flag must be 0 or 1");
        };

        let participant = self.query_session_participant(session);
        let is_admin = self
            .query_actor_effective_permissions(session)
            .ok()
            .is_some_and(|(_, perms)| {
                permission_value_or_default(&perms, &["b_permission_modify_power_ignore"]) > 0
            });

        let Some(msg) = self.store.offline_messages.get_mut(&msgid) else {
            return QueryResponse::error(
                ERROR_DATABASE_EMPTY_RESULT,
                "database empty result set",
            );
        };
        if msg.server_id != server_id
            || (!is_admin
                && msg.recipient_unique_identifier != participant.unique_identifier
                && (participant.database_id == 0
                    || msg.recipient_database_id != participant.database_id))
        {
            return QueryResponse::error(
                ERROR_DATABASE_EMPTY_RESULT,
                "database empty result set",
            );
        }

        msg.flag_read = flag_read;
        QueryResponse::ok()
    }

    pub(crate) fn handle_messagedel(
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
        let Some(msgid) = request
            .named_args
            .get("msgid")
            .and_then(|v| v.parse::<u64>().ok())
        else {
            return QueryResponse::error(512, "msgid is required");
        };

        let participant = self.query_session_participant(session);
        let is_admin = self
            .query_actor_effective_permissions(session)
            .ok()
            .is_some_and(|(_, perms)| {
                permission_value_or_default(&perms, &["b_permission_modify_power_ignore"]) > 0
            });

        let can_delete = self.store.offline_messages.get(&msgid).is_some_and(|msg| {
            msg.server_id == server_id
                && (is_admin
                    || msg.recipient_unique_identifier == participant.unique_identifier
                    || (participant.database_id != 0
                        && msg.recipient_database_id == participant.database_id))
        });
        if !can_delete {
            return QueryResponse::error(
                ERROR_DATABASE_EMPTY_RESULT,
                "database empty result set",
            );
        }

        self.store.offline_messages.remove(&msgid);
        QueryResponse::ok()
    }

    pub(crate) fn handle_conversationfetch(
        &self,
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

        let mut cids = Vec::new();
        if !request.option_groups.is_empty() {
            for group in &request.option_groups {
                if let Some(cid) = group.get("cid").and_then(|v| v.parse::<u32>().ok()) {
                    cids.push(cid);
                }
            }
        } else if let Some(cid) = request
            .named_args
            .get("cid")
            .and_then(|v| v.parse::<u32>().ok())
        {
            cids.push(cid);
        } else {
            cids.push(0);
            if let Some(channels) = self.store.channels.get(&server_id) {
                cids.extend(channels.iter().map(|c| c.id));
            }
        }

        let rows = self.web_conversation_index_rows(server_id, &cids);
        if rows.is_empty() {
            return QueryResponse::error(
                ERROR_DATABASE_EMPTY_RESULT,
                "database empty result set",
            );
        }
        QueryResponse::ok_rows(rows)
    }

    pub(crate) fn handle_conversationhistory(
        &self,
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

        let timestamp_begin = request
            .named_args
            .get("timestamp_begin")
            .or_else(|| request.named_args.get("begin_timestamp"))
            .and_then(|v| v.parse::<u64>().ok());
        let timestamp_end = request
            .named_args
            .get("timestamp_end")
            .or_else(|| request.named_args.get("end_timestamp"))
            .and_then(|v| v.parse::<u64>().ok());
        let message_count = request
            .named_args
            .get("message_count")
            .or_else(|| request.named_args.get("count"))
            .or_else(|| request.named_args.get("limit"))
            .and_then(|v| v.parse::<usize>().ok());

        let partner_uid = request.named_args.get("cluid").map(String::as_str);
        let partner_dbid = request
            .named_args
            .get("cldbid")
            .and_then(|v| v.parse::<u64>().ok());

        let rows = if partner_uid.is_some() || partner_dbid.is_some() {
            let participant = self.query_session_participant(session);
            self.web_private_conversation_history_rows(
                server_id,
                participant.database_id,
                partner_uid,
                partner_dbid,
                timestamp_begin,
                timestamp_end,
                message_count,
            )
            .unwrap_or_default()
        } else {
            let cid = request
                .named_args
                .get("cid")
                .and_then(|v| v.parse::<u32>().ok())
                .unwrap_or(0);
            self.web_conversation_history_rows(
                server_id,
                cid,
                timestamp_begin,
                timestamp_end,
                message_count,
            )
            .unwrap_or_default()
        };

        if rows.is_empty() {
            return QueryResponse::error(
                ERROR_DATABASE_EMPTY_RESULT,
                "database empty result set",
            );
        }
        QueryResponse::ok_rows(rows)
    }

    pub(crate) fn handle_conversationmessagedelete(
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
        let cid = request
            .named_args
            .get("cid")
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(0);
        let timestamp_begin = request
            .named_args
            .get("timestamp_begin")
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0);
        let timestamp_end = request
            .named_args
            .get("timestamp_end")
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(u64::MAX);
        let limit = request
            .named_args
            .get("limit")
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(100);
        let cldbid = request
            .named_args
            .get("cldbid")
            .and_then(|v| v.parse::<u64>().ok());

        let deleted = self.delete_conversation_messages(
            server_id,
            cid,
            Some(timestamp_begin),
            Some(timestamp_end),
            Some(limit),
            cldbid,
        );

        let mut row = BTreeMap::new();
        row.insert(String::from("cid"), cid.to_string());
        row.insert(String::from("timestamp_begin"), timestamp_begin.to_string());
        row.insert(String::from("timestamp_end"), timestamp_end.to_string());
        row.insert(
            String::from("cldbid"),
            cldbid.unwrap_or(0).to_string(),
        );
        row.insert(String::from("limit"), limit.to_string());
        row.insert(String::from("deleted"), deleted.to_string());
        QueryResponse::ok_row(row)
    }

}

