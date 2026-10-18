use std::collections::BTreeMap;

use crate::models::{
    WhisperTargetSelection, WHISPER_TARGET_CHANNEL, WHISPER_TARGET_CLIENT, WHISPER_TARGET_SELF,
    WHISPER_TARGET_SERVER_GROUP,
};
use crate::query::{CommandRequest, QueryResponse};
use crate::runtime::{
    ActiveMediaBroadcast, BaselineRuntime, QuerySessionState, check_required_permission,
    session_has_permission_actor, stable_web_client_database_id, stable_web_client_unique_identifier,
};

impl BaselineRuntime {
    pub(crate) fn handle_handshakebegin(
        &mut self,
        request: &CommandRequest,
        session: &mut QuerySessionState,
    ) -> QueryResponse {
        let public_key = request
            .named_args
            .get("public_key")
            .or_else(|| request.named_args.get("client_uid"))
            .or_else(|| request.named_args.get("client_unique_identifier"))
            .cloned()
            .filter(|v| !v.trim().is_empty());

        if let Some(pk) = public_key.as_deref() {
            let uid = stable_web_client_unique_identifier(pk);
            let cldbid = stable_web_client_database_id(&uid);
            session.actor_client_database_id_override = Some(cldbid);
            if let Some(online_client) = self.store.online_clients.get_mut(&session.client_id) {
                online_client.unique_identifier = uid;
                online_client.database_id = cldbid;
            }
        }

        let mut row = BTreeMap::new();
        row.insert(String::from("digest"), String::from("SHA-256"));
        row.insert(
            String::from("message"),
            String::from(
                "TeaSpeak, made with love and coffee by WolverinDEV (#QUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUE=)",
            ),
        );
        QueryResponse::ok_row(row)
    }

    pub(crate) fn handle_handshakeidentityproof(
        &mut self,
        request: &CommandRequest,
        _session: &mut QuerySessionState,
    ) -> QueryResponse {
        let proof = request
            .named_args
            .get("proof")
            .or_else(|| request.named_args.get("signature"))
            .map(|s| s.trim())
            .unwrap_or_default();
        if proof.is_empty() && !request.named_args.is_empty() {
            return QueryResponse::error(512, "proof is required");
        }
        QueryResponse::ok()
    }

    pub(crate) fn handle_rtcsessiondescribe(
        &mut self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) && !session.is_desktop_client {
            return QueryResponse::error(521, "login required");
        }

        let Some(sdp) = request
            .named_args
            .get("sdp")
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        else {
            return QueryResponse::error(512, "sdp is required");
        };
        let mode = request
            .named_args
            .get("mode")
            .map(|s| s.trim().to_ascii_lowercase())
            .unwrap_or_else(|| String::from("offer"));
        if !matches!(mode.as_str(), "offer" | "answer") {
            return QueryResponse::error(512, "mode must be offer or answer");
        }

        let mut row = BTreeMap::new();
        row.insert(String::from("mode"), String::from("answer"));
        row.insert(
            String::from("sdp"),
            if sdp.contains("a=setup:actpass") {
                sdp.replace("a=setup:actpass", "a=setup:passive")
            } else {
                format!(
                    "v=0\r\no=- 0 0 IN IP4 127.0.0.1\r\ns=BlackTeaSpeak-RTC\r\nt=0 0\r\na=setup:passive\r\n"
                )
            },
        );
        QueryResponse::ok_row(row)
    }

    pub(crate) fn handle_rtcsessionreset(
        &mut self,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) && !session.is_desktop_client {
            return QueryResponse::error(521, "login required");
        }
        if let Some(server_id) = session.selected_virtual_server_id {
            self.clear_client_media_broadcasts(server_id, session.client_id);
        }
        QueryResponse::ok()
    }

    pub(crate) fn handle_rtcicecandidate(
        &mut self,
        _request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) && !session.is_desktop_client {
            return QueryResponse::error(521, "login required");
        }
        // In TeaSpeak's WebRTC protocol, `rtcicecandidate` without a `candidate` parameter
        // (e.g. `rtcicecandidate return_code=...`) signals end-of-candidates (ICE gathering finished).
        QueryResponse::ok()
    }

    pub(crate) fn handle_broadcastvideo(
        &mut self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) && !session.is_desktop_client {
            return QueryResponse::error(521, "login required");
        }
        let Some(server_id) = session.selected_virtual_server_id else {
            return QueryResponse::error(522, "virtual server selection required");
        };
        let channel_id = self
            .store
            .online_clients
            .get(&session.client_id)
            .filter(|c| c.server_id == server_id)
            .map(|c| c.channel_id)
            .or(session.current_channel_id)
            .unwrap_or(1);

        let groups: Vec<&BTreeMap<String, String>> = if request.option_groups.is_empty() {
            vec![&request.named_args]
        } else {
            request.option_groups.iter().collect()
        };

        let actor_permissions = self
            .query_actor_effective_permissions(session)
            .ok()
            .map(|(_, p)| p);

        let mut rows = Vec::new();
        for (idx, group) in groups.into_iter().enumerate() {
            let broadcast_type = group
                .get("bt")
                .or_else(|| group.get("type"))
                .and_then(|v| {
                    let clean = v.trim_matches('\0').trim();
                    clean
                        .parse::<u32>()
                        .ok()
                        .or_else(|| clean.parse::<f64>().ok().map(|f| f.round() as u32))
                })
                .or_else(|| {
                    group.get("kind").map(|k| {
                        if k.trim_matches('\0').trim().eq_ignore_ascii_case("screen") {
                            1
                        } else {
                            0
                        }
                    })
                })
                .unwrap_or((idx as u32).min(1));
            if broadcast_type > 1 {
                return QueryResponse::error(512, "bt must be 0 (camera) or 1 (screen)");
            }

            let parsed_ssrc = group
                .get("ssrc")
                .and_then(|v| {
                    let clean = v.trim_matches('\0').trim();
                    clean
                        .parse::<u32>()
                        .ok()
                        .or_else(|| clean.parse::<f64>().ok().map(|f| f.round() as u32))
                })
                .unwrap_or(0);

            let is_web_active = group
                .get("active")
                .map(|v| v.trim_matches('\0').trim())
                .is_some_and(|v| v == "1" || v.eq_ignore_ascii_case("true"));

            let ssrc = if parsed_ssrc != 0 {
                parsed_ssrc
            } else if is_web_active {
                0x7000_0000_u32
                    .wrapping_add((session.client_id as u32).wrapping_mul(4))
                    .wrapping_add(broadcast_type)
            } else {
                0
            };

            let key = (server_id, session.client_id, broadcast_type);
            if ssrc == 0 {
                self.store.active_broadcasts.remove(&key);
                if let Ok(mut lock) = self.rtc_manager.viewer_slots.write() {
                    for slots in lock.values_mut() {
                        for slot in slots.iter_mut() {
                            if slot.assigned_broadcast == Some((session.client_id, broadcast_type))
                            {
                                slot.assigned_broadcast = None;
                            }
                        }
                    }
                }
                if let Some(client) = self.store.online_clients.get_mut(&session.client_id) {
                    let prop_key = if broadcast_type == 1 {
                        "client_flag_screen_broadcasting"
                    } else {
                        "client_flag_video_broadcasting"
                    };
                    client
                        .extra_properties
                        .insert(String::from(prop_key), String::from("0"));
                }
                continue;
            }

            if let Some(actor_permissions) = &actor_permissions {
                let perm_candidates: &[&str] = if broadcast_type == 1 {
                    &[
                        "b_video_screen_MustNotExist",
                        "b_client_C_screen_broadcast",
                        "b_video_camera",
                        "b_permission_modify_power_ignore",
                    ]
                } else {
                    &[
                        "b_video_camera",
                        "b_client_C_video_broadcast",
                        "b_permission_modify_power_ignore",
                    ]
                };
                // Only enforce if the permission is explicitly assigned and negated/zero
                let explicitly_denied = perm_candidates.iter().any(|name| {
                    actor_permissions
                        .get(*name)
                        .is_some_and(|a| a.value == 0 || a.negated)
                }) && check_required_permission(
                    actor_permissions,
                    &["b_permission_modify_power_ignore"],
                    "b_permission_modify_power_ignore",
                )
                .is_err();
                if explicitly_denied {
                    return self.insufficient_permission_response(if broadcast_type == 1 {
                        "b_video_screen"
                    } else {
                        "b_video_camera"
                    });
                }
            }

            self.rtc_manager
                .register_incoming_ssrc_type(session.client_id, ssrc, broadcast_type);

            let existing_joined = self
                .store
                .active_broadcasts
                .get(&key)
                .map(|b| b.joined_client_ids.clone())
                .unwrap_or_default();
            self.store.active_broadcasts.insert(
                key,
                ActiveMediaBroadcast {
                    server_id,
                    channel_id,
                    broadcaster_client_id: session.client_id,
                    broadcast_type,
                    ssrc,
                    joined_client_ids: existing_joined,
                },
            );
            if let Some(client) = self.store.online_clients.get_mut(&session.client_id) {
                let prop_key = if broadcast_type == 1 {
                    "client_flag_screen_broadcasting"
                } else {
                    "client_flag_video_broadcasting"
                };
                client
                    .extra_properties
                    .insert(String::from(prop_key), String::from("1"));
            }

            let mut row = BTreeMap::new();
            row.insert(String::from("bid"), session.client_id.to_string());
            row.insert(String::from("bt"), broadcast_type.to_string());
            row.insert(String::from("ssrc"), ssrc.to_string());
            rows.push(row);
        }

        self.broadcast_event(
            server_id,
            &crate::transport::TransportNotification::VideoBroadcastChanged {
                server_id,
                channel_id,
            },
        );

        if rows.is_empty() {
            QueryResponse::ok()
        } else {
            QueryResponse::ok_rows(rows)
        }
    }

    pub(crate) fn handle_broadcastaudio(
        &mut self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) && !session.is_desktop_client {
            return QueryResponse::error(521, "login required");
        }
        let Some(server_id) = session.selected_virtual_server_id else {
            return QueryResponse::error(522, "virtual server selection required");
        };
        let channel_id = self
            .store
            .online_clients
            .get(&session.client_id)
            .filter(|c| c.server_id == server_id)
            .map(|c| c.channel_id)
            .or(session.current_channel_id)
            .unwrap_or(1);

        let ssrc = request
            .named_args
            .get("ssrc")
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(0);

        let key = (server_id, session.client_id, 2);
        if ssrc == 0 {
            self.store.active_broadcasts.remove(&key);
            return QueryResponse::ok();
        }

        let existing_joined = self
            .store
            .active_broadcasts
            .get(&key)
            .map(|b| b.joined_client_ids.clone())
            .unwrap_or_default();
        self.store.active_broadcasts.insert(
            key,
            ActiveMediaBroadcast {
                server_id,
                channel_id,
                broadcaster_client_id: session.client_id,
                broadcast_type: 2,
                ssrc,
                joined_client_ids: existing_joined,
            },
        );

        let mut row = BTreeMap::new();
        row.insert(String::from("bid"), session.client_id.to_string());
        row.insert(String::from("bt"), String::from("2"));
        row.insert(String::from("ssrc"), ssrc.to_string());
        QueryResponse::ok_row(row)
    }

    pub(crate) fn handle_broadcastvideojoin(
        &mut self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) && !session.is_desktop_client {
            return QueryResponse::error(521, "login required");
        }
        let Some(server_id) = session.selected_virtual_server_id else {
            return QueryResponse::error(522, "virtual server selection required");
        };
        let Some(broadcaster_id) = request
            .named_args
            .get("bid")
            .or_else(|| request.named_args.get("clid"))
            .or_else(|| request.named_args.get("scl"))
            .and_then(|v| v.parse::<u64>().ok())
        else {
            return QueryResponse::error(512, "bid is required");
        };
        let requested_bt = request
            .named_args
            .get("bt")
            .or_else(|| request.named_args.get("type"))
            .and_then(|v| v.parse::<u32>().ok());

        let target_key = if let Some(bt) = requested_bt {
            Some((server_id, broadcaster_id, bt))
        } else {
            [0_u32, 1_u32]
                .into_iter()
                .map(|bt| (server_id, broadcaster_id, bt))
                .find(|k| self.store.active_broadcasts.contains_key(k))
        };

        let Some(key) = target_key.filter(|k| self.store.active_broadcasts.contains_key(k)) else {
            return QueryResponse::error(768, "active video broadcast not found");
        };

        let broadcast = self.store.active_broadcasts.get_mut(&key).unwrap();
        broadcast.joined_client_ids.insert(session.client_id);

        let mut row = BTreeMap::new();
        row.insert(String::from("bid"), broadcast.broadcaster_client_id.to_string());
        row.insert(String::from("scl"), broadcast.broadcaster_client_id.to_string());
        row.insert(String::from("bt"), broadcast.broadcast_type.to_string());
        row.insert(String::from("ssrc"), broadcast.ssrc.to_string());
        QueryResponse::ok_row(row)
    }

    pub(crate) fn handle_broadcastaudiojoin(
        &mut self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) && !session.is_desktop_client {
            return QueryResponse::error(521, "login required");
        }
        let Some(server_id) = session.selected_virtual_server_id else {
            return QueryResponse::error(522, "virtual server selection required");
        };
        let Some(broadcaster_id) = request
            .named_args
            .get("bid")
            .or_else(|| request.named_args.get("clid"))
            .or_else(|| request.named_args.get("scl"))
            .and_then(|v| v.parse::<u64>().ok())
        else {
            return QueryResponse::error(512, "bid is required");
        };

        let key = (server_id, broadcaster_id, 2);
        let Some(broadcast) = self.store.active_broadcasts.get_mut(&key) else {
            return QueryResponse::error(768, "active audio broadcast not found");
        };
        broadcast.joined_client_ids.insert(session.client_id);

        let mut row = BTreeMap::new();
        row.insert(String::from("bid"), broadcast.broadcaster_client_id.to_string());
        row.insert(String::from("scl"), broadcast.broadcaster_client_id.to_string());
        row.insert(String::from("bt"), String::from("2"));
        row.insert(String::from("ssrc"), broadcast.ssrc.to_string());
        QueryResponse::ok_row(row)
    }

    pub(crate) fn handle_broadcastleave(
        &mut self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) && !session.is_desktop_client {
            return QueryResponse::error(521, "login required");
        }
        let Some(server_id) = session.selected_virtual_server_id else {
            return QueryResponse::error(522, "virtual server selection required");
        };
        let broadcaster_id = request
            .named_args
            .get("bid")
            .or_else(|| request.named_args.get("clid"))
            .or_else(|| request.named_args.get("scl"))
            .and_then(|v| v.parse::<u64>().ok());
        let requested_bt = request
            .named_args
            .get("bt")
            .or_else(|| request.named_args.get("type"))
            .and_then(|v| v.parse::<u32>().ok());

        for ((sid, bid, bt), broadcast) in &mut self.store.active_broadcasts {
            if *sid == server_id
                && broadcaster_id.is_none_or(|target_bid| *bid == target_bid)
                && requested_bt.is_none_or(|target_bt| *bt == target_bt)
            {
                broadcast.joined_client_ids.remove(&session.client_id);
            }
        }
        QueryResponse::ok()
    }

    pub(crate) fn handle_broadcastlist(
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
        let filter_cid = request
            .named_args
            .get("cid")
            .and_then(|v| v.parse::<u32>().ok());

        let rows = self.active_broadcast_rows(server_id, filter_cid);
        QueryResponse::ok_rows(rows)
    }

    pub fn active_broadcast_rows(
        &self,
        server_id: u32,
        filter_cid: Option<u32>,
    ) -> Vec<BTreeMap<String, String>> {
        self.store
            .active_broadcasts
            .values()
            .filter(|b| b.server_id == server_id && filter_cid.is_none_or(|cid| b.channel_id == cid))
            .map(|b| {
                let mut row = BTreeMap::new();
                row.insert(String::from("clid"), b.broadcaster_client_id.to_string());
                row.insert(String::from("sclid"), b.broadcaster_client_id.to_string());
                row.insert(String::from("scl"), b.broadcaster_client_id.to_string());
                row.insert(String::from("bid"), b.broadcaster_client_id.to_string());
                row.insert(String::from("cid"), b.channel_id.to_string());
                row.insert(String::from("bt"), b.broadcast_type.to_string());
                row.insert(String::from("ssrc"), b.ssrc.to_string());
                row
            })
            .collect()
    }


    pub fn clear_client_media_broadcasts(&mut self, server_id: u32, client_id: u64) {
        let mut affected_channels = std::collections::BTreeSet::new();
        for ((sid, bid, bt), b) in &self.store.active_broadcasts {
            if *sid == server_id && *bid == client_id && (*bt == 0 || *bt == 1) {
                affected_channels.insert(b.channel_id);
            }
        }
        self.store
            .active_broadcasts
            .retain(|(sid, bid, _), _| !(*sid == server_id && *bid == client_id));
        for ((sid, _, _), broadcast) in &mut self.store.active_broadcasts {
            if *sid == server_id {
                broadcast.joined_client_ids.remove(&client_id);
            }
        }
        self.rtc_manager.unassign_broadcaster(client_id);
        let _ = self.rtc_manager.unassign_viewer_slot(client_id, None, None);
        for channel_id in affected_channels {
            self.broadcast_event(
                server_id,
                &crate::transport::TransportNotification::VideoBroadcastChanged {
                    server_id,
                    channel_id,
                },
            );
        }
    }

    pub(crate) fn handle_whispersessioninitialize(
        &mut self,
        request: &CommandRequest,
        session: &mut QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) && !session.is_desktop_client {
            return QueryResponse::error(521, "login required");
        }

        let groups = if request.option_groups.is_empty() {
            vec![request.named_args.clone()]
        } else {
            request
                .option_groups
                .iter()
                .map(|g| {
                    let mut merged = request.named_args.clone();
                    merged.extend(g.clone());
                    merged
                })
                .collect()
        };

        let Some(first_row) = groups.first() else {
            return QueryResponse::error(512, "missing whispersessioninitialize payload");
        };
        let Some(ssrc) = first_row
            .get("ssrc")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return QueryResponse::error(512, "missing whisper ssrc");
        };

        let mut selection = WhisperTargetSelection::default();
        for row in &groups {
            let row_ssrc = row
                .get("ssrc")
                .and_then(|value| value.parse::<u32>().ok())
                .unwrap_or(ssrc);
            if row_ssrc != ssrc {
                return QueryResponse::error(512, "whisper ssrc must stay consistent");
            }

            let target_type = row
                .get("type")
                .and_then(|value| value.parse::<u32>().ok())
                .unwrap_or_default();
            let target = row
                .get("target")
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or_default();
            let id = row
                .get("id")
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or_default();

            let effective_target = if target != 0 { target } else { id };

            match target_type {
                WHISPER_TARGET_SELF => {
                    if !(target == 0 && id == 0) {
                        return QueryResponse::error(512, "invalid echo whisper target");
                    }
                    selection.echo_self = true;
                }
                WHISPER_TARGET_CHANNEL => {
                    let Ok(channel_id) = u32::try_from(effective_target) else {
                        return QueryResponse::error(512, "invalid whisper channel target");
                    };
                    if channel_id == 0 {
                        return QueryResponse::error(512, "invalid whisper channel target");
                    }
                    selection.channel_ids.insert(channel_id);
                }
                WHISPER_TARGET_CLIENT => {
                    if effective_target == 0 {
                        return QueryResponse::error(512, "invalid whisper client target");
                    }
                    selection.client_ids.insert(effective_target);
                }
                WHISPER_TARGET_SERVER_GROUP => {
                    let Ok(group_id) = u32::try_from(effective_target) else {
                        return QueryResponse::error(512, "invalid whisper server-group target");
                    };
                    if group_id == 0 {
                        return QueryResponse::error(512, "invalid whisper server-group target");
                    }
                    selection.server_group_ids.insert(group_id);
                }
                _ => return QueryResponse::error(512, "unsupported whisper target"),
            }

        }

        if selection.is_empty() {
            return QueryResponse::error(512, "missing whisper target");
        }

        session.whisper_targets = Some(selection.clone());
        if let Some(online_client) = self.store.online_clients.get_mut(&session.client_id) {
            online_client.whisper_targets = Some(selection);
        }

        QueryResponse::ok()
    }

    pub(crate) fn handle_whispersessionreset(
        &mut self,
        session: &mut QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) && !session.is_desktop_client {
            return QueryResponse::error(521, "login required");
        }
        session.whisper_targets = None;
        if let Some(online_client) = self.store.online_clients.get_mut(&session.client_id) {
            online_client.whisper_targets = None;
        }
        QueryResponse::ok()
    }
}
