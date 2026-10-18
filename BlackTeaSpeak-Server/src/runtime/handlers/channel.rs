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
        row.insert(
            String::from("channel_flag_password"),
            if channel.flag_password || !channel.password.is_empty() {
                String::from("1")
            } else {
                String::from("0")
            },
        );
        row.insert(
            String::from("channel_flag_maxclients_unlimited"),
            if channel.maxclients < 0 {
                String::from("1")
            } else {
                String::from("0")
            },
        );
        row.insert(
            String::from("channel_flag_maxfamilyclients_unlimited"),
            if channel.maxfamilyclients < 0 {
                String::from("1")
            } else {
                String::from("0")
            },
        );
        row.insert(String::from("cpid"), channel.parent_id.to_string());
        row.insert(
            String::from("channel_needed_talk_power"),
            channel.needed_talk_power().to_string(),
        );
        row.insert(
            String::from("i_channel_needed_join_power"),
            channel
                .permission_power("i_channel_needed_join_power")
                .to_string(),
        );
        row.insert(
            String::from("i_channel_needed_subscribe_power"),
            channel
                .permission_power("i_channel_needed_subscribe_power")
                .to_string(),
        );
        row.insert(
            String::from("i_channel_needed_description_view_power"),
            channel
                .permission_power("i_channel_needed_description_view_power")
                .to_string(),
        );
        row.insert(
            String::from("i_channel_needed_modify_power"),
            channel
                .permission_power("i_channel_needed_modify_power")
                .to_string(),
        );
        row.insert(
            String::from("i_channel_needed_delete_power"),
            channel
                .permission_power("i_channel_needed_delete_power")
                .to_string(),
        );
        row.insert(
            String::from("channel_icon_id"),
            channel.permission_power("i_icon_id").to_string(),
        );
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
            .or_else(|| request.named_args.get("pid"))
            .and_then(|value| value.parse::<u32>().ok())
            .unwrap_or(0);
        let requested_order = request
            .named_args
            .get("order")
            .or_else(|| request.named_args.get("channel_order"))
            .and_then(|value| value.parse::<u32>().ok());
        let make_default = request
            .named_args
            .get("channel_flag_default")
            .is_some_and(|v| runtime_bool_flag(v));
        let channel_kind = if make_default {
            ChannelKind::Permanent
        } else {
            ChannelKind::from_request_flags(request)
        };

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

        let next_global_channel_id = self
            .store
            .channels
            .values()
            .flat_map(|chs| chs.iter().map(|channel| channel.id))
            .max()
            .unwrap_or(0)
            .saturating_add(1);
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
        let channel_id = next_global_channel_id.max(
            channels
                .iter()
                .map(|channel| channel.id)
                .max()
                .unwrap_or(0)
                .saturating_add(1),
        );

        let password = request
            .named_args
            .get("channel_password")
            .cloned()
            .unwrap_or_default();
        let flag_password = !password.is_empty()
            || request
                .named_args
                .get("channel_flag_password")
                .is_some_and(|v| runtime_bool_flag(v));
        let codec = request
            .named_args
            .get("channel_codec")
            .and_then(|v| v.parse().ok())
            .unwrap_or(4);
        let codec_quality = request
            .named_args
            .get("channel_codec_quality")
            .and_then(|v| v.parse().ok())
            .unwrap_or(6);
        let mut maxclients = request
            .named_args
            .get("channel_maxclients")
            .and_then(|v| v.parse().ok())
            .unwrap_or(-1);
        if request
            .named_args
            .get("channel_flag_maxclients_unlimited")
            .is_some_and(|v| runtime_bool_flag(v))
            && !request.named_args.contains_key("channel_maxclients")
        {
            maxclients = -1;
        }
        let mut maxfamilyclients = request
            .named_args
            .get("channel_maxfamilyclients")
            .and_then(|v| v.parse().ok())
            .unwrap_or(-1);
        if request
            .named_args
            .get("channel_flag_maxfamilyclients_unlimited")
            .is_some_and(|v| runtime_bool_flag(v))
            && !request.named_args.contains_key("channel_maxfamilyclients")
        {
            maxfamilyclients = -1;
        }

        if make_default {
            for ch in channels.iter_mut() {
                ch.flag_default = false;
            }
        }

        let mut new_channel = Channel {
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
            password,
            codec,
            codec_quality,
            maxclients,
            maxfamilyclients,
            flag_default: make_default,
            flag_password,
            permissions: BTreeMap::new(),
        };
        if let Some(needed_talk_power) = request
            .named_args
            .get("channel_needed_talk_power")
            .or_else(|| request.named_args.get("i_channel_needed_talk_power"))
            .and_then(|v| v.parse::<i64>().ok())
        {
            new_channel.set_needed_talk_power(needed_talk_power);
        }
        for (arg_key, perm_name) in [
            ("i_channel_needed_join_power", "i_channel_needed_join_power"),
            ("channel_needed_join_power", "i_channel_needed_join_power"),
            (
                "i_channel_needed_subscribe_power",
                "i_channel_needed_subscribe_power",
            ),
            (
                "channel_needed_subscribe_power",
                "i_channel_needed_subscribe_power",
            ),
            (
                "i_channel_needed_description_view_power",
                "i_channel_needed_description_view_power",
            ),
            (
                "channel_needed_description_view_power",
                "i_channel_needed_description_view_power",
            ),
            (
                "i_channel_needed_modify_power",
                "i_channel_needed_modify_power",
            ),
            (
                "channel_needed_modify_power",
                "i_channel_needed_modify_power",
            ),
            (
                "i_channel_needed_delete_power",
                "i_channel_needed_delete_power",
            ),
            (
                "channel_needed_delete_power",
                "i_channel_needed_delete_power",
            ),
            ("channel_icon_id", "i_icon_id"),
            ("i_icon_id", "i_icon_id"),
        ] {
            if let Some(val) = request
                .named_args
                .get(arg_key)
                .and_then(|v| v.parse::<i64>().ok())
            {
                new_channel.set_permission_power(perm_name, val);
            }
        }

        channels.push(new_channel);

        let mut ordered_ids = sibling_ids;
        ordered_ids.insert(insert_index, channel_id);
        relink_sibling_orders(channels, parent_id, &ordered_ids);

        for ch in channels.iter() {
            if ch.parent_id == parent_id || make_default {
                let _ = self.db.save_channel(server_id, ch);
                let _ = self.store.db.save_channel(server_id, ch);
            }
        }

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
        let force = request
            .named_args
            .get("force")
            .and_then(|value| value.parse::<u32>().ok())
            .unwrap_or(1);

        let channel_client_count = self.client_count_in_channel(server_id, channel_id);

        let (_actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        let Some((target_kind, target_permissions)) = self
            .store
            .channels
            .get(&server_id)
            .and_then(|channels| channels.iter().find(|channel| channel.id == channel_id))
            .map(|channel| (channel.kind, channel.permissions.clone()))
        else {
            return QueryResponse::error(768, "channel not found");
        };
        let delete_perm = match target_kind {
            ChannelKind::Permanent => "b_channel_delete_permanent",
            ChannelKind::SemiPermanent => "b_channel_delete_semi_permanent",
            ChannelKind::Temporary => "b_channel_delete_temporary",
        };
        if let Err(permission_name) = check_required_permission(
            &actor_permissions,
            &[delete_perm, "b_channel_delete_permanent"],
            delete_perm,
        ) {
            return self.insufficient_permission_response(permission_name);
        }
        if force == 1
            && channel_client_count > 0
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

        let default_cid = self.default_channel_id_for_server(server_id).unwrap_or(1);
        let Some(channels) = self.store.channels.get_mut(&server_id) else {
            return QueryResponse::error(768, "virtual server channels not found");
        };
        let Some(channel_index) = channels.iter().position(|channel| channel.id == channel_id)
        else {
            return QueryResponse::error(768, "channel not found");
        };
        if channel_id == 1 || channels[channel_index].flag_default {
            return QueryResponse::error(770, "default channel cannot be deleted in baseline");
        }
        let has_children = channels
            .iter()
            .any(|channel| channel.parent_id == channel_id);
        if has_children && force == 0 {
            return QueryResponse::error(770, "channel has child channels");
        }
        if channel_client_count > 0 && force == 0 {
            return QueryResponse::error(770, "channel is not empty");
        }

        // Collect channel_id and (if force==1) any descendant channels
        let mut to_delete = BTreeSet::new();
        to_delete.insert(channel_id);
        if force == 1 {
            loop {
                let mut added = false;
                for ch in channels.iter() {
                    if !to_delete.contains(&ch.id) && to_delete.contains(&ch.parent_id) {
                        to_delete.insert(ch.id);
                        added = true;
                    }
                }
                if !added {
                    break;
                }
            }
        }

        let parent_id = channels[channel_index].parent_id;
        channels.retain(|ch| !to_delete.contains(&ch.id));
        let sibling_ids = ordered_sibling_ids(channels, parent_id, None);
        relink_sibling_orders(channels, parent_id, &sibling_ids);
        for &del_cid in &to_delete {
            let _ = self.db.delete_channel(server_id, del_cid);
            let _ = self.store.db.delete_channel(server_id, del_cid);
        }

        // Move any clients in deleted channels to default channel (or remove MusicBots if temporary channel)
        let bot_clients_in_deleted: Vec<(u32, u64)> = self
            .store
            .music_bots
            .iter()
            .filter_map(|(bot_id, bot)| {
                let clid = bot.linked_client_id?;
                let cid = self.store.online_clients.get(&clid).map(|c| c.channel_id)?;
                (bot.server_id == server_id && to_delete.contains(&cid)).then_some((*bot_id, clid))
            })
            .collect();

        if target_kind == ChannelKind::Temporary {
            for (bot_id, bot_clid) in &bot_clients_in_deleted {
                if let Some(mut removed_bot) = self.store.music_bots.remove(bot_id) {
                    Self::stop_music_bot_playback(&mut removed_bot);
                }
                self.store.online_clients.remove(bot_clid);
            }
        }

        for client in self.store.online_clients.values_mut() {
            if client.server_id == server_id && to_delete.contains(&client.channel_id) {
                client.channel_id = default_cid;
            }
        }

        if target_kind != ChannelKind::Temporary {
            for (bot_id, _) in &bot_clients_in_deleted {
                self.sync_music_bot_client_state(*bot_id);
            }
        }

        self.store
            .channel_group_assignments
            .retain(|assignment| !to_delete.contains(&assignment.channel_id));
        self.store
            .channel_client_permissions
            .retain(|target| !to_delete.contains(&target.channel_id));
        if let Some(messages) = self.store.conversation_messages.get_mut(&server_id) {
            messages.retain(|message| !to_delete.contains(&message.conversation_id));
        }

        if session
            .current_channel_id
            .is_some_and(|cid| to_delete.contains(&cid))
        {
            session.current_channel_id = Some(default_cid);
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

        let make_default = request
            .named_args
            .get("channel_flag_default")
            .is_some_and(|v| runtime_bool_flag(v));
        if make_default {
            if let Some(channels) = self.store.channels.get_mut(&server_id) {
                for ch in channels.iter_mut() {
                    ch.flag_default = ch.id == channel_id;
                }
            }
        }

        // Check if parent_id or order is also being modified via channeledit
        let requested_cpid = request
            .named_args
            .get("cpid")
            .or_else(|| request.named_args.get("pid"))
            .and_then(|v| v.parse::<u32>().ok());
        let requested_order = request
            .named_args
            .get("channel_order")
            .or_else(|| request.named_args.get("order"))
            .and_then(|v| v.parse::<u32>().ok());

        if requested_cpid.is_some() || requested_order.is_some() {
            if let Some(channels) = self.store.channels.get_mut(&server_id) {
                if let Some(channel_index) = channels.iter().position(|c| c.id == channel_id) {
                    let current_parent_id = channels[channel_index].parent_id;
                    let target_parent_id = requested_cpid.unwrap_or(current_parent_id);
                    if target_parent_id != 0
                        && !channels.iter().any(|c| c.id == target_parent_id)
                    {
                        return QueryResponse::error(768, "parent channel not found");
                    }
                    if channel_id == target_parent_id
                        || channel_is_descendant(channels, target_parent_id, channel_id)
                    {
                        return QueryResponse::error(
                            770,
                            "channel cannot be moved below itself",
                        );
                    }
                    let mut sibling_ids =
                        ordered_sibling_ids(channels, target_parent_id, Some(channel_id));
                    if let Some(insert_index) =
                        resolve_insert_index(&sibling_ids, requested_order)
                    {
                        channels[channel_index].parent_id = target_parent_id;
                        sibling_ids.insert(insert_index, channel_id);
                        relink_sibling_orders(channels, target_parent_id, &sibling_ids);
                        if current_parent_id != target_parent_id {
                            let prev_siblings = ordered_sibling_ids(
                                channels,
                                current_parent_id,
                                Some(channel_id),
                            );
                            relink_sibling_orders(channels, current_parent_id, &prev_siblings);
                        }
                    }
                }
            }
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
        if let Some(flag_pw) = request.named_args.get("channel_flag_password") {
            let enabled = runtime_bool_flag(flag_pw);
            channel.flag_password = enabled;
            if !enabled && !request.named_args.contains_key("channel_password") {
                channel.password.clear();
            }
        }
        if let Some(codec) = request
            .named_args
            .get("channel_codec")
            .and_then(|v| v.parse().ok())
        {
            channel.codec = codec;
        }
        if let Some(quality) = request
            .named_args
            .get("channel_codec_quality")
            .and_then(|v| v.parse().ok())
        {
            channel.codec_quality = quality;
        }
        if let Some(maxclients) = request
            .named_args
            .get("channel_maxclients")
            .and_then(|v| v.parse().ok())
        {
            channel.maxclients = maxclients;
        }
        if request
            .named_args
            .get("channel_flag_maxclients_unlimited")
            .is_some_and(|v| runtime_bool_flag(v))
            && !request.named_args.contains_key("channel_maxclients")
        {
            channel.maxclients = -1;
        }
        if let Some(maxfamilyclients) = request
            .named_args
            .get("channel_maxfamilyclients")
            .and_then(|v| v.parse().ok())
        {
            channel.maxfamilyclients = maxfamilyclients;
        }
        if request
            .named_args
            .get("channel_flag_maxfamilyclients_unlimited")
            .is_some_and(|v| runtime_bool_flag(v))
            && !request.named_args.contains_key("channel_maxfamilyclients")
        {
            channel.maxfamilyclients = -1;
        }
        if let Some(needed_talk_power) = request
            .named_args
            .get("channel_needed_talk_power")
            .or_else(|| request.named_args.get("i_channel_needed_talk_power"))
            .and_then(|v| v.parse::<i64>().ok())
        {
            channel.set_needed_talk_power(needed_talk_power);
        }
        for (arg_key, perm_name) in [
            ("i_channel_needed_join_power", "i_channel_needed_join_power"),
            ("channel_needed_join_power", "i_channel_needed_join_power"),
            (
                "i_channel_needed_subscribe_power",
                "i_channel_needed_subscribe_power",
            ),
            (
                "channel_needed_subscribe_power",
                "i_channel_needed_subscribe_power",
            ),
            (
                "i_channel_needed_description_view_power",
                "i_channel_needed_description_view_power",
            ),
            (
                "channel_needed_description_view_power",
                "i_channel_needed_description_view_power",
            ),
            (
                "i_channel_needed_modify_power",
                "i_channel_needed_modify_power",
            ),
            (
                "channel_needed_modify_power",
                "i_channel_needed_modify_power",
            ),
            (
                "i_channel_needed_delete_power",
                "i_channel_needed_delete_power",
            ),
            (
                "channel_needed_delete_power",
                "i_channel_needed_delete_power",
            ),
            ("channel_icon_id", "i_icon_id"),
            ("i_icon_id", "i_icon_id"),
        ] {
            if let Some(val) = request
                .named_args
                .get(arg_key)
                .and_then(|v| v.parse::<i64>().ok())
            {
                channel.set_permission_power(perm_name, val);
            }
        }

        if make_default {
            channel.kind = ChannelKind::Permanent;
        } else if request.named_args.contains_key("channel_flag_semi_permanent")
            || request.named_args.contains_key("channel_flag_permanent")
            || request.named_args.contains_key("channel_flag_temporary")
        {
            let is_temp = request
                .named_args
                .get("channel_flag_temporary")
                .is_some_and(|v| runtime_bool_flag(v));
            let is_semi = request
                .named_args
                .get("channel_flag_semi_permanent")
                .is_some_and(|v| runtime_bool_flag(v));
            let is_perm = request
                .named_args
                .get("channel_flag_permanent")
                .is_some_and(|v| runtime_bool_flag(v));
            channel.kind = if is_temp {
                ChannelKind::Temporary
            } else {
                ChannelKind::from_flags(is_perm, is_semi)
            };
        }

        // Save all channels in server to DB (including order/default changes)
        if let Some(channels) = self.store.channels.get(&server_id) {
            for ch in channels {
                let _ = self.db.save_channel(server_id, ch);
                let _ = self.store.db.save_channel(server_id, ch);
            }
        }

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
            .or_else(|| request.named_args.get("pid"))
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return QueryResponse::error(512, "cpid is required");
        };
        let requested_order = request
            .named_args
            .get("order")
            .or_else(|| request.named_args.get("channel_order"))
            .and_then(|value| value.parse::<u32>().ok());

        let Some(channels) = self.store.channels.get_mut(&server_id) else {
            return QueryResponse::error(768, "virtual server channels not found");
        };
        let Some(channel_index) = channels.iter().position(|channel| channel.id == channel_id)
        else {
            return QueryResponse::error(768, "channel not found");
        };
        let Some(parent_id) = (if parent_id == 0 {
            Some(0)
        } else {
            channels.iter().find(|c| c.id == parent_id).map(|c| c.id)
        }) else {
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

        for ch in channels.iter() {
            if ch.parent_id == parent_id || ch.parent_id == previous_parent_id {
                let _ = self.db.save_channel(server_id, ch);
                let _ = self.store.db.save_channel(server_id, ch);
            }
        }

        QueryResponse::ok()
    }

    pub(crate) fn handle_channellist(
        &self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        let server_id = match session.selected_virtual_server_id {
            Some(server_id) => server_id,
            None => return QueryResponse::error(522, "virtual server selection required"),
        };

        let include_flags = request.flags.contains("flags");
        let include_voice = request.flags.contains("voice");
        let include_limits = request.flags.contains("limits");
        let default_cid = self.default_channel_id_for_server(server_id);

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
                        if include_flags {
                            row.insert(
                                String::from("channel_flag_default"),
                                if default_cid == Some(channel.id) {
                                    String::from("1")
                                } else {
                                    String::from("0")
                                },
                            );
                            row.insert(
                                String::from("channel_flag_password"),
                                if channel.flag_password || !channel.password.is_empty() {
                                    String::from("1")
                                } else {
                                    String::from("0")
                                },
                            );
                        }
                        if include_voice {
                            row.insert(String::from("channel_codec"), channel.codec.to_string());
                            row.insert(
                                String::from("channel_codec_quality"),
                                channel.codec_quality.to_string(),
                            );
                            row.insert(
                                String::from("channel_needed_talk_power"),
                                channel.needed_talk_power().to_string(),
                            );
                        }
                        if include_limits {
                            row.insert(
                                String::from("total_clients_family"),
                                self.family_client_count_in_channel(server_id, channel.id)
                                    .to_string(),
                            );
                            row.insert(
                                String::from("channel_maxclients"),
                                channel.maxclients.to_string(),
                            );
                            row.insert(
                                String::from("channel_maxfamilyclients"),
                                channel.maxfamilyclients.to_string(),
                            );
                        }
                        row
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        QueryResponse::ok_rows(rows)
    }

    pub(crate) fn handle_channelsubscribe(
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

        let mut requested_cids = Vec::new();
        if !request.option_groups.is_empty() {
            for group in &request.option_groups {
                if let Some(cid) = group.get("cid").and_then(|v| v.parse::<u32>().ok()) {
                    requested_cids.push(cid);
                }
            }
        } else if let Some(cid) = request
            .named_args
            .get("cid")
            .and_then(|v| v.parse::<u32>().ok())
        {
            requested_cids.push(cid);
        }
        if requested_cids.is_empty() {
            return QueryResponse::error(512, "cid is required");
        }

        let (actor, _actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };

        for &cid in &requested_cids {
            if !self.channel_exists(server_id, cid) {
                return QueryResponse::error(768, "channel not found");
            }
            if let Some(perms) = self.effective_permissions_for_client_in_channel_context(
                server_id,
                cid,
                actor.client_database_id,
            ) {
                let ignore_sub_power =
                    permission_value_or_default(&perms, &["b_channel_ignore_subscribe_power"]) > 0;
                if !ignore_sub_power {
                    let sub_power =
                        permission_power_max_or_default(&perms, &["i_channel_subscribe_power"]);
                    let needed_sub_power =
                        permission_value_or_default(&perms, &["i_channel_needed_subscribe_power"]);
                    if sub_power < needed_sub_power {
                        return self.insufficient_permission_response("i_channel_subscribe_power");
                    }
                }
                if let Some(max_subs) = perms.get("i_client_max_channel_subscriptions")
                    && max_subs.value >= 0
                    && !session.subscribed_channels.contains(&cid)
                    && (session.subscribed_channels.len() as i64) >= max_subs.value
                {
                    return self
                        .insufficient_permission_response("i_client_max_channel_subscriptions");
                }
            }
            session.subscribed_channels.insert(cid);
        }

        self.sync_session_subscribed_channels(server_id, session);
        QueryResponse::ok()
    }

    pub(crate) fn handle_channelunsubscribe(
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

        let mut requested_cids = Vec::new();
        if !request.option_groups.is_empty() {
            for group in &request.option_groups {
                if let Some(cid) = group.get("cid").and_then(|v| v.parse::<u32>().ok()) {
                    requested_cids.push(cid);
                }
            }
        } else if let Some(cid) = request
            .named_args
            .get("cid")
            .and_then(|v| v.parse::<u32>().ok())
        {
            requested_cids.push(cid);
        }
        if requested_cids.is_empty() {
            return QueryResponse::error(512, "cid is required");
        }

        for &cid in &requested_cids {
            if !self.channel_exists(server_id, cid) {
                return QueryResponse::error(768, "channel not found");
            }
            session.subscribed_channels.remove(&cid);
        }

        self.sync_session_subscribed_channels(server_id, session);
        QueryResponse::ok()
    }

    pub(crate) fn handle_channelsubscribeall(
        &mut self,
        session: &mut QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }
        let Some(server_id) = session.selected_virtual_server_id else {
            return QueryResponse::error(522, "virtual server selection required");
        };
        let (actor, _) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };

        let channel_ids = self
            .store
            .channels
            .get(&server_id)
            .map(|channels| channels.iter().map(|c| c.id).collect::<Vec<_>>())
            .unwrap_or_default();

        for cid in channel_ids {
            let allowed = self
                .effective_permissions_for_client_in_channel_context(
                    server_id,
                    cid,
                    actor.client_database_id,
                )
                .map(|perms| {
                    permission_value_or_default(&perms, &["b_channel_ignore_subscribe_power"]) > 0
                        || permission_power_max_or_default(&perms, &["i_channel_subscribe_power"])
                            >= permission_value_or_default(
                                &perms,
                                &["i_channel_needed_subscribe_power"],
                            )
                })
                .unwrap_or(true);
            if allowed {
                session.subscribed_channels.insert(cid);
            }
        }

        self.sync_session_subscribed_channels(server_id, session);
        QueryResponse::ok()
    }

    pub(crate) fn handle_channelunsubscribeall(
        &mut self,
        session: &mut QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }
        let Some(server_id) = session.selected_virtual_server_id else {
            return QueryResponse::error(522, "virtual server selection required");
        };

        session.subscribed_channels.clear();
        self.sync_session_subscribed_channels(server_id, session);
        QueryResponse::ok()
    }

    fn sync_session_subscribed_channels(&mut self, server_id: u32, session: &QuerySessionState) {
        if let Some(client) = self
            .store
            .online_clients
            .get_mut(&session.client_id)
            .filter(|c| c.server_id == server_id)
        {
            let joined = session
                .subscribed_channels
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join(",");
            client
                .extra_properties
                .insert(String::from("subscribed_channels"), joined);
        }
    }

    pub(crate) fn handle_channelfind(
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
        let normalized_pattern = pattern.trim_matches('%').to_ascii_lowercase();
        let Some(channels) = self.store.channels.get(&server_id) else {
            return QueryResponse::error(
                ERROR_DATABASE_EMPTY_RESULT,
                "database empty result set",
            );
        };

        let rows = channels
            .iter()
            .filter(|ch| ch.name.to_ascii_lowercase().contains(&normalized_pattern))
            .map(|ch| {
                let mut row = BTreeMap::new();
                row.insert(String::from("cid"), ch.id.to_string());
                row.insert(String::from("channel_name"), ch.name.clone());
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
}
