use super::*;
use std::collections::{BTreeMap, BTreeSet};
use std::io::{self};

use anyhow::Result;

use crate::runtime::BaselineRuntime;
use crate::transport::TransportNotification;

pub(crate) fn presence_with_channel_id(presence: &BlackTeaWebPresence, channel_id: u32) -> BlackTeaWebPresence {
    let mut overridden = presence.clone();
    overridden.channel_id = channel_id;
    overridden
}

pub(crate) fn channel_parent_ids_for_server(runtime: &BaselineRuntime, server_id: u32) -> BTreeMap<u32, u32> {
    runtime
        .web_channel_rows(server_id)
        .into_iter()
        .filter_map(|row| {
            Some((
                row.get("cid")?.parse::<u32>().ok()?,
                row.get("cpid")?.parse::<u32>().ok()?,
            ))
        })
        .collect()
}

pub(crate) fn top_level_hidden_channel_ids(
    parent_ids: &BTreeMap<u32, u32>,
    hidden_channel_ids: &BTreeSet<u32>,
) -> Vec<u32> {
    let mut roots = hidden_channel_ids
        .iter()
        .copied()
        .filter(|channel_id| {
            let mut current_parent_id = parent_ids.get(channel_id).copied().unwrap_or(0);
            while current_parent_id != 0 {
                if hidden_channel_ids.contains(&current_parent_id) {
                    return false;
                }
                current_parent_id = parent_ids.get(&current_parent_id).copied().unwrap_or(0);
            }
            true
        })
        .collect::<Vec<_>>();
    roots.sort_unstable();
    roots
}

pub(crate) fn visibility_transition_frames(
    runtime: &BaselineRuntime,
    server_id: u32,
    client_id: u64,
    before_visible_channel_ids: &BTreeSet<u32>,
    after_visible_channel_ids: &BTreeSet<u32>,
    created_channel_ids: &BTreeSet<u32>,
    deleted_channel_ids: &BTreeSet<u32>,
) -> Result<Vec<String>> {
    let gained_channel_ids = after_visible_channel_ids
        .difference(before_visible_channel_ids)
        .copied()
        .filter(|channel_id| !created_channel_ids.contains(channel_id))
        .collect::<BTreeSet<_>>();
    let lost_channel_ids = before_visible_channel_ids
        .difference(after_visible_channel_ids)
        .copied()
        .filter(|channel_id| !deleted_channel_ids.contains(channel_id))
        .collect::<BTreeSet<_>>();

    let mut frames = Vec::new();
    let parent_ids = channel_parent_ids_for_server(runtime, server_id);
    for channel_id in top_level_hidden_channel_ids(&parent_ids, &lost_channel_ids) {
        frames.push(command_frame(
            "notifychannelhide",
            vec![row_map([("cid", channel_id.to_string())])],
        )?);
    }

    if !gained_channel_ids.is_empty() {
        for row in runtime
            .web_channel_rows_for_visibility(server_id, after_visible_channel_ids)
            .into_iter()
            .filter(|row| {
                row.get("cid")
                    .and_then(|value| value.parse::<u32>().ok())
                    .is_some_and(|channel_id| gained_channel_ids.contains(&channel_id))
            })
        {
            frames.push(command_frame("notifychannelshow", vec![row])?);
        }

        let gained_client_rows = runtime.web_visible_client_rows_excluding_in_channels(
            server_id,
            Some(client_id),
            &gained_channel_ids,
        );
        if !gained_client_rows.is_empty() {
            frames.push(command_frame("notifycliententerview", gained_client_rows)?);
        }
    }

    Ok(frames)
}

pub(crate) fn visibility_aware_transport_broadcasts(
    sessions: &SharedBlackTeaWebSessions,
    runtime: &BaselineRuntime,
    origin_client_id: Option<u64>,
    notifications: &[TransportNotification],
) -> Result<Vec<BlackTeaWebFrameBroadcast>> {
    let mut broadcasts = Vec::new();
    let mut sessions = sessions
        .lock()
        .map_err(|_| io::Error::other("BlackTeaWeb session registry lock poisoned"))?;

    for session in sessions.values_mut() {
        let before_visible_channel_ids = session.visible_channel_ids.clone();
        let after_visible_channel_ids = runtime.web_visible_channel_ids_for_client(
            session.presence.server_id,
            session.client_database_id,
            Some(session.presence.channel_id),
        );
        let frames = session_frames_for_transport_notifications(
            session,
            runtime,
            origin_client_id,
            notifications,
            &before_visible_channel_ids,
            &after_visible_channel_ids,
        )?;
        session.visible_channel_ids = after_visible_channel_ids;
        broadcasts.extend(frames.into_iter().map(|frame| BlackTeaWebFrameBroadcast::Client {
            client_id: session.presence.client_id,
            frame,
        }));
    }

    for notification in notifications {
        if let TransportNotification::TextMessage {
            target,
            invoker_id,
            invoker_name,
            invoker_uid,
        } = notification
        {
            broadcasts.extend(frame_broadcasts_from_transport_notifications(
                runtime,
                origin_client_id,
                &[TransportNotification::TextMessage {
                    target: target.clone(),
                    invoker_id: *invoker_id,
                    invoker_name: invoker_name.clone(),
                    invoker_uid: invoker_uid.clone(),
                }],
            ));
            continue;
        }

        if let TransportNotification::ClientPoke {
            target_client_id,
            invoker_id,
            invoker_name,
            invoker_uid,
            message,
            ..
        } = notification
        {
            broadcasts.push(BlackTeaWebFrameBroadcast::Client {
                client_id: *target_client_id,
                frame: command_frame(
                    "notifyclientpoke",
                    vec![row_map([
                        ("invokerid", invoker_id.to_string()),
                        ("invokername", invoker_name.clone()),
                        ("invokeruid", invoker_uid.clone()),
                        ("msg", message.clone()),
                    ])],
                )?,
            });
        }
    }

    Ok(broadcasts)
}

pub(crate) fn session_frames_for_transport_notifications(
    session: &RegisteredBlackTeaWebSession,
    runtime: &BaselineRuntime,
    origin_client_id: Option<u64>,
    notifications: &[TransportNotification],
    before_visible_channel_ids: &BTreeSet<u32>,
    after_visible_channel_ids: &BTreeSet<u32>,
) -> Result<Vec<String>> {
    let server_id = session.presence.server_id;
    let suppress_origin_peer_frames = Some(session.presence.client_id) == origin_client_id;
    let created_channel_ids = notifications
        .iter()
        .filter_map(|notification| match notification {
            TransportNotification::ChannelCreated { server_id, channel, .. }
                if *server_id == session.presence.server_id =>
            {
                Some(channel.id)
            }
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    let deleted_channel_ids = notifications
        .iter()
        .filter_map(|notification| match notification {
            TransportNotification::ChannelDeleted { server_id, channel, .. }
                if *server_id == session.presence.server_id =>
            {
                Some(channel.id)
            }
            _ => None,
        })
        .collect::<BTreeSet<_>>();

    let mut frames = visibility_transition_frames(
        runtime,
        server_id,
        session.presence.client_id,
        before_visible_channel_ids,
        after_visible_channel_ids,
        &created_channel_ids,
        &deleted_channel_ids,
    )?;

    for notification in notifications {
        match notification {
            TransportNotification::ClientEnterView {
                presence,
                from_channel_id,
                reason_id,
            } if presence.server_id == server_id => {
                let bridged_presence = runtime
                    .online_client_snapshot(presence.server_id, presence.client_id)
                    .map(blackteaweb_presence_from_snapshot)
                    .unwrap_or_else(|| blackteaweb_presence_from_transport_presence(presence));

                if session.presence.client_id == presence.client_id {
                    if from_channel_id.is_some() && !suppress_origin_peer_frames {
                        frames.push(command_frame(
                            "notifycliententerview",
                            vec![presence_enter_view_row(
                                &bridged_presence,
                                *from_channel_id,
                                *reason_id,
                            )],
                        )?);
                    }
                } else if !suppress_origin_peer_frames
                    && after_visible_channel_ids.contains(&bridged_presence.channel_id)
                {
                    frames.push(command_frame(
                        "notifycliententerview",
                        vec![presence_enter_view_row(
                            &bridged_presence,
                            *from_channel_id,
                            *reason_id,
                        )],
                    )?);
                }
            }
            TransportNotification::ClientUpdated {
                server_id,
                before,
                after,
            } if *server_id == session.presence.server_id => {
                let before_presence = blackteaweb_presence_from_snapshot(before.clone());
                let after_presence = blackteaweb_presence_from_snapshot(after.clone());
                if let Some(row) = presence_update_row(&before_presence, &after_presence)
                    && (session.presence.client_id == after.id
                        || (!suppress_origin_peer_frames
                            && after_visible_channel_ids.contains(&after_presence.channel_id)))
                {
                    frames.push(command_frame("notifyclientupdated", vec![row])?);
                }
                if !suppress_origin_peer_frames
                    && (after.client_type_exact == 4
                        || after.unique_identifier.starts_with("compat-musicbot-"))
                {
                    let music_sync_changed = before.extra_properties.get("_music_sync_state")
                        != after.extra_properties.get("_music_sync_state")
                        || before.extra_properties.get("player_volume")
                            != after.extra_properties.get("player_volume");
                    let music_loaded_changed = before.extra_properties.get("_music_loaded_songs")
                        != after.extra_properties.get("_music_loaded_songs");
                    if (music_sync_changed || music_loaded_changed)
                        && let Some(payload) =
                            runtime.music_bot_notify_payload_by_identifier(*server_id, after.id)
                    {
                        if music_loaded_changed && payload.playlist_id > 0 {
                            let prev_loaded = before
                                .extra_properties
                                .get("_music_loaded_songs")
                                .map(String::as_str)
                                .unwrap_or("");
                            for loaded_row in &payload.loaded_songs {
                                if let Some(sid_str) = loaded_row.get("song_id")
                                    && !prev_loaded.split(',').any(|entry| entry == sid_str)
                                {
                                    frames.push(command_frame(
                                        "notifyplaylistsongloaded",
                                        vec![loaded_row.clone()],
                                    )?);
                                }
                            }
                        }
                        if music_sync_changed {
                            frames.push(command_frame(
                                "notifymusicplayerinfo",
                                vec![payload.player_info],
                            )?);
                            frames.push(command_frame(
                                "notifymusicplayersongchange",
                                vec![payload.song_change],
                            )?);
                            frames.push(command_frame(
                                "notifymusicstatusupdate",
                                vec![payload.status_update],
                            )?);
                        }
                    }
                }
            }
            TransportNotification::ClientMoved {
                presence,
                from_channel_id,
                reason_id,
                reason_message,
                invoker_id,
                invoker_name,
                invoker_uid,
            } if presence.server_id == server_id => {
                let bridged_presence = runtime
                    .online_client_snapshot(presence.server_id, presence.client_id)
                    .map(blackteaweb_presence_from_snapshot)
                    .unwrap_or_else(|| blackteaweb_presence_from_transport_presence(presence));

                if session.presence.client_id == presence.client_id {
                    frames.push(command_frame(
                        "notifyclientmoved",
                        vec![presence_move_row_for_invoker(
                            &bridged_presence,
                            *from_channel_id,
                            *reason_id,
                            reason_message,
                            *invoker_id,
                            invoker_name,
                            invoker_uid,
                        )],
                    )?);
                    continue;
                }
                if suppress_origin_peer_frames {
                    continue;
                }

                let source_visible = before_visible_channel_ids.contains(from_channel_id);
                let target_visible = after_visible_channel_ids.contains(&bridged_presence.channel_id);
                if source_visible && target_visible {
                    frames.push(command_frame(
                        "notifyclientmoved",
                        vec![presence_move_row_for_invoker(
                            &bridged_presence,
                            *from_channel_id,
                            *reason_id,
                            reason_message,
                            *invoker_id,
                            invoker_name,
                            invoker_uid,
                        )],
                    )?);
                } else if source_visible {
                    frames.push(command_frame(
                        "notifyclientleftview",
                        vec![presence_left_view_row_for_invoker(
                            &presence_with_channel_id(&bridged_presence, *from_channel_id),
                            Some(bridged_presence.channel_id),
                            *reason_id,
                            reason_message,
                            *invoker_id,
                            invoker_name,
                            invoker_uid,
                            None,
                        )],
                    )?);
                } else if target_visible {
                    frames.push(command_frame(
                        "notifycliententerview",
                        vec![presence_enter_view_row(
                            &bridged_presence,
                            Some(*from_channel_id),
                            *reason_id,
                        )],
                    )?);
                }
            }
            TransportNotification::ClientLeftView {
                presence,
                to_channel_id,
                reason_id,
                reason_message,
                invoker_id,
                invoker_name,
                invoker_uid,
                ban_time,
            } if presence.server_id == server_id => {
                let mut bridged_presence = runtime
                    .online_client_snapshot(presence.server_id, presence.client_id)
                    .map(blackteaweb_presence_from_snapshot)
                    .unwrap_or_else(|| blackteaweb_presence_from_transport_presence(presence));
                bridged_presence.channel_id = presence.channel_id;
                bridged_presence
                    .client_state
                    .insert(String::from("client_nickname"), presence.login_name.clone());
                bridged_presence.client_state.insert(
                    String::from("client_unique_identifier"),
                    presence.unique_identifier.clone(),
                );
                bridged_presence.client_state.insert(
                    String::from("client_type"),
                    presence.client_type.to_string(),
                );
                bridged_presence.client_state.insert(
                    String::from("client_type_exact"),
                    presence.client_type.to_string(),
                );

                if session.presence.client_id == presence.client_id {
                    frames.push(command_frame(
                        "notifyclientleftview",
                        vec![presence_left_view_row_for_invoker(
                            &bridged_presence,
                            *to_channel_id,
                            *reason_id,
                            reason_message,
                            *invoker_id,
                            invoker_name,
                            invoker_uid,
                            *ban_time,
                        )],
                    )?);
                } else if !suppress_origin_peer_frames
                    && before_visible_channel_ids.contains(&presence.channel_id)
                {
                    frames.push(command_frame(
                        "notifyclientleftview",
                        vec![presence_left_view_row_for_invoker(
                            &bridged_presence,
                            *to_channel_id,
                            *reason_id,
                            reason_message,
                            *invoker_id,
                            invoker_name,
                            invoker_uid,
                            *ban_time,
                        )],
                    )?);
                }
            }
            TransportNotification::ServerEdited {
                server_id,
                before,
                after,
                invoker_id,
                invoker_name,
            } if *server_id == session.presence.server_id && !suppress_origin_peer_frames => {
                if let Some(row) = server_edited_row(before, after, *invoker_id, invoker_name) {
                    frames.push(command_frame("notifyserveredited", vec![row])?);
                }
            }
            TransportNotification::ChannelCreated {
                server_id,
                channel,
                invoker_id,
                invoker_name,
            } if *server_id == session.presence.server_id
                && !suppress_origin_peer_frames
                && after_visible_channel_ids.contains(&channel.id) =>
            {
                if let Some(mut row) =
                    runtime.web_channel_row_for_visibility(*server_id, channel.id, after_visible_channel_ids)
                {
                    row.insert(String::from("invokerid"), invoker_id.to_string());
                    row.insert(String::from("invokername"), invoker_name.clone());
                    row.insert(
                        String::from("channel_description"),
                        channel.description.clone(),
                    );
                    frames.push(command_frame("notifychannelcreated", vec![row])?);
                }
            }
            TransportNotification::ChannelDeleted {
                server_id,
                channel,
                invoker_id,
                invoker_name,
            } if *server_id == session.presence.server_id
                && !suppress_origin_peer_frames
                && before_visible_channel_ids.contains(&channel.id) =>
            {
                frames.push(command_frame(
                    "notifychanneldeleted",
                    vec![row_map([
                        ("cid", channel.id.to_string()),
                        ("cpid", channel.parent_id.to_string()),
                        ("invokerid", invoker_id.to_string()),
                        ("invokername", invoker_name.clone()),
                    ])],
                )?);
            }
            TransportNotification::ChannelMoved {
                server_id,
                channel,
                invoker_id,
                invoker_name,
                ..
            } if *server_id == session.presence.server_id && !suppress_origin_peer_frames => {
                let was_visible = before_visible_channel_ids.contains(&channel.id);
                let is_visible = after_visible_channel_ids.contains(&channel.id);
                if was_visible && is_visible
                    && let Some(mut row) = runtime
                        .web_channel_row_for_visibility(*server_id, channel.id, after_visible_channel_ids)
                {
                    let order = row.get("channel_order").cloned().unwrap_or_else(|| String::from("0"));
                    row.insert(String::from("order"), order);
                    row.insert(String::from("invokerid"), invoker_id.to_string());
                    row.insert(String::from("invokername"), invoker_name.clone());
                    frames.push(command_frame("notifychannelmoved", vec![row])?);
                }
            }
            TransportNotification::ChannelEdited {
                server_id,
                channel,
                description_changed,
                invoker_id,
                invoker_name,
                ..
            } if *server_id == session.presence.server_id && !suppress_origin_peer_frames => {
                let was_visible = before_visible_channel_ids.contains(&channel.id);
                let is_visible = after_visible_channel_ids.contains(&channel.id);
                if was_visible && is_visible {
                    if *description_changed {
                        frames.push(command_frame(
                            "notifychanneldescriptionchanged",
                            vec![row_map([("cid", channel.id.to_string())])],
                        )?);
                    }
                    if let Some(mut row) = runtime
                        .web_channel_row_for_visibility(*server_id, channel.id, after_visible_channel_ids)
                    {
                        row.insert(String::from("invokerid"), invoker_id.to_string());
                        row.insert(String::from("invokername"), invoker_name.clone());
                        row.insert(
                            String::from("channel_description"),
                            channel.description.clone(),
                        );
                        frames.push(command_frame("notifychanneledited", vec![row])?);
                    }
                }
            }
            TransportNotification::VideoBroadcastChanged { server_id, .. }
                if *server_id == session.presence.server_id =>
            {
                let mut broadcast_rows = runtime.active_broadcast_rows(*server_id, None);
                if broadcast_rows.is_empty() {
                    broadcast_rows.push(BTreeMap::from([(
                        String::from("empty"),
                        String::from("1"),
                    )]));
                }
                frames.push(command_frame("notifybroadcasts", broadcast_rows)?);
            }
            _ => {}
        }
    }

    Ok(frames)
}

pub(crate) fn connected_needed_permission_frame_for_presence(
    runtime: &BaselineRuntime,
    presence: &BlackTeaWebPresence,
) -> Result<Option<String>> {
    let Some(client_database_id) = presence
        .client_state
        .get("client_database_id")
        .and_then(|value| value.parse::<u64>().ok())
    else {
        return Ok(None);
    };

    let Some(permission_rows) = runtime.web_client_needed_permission_rows(
        presence.server_id,
        presence.channel_id,
        client_database_id,
    ) else {
        return Ok(None);
    };

    Ok(Some(command_frame(
        "notifyclientneededpermissions",
        permission_rows,
    )?))
}

