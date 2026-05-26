use crate::runtime::BaselineRuntime;
use crate::query::{CommandRequest, QueryResponse};
use crate::runtime::QuerySessionState;
use crate::runtime::*;

impl BaselineRuntime {
    pub(crate) fn handle_channelinfo(
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

        let mut row = BTreeMap::new();
        row.insert(String::from("cid"), channel.id.to_string());
        row.insert(String::from("pid"), channel.parent_id.to_string());
        row.insert(String::from("channel_order"), channel.order.to_string());
        row.insert(String::from("channel_name"), channel.name.clone());
        row.insert(String::from("channel_topic"), channel.topic.clone());
        row.insert(
            String::from("channel_description"),
            channel.description.clone(),
        );
        row.insert(String::from("channel_password"), channel.password.clone());
        row.insert(String::from("channel_codec"), channel.codec.to_string());
        row.insert(String::from("channel_codec_quality"), channel.codec_quality.to_string());
        row.insert(String::from("channel_maxclients"), channel.maxclients.to_string());
        row.insert(String::from("channel_maxfamilyclients"), channel.maxfamilyclients.to_string());
        
        apply_channel_kind_rows(&mut row, channel.kind);
        row.insert(
            String::from("channel_flag_default"),
            if self.default_channel_id_for_server(server_id) == Some(channel.id) {
                String::from("1")
            } else {
                String::from("0")
            },
        );
        row.insert(String::from("channel_flag_password"), if channel.flag_password { String::from("1") } else { String::from("0") });
        row.insert(String::from("channel_needed_talk_power"), String::from("0"));
        row.insert(
            String::from("total_clients"),
            self.client_count_in_channel(server_id, channel.id)
                .to_string(),
        );
        QueryResponse::ok_row(row)
    }

    pub(crate) fn handle_channelcreate(
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
        let Some(channel_name) = request.named_args.get("channel_name").cloned() else {
            return QueryResponse::error(512, "channel_name is required");
        };

        let parent_id = request
            .named_args
            .get("cpid")
            .and_then(|value| value.parse::<u32>().ok())
            .unwrap_or(0);
        let requested_order = request
            .named_args
            .get("order")
            .and_then(|value| value.parse::<u32>().ok());
        let channel_kind = ChannelKind::from_request_flags(request);

        let (_actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        let create_permission = match channel_kind {
            ChannelKind::Temporary => "b_channel_create_temporary",
            ChannelKind::SemiPermanent => "b_channel_create_semi_permanent",
            ChannelKind::Permanent => "b_channel_create_permanent",
        };
        if let Err(permission_name) = check_required_permission(
            &actor_permissions,
            &[create_permission],
            create_permission,
        ) {
            return self.insufficient_permission_response(permission_name);
        }
        if parent_id != 0
            && let Err(permission_name) = check_required_permission(
                &actor_permissions,
                &["b_channel_create_child"],
                "b_channel_create_child",
            )
        {
            return self.insufficient_permission_response(permission_name);
        }
        if request.named_args.contains_key("channel_topic")
            && let Err(permission_name) = check_required_permission(
                &actor_permissions,
                &["b_channel_create_with_topic"],
                "b_channel_create_with_topic",
            )
        {
            return self.insufficient_permission_response(permission_name);
        }
        if request.named_args.contains_key("channel_description")
            && let Err(permission_name) = check_required_permission(
                &actor_permissions,
                &["b_channel_create_with_description"],
                "b_channel_create_with_description",
            )
        {
            return self.insufficient_permission_response(permission_name);
        }
        if requested_order.is_some()
            && let Err(permission_name) = check_required_permission(
                &actor_permissions,
                &["b_channel_create_with_sortorder"],
                "b_channel_create_with_sortorder",
            )
        {
            return self.insufficient_permission_response(permission_name);
        }

        let Some(channels) = self.store.channels.get_mut(&server_id) else {
            return QueryResponse::error(768, "virtual server channels not found");
        };
        if parent_id != 0 && !channels.iter().any(|channel| channel.id == parent_id) {
            return QueryResponse::error(768, "parent channel not found");
        }

        let sibling_ids = ordered_sibling_ids(channels, parent_id, None);
        let insert_index = match resolve_insert_index(&sibling_ids, requested_order) {
            Some(insert_index) => insert_index,
            None => return QueryResponse::error(768, "sort order anchor not found"),
        };
        let channel_id = channels
            .iter()
            .map(|channel| channel.id)
            .max()
            .unwrap_or(0)
            .saturating_add(1);

        channels.push(Channel {
            id: channel_id,
            parent_id,
            order: 0,
            kind: channel_kind,
            name: channel_name,
            topic: request
                .named_args
                .get("channel_topic")
                .cloned()
                .unwrap_or_default(),
            description: request
                .named_args
                .get("channel_description")
                .cloned()
                .unwrap_or_default(),
            password: String::new(),
            codec: 0,
            codec_quality: 5,
            maxclients: -1,
            maxfamilyclients: -1,
            flag_default: false,
            flag_password: false,
            permissions: BTreeMap::new(),
        });

        let mut ordered_ids = sibling_ids;
        ordered_ids.insert(insert_index, channel_id);
        relink_sibling_orders(channels, parent_id, &ordered_ids);

        let mut row = BTreeMap::new();
        row.insert(String::from("cid"), channel_id.to_string());
        QueryResponse::ok_row(row)
    }

    pub(crate) fn handle_channeldelete(
        &mut self,
        request: &CommandRequest,
        session: &mut QuerySessionState,
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
        let Some(force) = request
            .named_args
            .get("force")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return QueryResponse::error(512, "force is required");
        };

        let channel_client_count = self.client_count_in_channel(server_id, channel_id);

        let (_actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
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
        if let Err(permission_name) = check_required_permission(
            &actor_permissions,
            &["b_channel_delete_permanent"],
            "b_channel_delete_permanent",
        ) {
            return self.insufficient_permission_response(permission_name);
        }
        if force == 1
            && let Err(permission_name) = check_required_permission(
                &actor_permissions,
                &["b_channel_delete_flag_force"],
                "b_channel_delete_flag_force",
            )
        {
            return self.insufficient_permission_response(permission_name);
        }
        if let Err(permission_name) =
            check_channel_delete_power_allowed(&actor_permissions, &target_permissions)
        {
            return self.insufficient_permission_response(permission_name);
        }

        let Some(channels) = self.store.channels.get_mut(&server_id) else {
            return QueryResponse::error(768, "virtual server channels not found");
        };
        let Some(channel_index) = channels.iter().position(|channel| channel.id == channel_id)
        else {
            return QueryResponse::error(768, "channel not found");
        };
        if channel_id == 1 {
            return QueryResponse::error(770, "default channel cannot be deleted in baseline");
        }
        if channels
            .iter()
            .any(|channel| channel.parent_id == channel_id)
        {
            return QueryResponse::error(770, "channel has child channels");
        }
        if channel_client_count > 0 {
            return QueryResponse::error(
                770,
                if force == 1 {
                    "baseline cannot force-delete occupied channels yet"
                } else {
                    "channel is not empty"
                },
            );
        }

        let parent_id = channels[channel_index].parent_id;
        channels.remove(channel_index);
        let sibling_ids = ordered_sibling_ids(channels, parent_id, None);
        relink_sibling_orders(channels, parent_id, &sibling_ids);
        self.store
            .channel_group_assignments
            .retain(|assignment| assignment.channel_id != channel_id);
        self.store
            .channel_client_permissions
            .retain(|target| target.channel_id != channel_id);
        if let Some(messages) = self.store.conversation_messages.get_mut(&server_id) {
            messages.retain(|message| message.conversation_id != channel_id);
        }

        if session.current_channel_id == Some(channel_id) {
            session.current_channel_id = self.default_channel_id_for_server(server_id);
        }

        QueryResponse::ok()
    }

    pub(crate) fn handle_channeledit(
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

        let (_actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
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
        if let Err(permission_name) =
            check_channel_modify_power_allowed(&actor_permissions, &target_permissions)
        {
            return self.insufficient_permission_response(permission_name);
        }
        if request.named_args.contains_key("channel_name")
            && let Err(permission_name) = check_required_permission(
                &actor_permissions,
                &["b_channel_modify_name"],
                "b_channel_modify_name",
            )
        {
            return self.insufficient_permission_response(permission_name);
        }
        if request.named_args.contains_key("channel_topic")
            && let Err(permission_name) = check_required_permission(
                &actor_permissions,
                &["b_channel_modify_topic"],
                "b_channel_modify_topic",
            )
        {
            return self.insufficient_permission_response(permission_name);
        }
        if request.named_args.contains_key("channel_description")
            && let Err(permission_name) = check_required_permission(
                &actor_permissions,
                &["b_channel_modify_description"],
                "b_channel_modify_description",
            )
        {
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

        if let Some(channel_name) = request.named_args.get("channel_name") {
            channel.name = channel_name.clone();
        }
        if let Some(channel_topic) = request.named_args.get("channel_topic") {
            channel.topic = channel_topic.clone();
        }
        if let Some(channel_description) = request.named_args.get("channel_description") {
            channel.description = channel_description.clone();
        }
        if let Some(channel_password) = request.named_args.get("channel_password") {
            channel.password = channel_password.clone();
            channel.flag_password = !channel.password.is_empty();
        }
        if let Some(codec) = request.named_args.get("channel_codec").and_then(|v| v.parse().ok()) {
            channel.codec = codec;
        }
        if let Some(quality) = request.named_args.get("channel_codec_quality").and_then(|v| v.parse().ok()) {
            channel.codec_quality = quality;
        }
        if let Some(maxclients) = request.named_args.get("channel_maxclients").and_then(|v| v.parse().ok()) {
            channel.maxclients = maxclients;
        }
        
        let mut is_semi = false;
        let mut is_perm = false;
        if request.named_args.get("channel_flag_semi_permanent").map(|v| v.as_str()) == Some("1") { is_semi = true; }
        if request.named_args.get("channel_flag_permanent").map(|v| v.as_str()) == Some("1") { is_perm = true; }
        if request.named_args.contains_key("channel_flag_semi_permanent") || request.named_args.contains_key("channel_flag_permanent") {
            channel.kind = ChannelKind::from_flags(is_perm, is_semi);
        }
        
        // Save to DB
        let _ = self.store.db.save_channel(server_id, channel);

        QueryResponse::ok()
    }

    pub(crate) fn handle_channelmove(
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
        let Some(parent_id) = request
            .named_args
            .get("cpid")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return QueryResponse::error(512, "cpid is required");
        };
        let requested_order = request
            .named_args
            .get("order")
            .and_then(|value| value.parse::<u32>().ok());

        let Some(channels) = self.store.channels.get_mut(&server_id) else {
            return QueryResponse::error(768, "virtual server channels not found");
        };
        let Some(channel_index) = channels.iter().position(|channel| channel.id == channel_id)
        else {
            return QueryResponse::error(768, "channel not found");
        };
        let Some(parent_id) = (if parent_id == 0 { Some(0) } else { channels.iter().find(|c| c.id == parent_id).map(|c| c.id) }) else {
            return QueryResponse::error(768, "parent channel not found");
        };
        if channel_id == parent_id || channel_is_descendant(channels, parent_id, channel_id) {
            return QueryResponse::error(770, "channel cannot be moved below itself");
        }

        let previous_parent_id = channels[channel_index].parent_id;
        let mut sibling_ids = ordered_sibling_ids(channels, parent_id, Some(channel_id));
        let insert_index = match resolve_insert_index(&sibling_ids, requested_order) {
            Some(insert_index) => insert_index,
            None => return QueryResponse::error(768, "sort order anchor not found"),
        };

        channels[channel_index].parent_id = parent_id;
        sibling_ids.insert(insert_index, channel_id);
        relink_sibling_orders(channels, parent_id, &sibling_ids);

        if previous_parent_id != parent_id {
            let previous_sibling_ids =
                ordered_sibling_ids(channels, previous_parent_id, Some(channel_id));
            relink_sibling_orders(channels, previous_parent_id, &previous_sibling_ids);
        }

        QueryResponse::ok()
    }

    pub(crate) fn handle_channellist(&self, session: &QuerySessionState) -> QueryResponse {
        let server_id = match session.selected_virtual_server_id {
            Some(server_id) => server_id,
            None => return QueryResponse::error(522, "virtual server selection required"),
        };

        let rows = self
            .store
            .channels
            .get(&server_id)
            .map(|channels| {
                channels
                    .iter()
                    .map(|channel| {
                        let mut row = BTreeMap::new();
                        row.insert(String::from("cid"), channel.id.to_string());
                        row.insert(String::from("pid"), channel.parent_id.to_string());
                        row.insert(String::from("channel_order"), channel.order.to_string());
                        row.insert(String::from("channel_name"), channel.name.clone());
                        row.insert(String::from("channel_topic"), channel.topic.clone());
                        apply_channel_kind_rows(&mut row, channel.kind);
                        row.insert(
                            String::from("total_clients"),
                            self.client_count_in_channel(server_id, channel.id)
                                .to_string(),
                        );
                        row
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        QueryResponse::ok_rows(rows)
    }

}
