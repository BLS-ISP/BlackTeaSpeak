use super::*;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs::{self, File};
use std::io::{self, BufReader};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use rcgen::generate_simple_self_signed;
use rustls::pki_types::{PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::{ServerConfig, ServerConnection, StreamOwned};
use serde::Deserialize;
use serde_json::{Map, Value, json};
use tungstenite::error::Error as WebSocketError;
use tungstenite::protocol::{CloseFrame, frame::coding::CloseCode};
use tungstenite::{Message, accept};
use wtransport::{Endpoint, ServerConfig as WTransportServerConfig, Identity};

use crate::file_transfer::{
    FileEntryInfo, FileTransferError, FileTransferEvent, FileTransferRegistry,
    PreparedFileTransfer,
};
use crate::query::{CommandRequest, QueryResponse, encode_query_value};
use crate::models::{
    WhisperTargetSelection, WHISPER_TARGET_CHANNEL, WHISPER_TARGET_CLIENT,
    WHISPER_TARGET_SERVER_GROUP, WHISPER_TARGET_SELF,
};
use crate::runtime::{
    AntiFloodSessionState, BaselineRuntime, ChannelSnapshot, MusicBotNotifyPayload,
    OnlineClientSnapshot, QuerySessionState, ServerSnapshot,
    WebServerGroupMutationError, WebServerInitInfo, create_baseline_runtime,
    create_baseline_runtime_with_state_path, stable_web_client_database_id,
    stable_web_client_unique_identifier,
};
use crate::transport::{SessionPresence, TransportNotification};

#[derive(Clone)]
pub struct BlackTeaWebNotificationBridge {
    pub(crate) sessions: SharedBlackTeaWebSessions,
}

#[derive(Clone)]
pub(crate) struct BlackTeaWebRtcNotificationBridge {
    pub(crate) sessions: SharedBlackTeaWebSessions,
}

pub(crate) fn permission_refresh_scope(command: &str) -> PermissionRefreshScope {
    match command {
        "servergroupaddperm" | "servergroupdelperm" => PermissionRefreshScope {
            needed_permissions: true,
            server_groups: true,
            channel_groups: false,
        },
        "servergroupdel" => PermissionRefreshScope {
            needed_permissions: true,
            server_groups: true,
            channel_groups: false,
        },
        "servergrouprename" | "servergroupadd" | "servergroupcopy" => PermissionRefreshScope {
            needed_permissions: false,
            server_groups: true,
            channel_groups: false,
        },
        "channelgroupaddperm" | "channelgroupdelperm" => PermissionRefreshScope {
            needed_permissions: true,
            server_groups: false,
            channel_groups: true,
        },
        "channelgroupdel" => PermissionRefreshScope {
            needed_permissions: true,
            server_groups: false,
            channel_groups: true,
        },
        "channelgrouprename" | "channelgroupadd" | "channelgroupcopy" => PermissionRefreshScope {
            needed_permissions: false,
            server_groups: false,
            channel_groups: true,
        },
        "servergroupaddclient"
        | "servergroupdelclient"
        | "tokenuse"
        | "setclientchannelgroup"
        | "channeladdperm"
        | "channeldelperm"
        | "clientaddperm"
        | "clientdelperm"
        | "channelclientaddperm"
        | "channelclientdelperm" => PermissionRefreshScope {
            needed_permissions: true,
            server_groups: false,
            channel_groups: false,
        },
        _ => PermissionRefreshScope::default(),
    }
}

impl BlackTeaWebNotificationBridge {
    pub fn broadcast_transport_notifications(
        &self,
        runtime: &BaselineRuntime,
        origin_client_id: Option<u64>,
        notifications: &[TransportNotification],
    ) -> Result<()> {
        let frames = visibility_aware_transport_broadcasts(
            &self.sessions,
            runtime,
            origin_client_id,
            notifications,
        )?;
        broadcast_queued_frames(&self.sessions, &frames)
    }

    pub(crate) fn broadcast_permission_refreshes(
        &self,
        runtime: &BaselineRuntime,
        server_id: u32,
        scope: PermissionRefreshScope,
    ) -> Result<()> {
        if scope.is_empty() {
            return Ok(());
        }

        broadcast_permission_refreshes(
            &self.sessions,
            runtime,
            &[BlackTeaWebPermissionRefresh { server_id, scope }],
        )
    }
}

pub(crate) fn frame_broadcasts_from_transport_notifications(
    runtime: &BaselineRuntime,
    origin_client_id: Option<u64>,
    notifications: &[TransportNotification],
) -> Vec<BlackTeaWebFrameBroadcast> {
    let mut frames = Vec::new();
    for notification in notifications {
        let mut notification_frames = match notification {
            TransportNotification::ClientEnterView {
                presence,
                from_channel_id,
                reason_id,
            } => runtime
                .online_client_snapshot(presence.server_id, presence.client_id)
                .map(|snapshot| BlackTeaWebFrameBroadcast::Server {
                    server_id: presence.server_id,
                    exclude_client_id: origin_client_id,
                    frame: command_frame(
                        "notifycliententerview",
                        vec![presence_enter_view_row(
                            &blackteaweb_presence_from_snapshot(snapshot),
                            *from_channel_id,
                            *reason_id,
                        )],
                    )
                    .expect("notifycliententerview should encode"),
                })
                .into_iter()
                .collect(),
            TransportNotification::ClientUpdated {
                server_id,
                before,
                after,
            } => presence_update_row(
                &blackteaweb_presence_from_snapshot(before.clone()),
                &blackteaweb_presence_from_snapshot(after.clone()),
            )
            .map(|row| BlackTeaWebFrameBroadcast::Server {
                server_id: *server_id,
                exclude_client_id: origin_client_id,
                frame: command_frame("notifyclientupdated", vec![row])
                    .expect("notifyclientupdated should encode"),
            })
            .into_iter()
            .collect(),
            TransportNotification::ClientPoke {
                target_client_id,
                invoker_id,
                invoker_name,
                invoker_uid,
                message,
                ..
            } => vec![BlackTeaWebFrameBroadcast::Client {
                client_id: *target_client_id,
                frame: command_frame(
                    "notifyclientpoke",
                    vec![row_map([
                        ("invokerid", invoker_id.to_string()),
                        ("invokername", invoker_name.clone()),
                        ("invokeruid", invoker_uid.clone()),
                        ("msg", message.clone()),
                    ])],
                )
                .expect("notifyclientpoke should encode"),
            }],
            TransportNotification::ClientMoved {
                presence,
                from_channel_id,
                reason_id,
                reason_message,
                invoker_id,
                invoker_name,
                invoker_uid,
            } => {
                let frame = command_frame(
                    "notifyclientmoved",
                    vec![presence_move_row_for_invoker(
                        &blackteaweb_presence_from_transport_presence(presence),
                        *from_channel_id,
                        *reason_id,
                        reason_message,
                        *invoker_id,
                        invoker_name,
                        invoker_uid,
                    )],
                )
                .expect("notifyclientmoved should encode");
                vec![
                    BlackTeaWebFrameBroadcast::Client {
                        client_id: presence.client_id,
                        frame: frame.clone(),
                    },
                    BlackTeaWebFrameBroadcast::Server {
                        server_id: presence.server_id,
                        exclude_client_id: Some(presence.client_id),
                        frame,
                    },
                ]
            }
            TransportNotification::ServerEdited {
                server_id,
                before,
                after,
                invoker_id,
                invoker_name,
            } => server_edited_row(before, after, *invoker_id, invoker_name)
                .map(|row| BlackTeaWebFrameBroadcast::Server {
                    server_id: *server_id,
                    exclude_client_id: origin_client_id,
                    frame: command_frame("notifyserveredited", vec![row])
                        .expect("notifyserveredited should encode"),
                })
                .into_iter()
                .collect(),
            TransportNotification::ClientLeftView {
                presence,
                to_channel_id,
                reason_id,
                reason_message,
                invoker_id,
                invoker_name,
                invoker_uid,
                ban_time,
            } => runtime
                .online_client_snapshot(presence.server_id, presence.client_id)
                .map(blackteaweb_presence_from_snapshot)
                .map(|mut bridged_presence| {
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
                    bridged_presence
                })
                .or_else(|| Some(blackteaweb_presence_from_transport_presence(presence)))
                .map(|bridged_presence| {
                    let frame = command_frame(
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
                    )
                    .expect("notifyclientleftview should encode");
                    vec![
                        BlackTeaWebFrameBroadcast::Client {
                            client_id: presence.client_id,
                            frame: frame.clone(),
                        },
                        BlackTeaWebFrameBroadcast::Server {
                            server_id: presence.server_id,
                            exclude_client_id: Some(presence.client_id),
                            frame,
                        },
                    ]
                })
                .unwrap_or_default(),
            TransportNotification::ChannelEdited {
                server_id,
                channel,
                description_changed,
                invoker_id,
                invoker_name,
                ..
            } => {
                let mut broadcasts = Vec::new();
                if *description_changed {
                    broadcasts.push(BlackTeaWebFrameBroadcast::Server {
                        server_id: *server_id,
                        exclude_client_id: origin_client_id,
                        frame: command_frame(
                            "notifychanneldescriptionchanged",
                            vec![row_map([("cid", channel.id.to_string())])],
                        )
                        .expect("notifychanneldescriptionchanged should encode"),
                    });
                }
                broadcasts.push(BlackTeaWebFrameBroadcast::Server {
                    server_id: *server_id,
                    exclude_client_id: origin_client_id,
                    frame: command_frame(
                        "notifychanneledited",
                        vec![row_map([
                            ("cid", channel.id.to_string()),
                            ("cpid", channel.parent_id.to_string()),
                            ("channel_name", channel.name.clone()),
                            ("channel_topic", channel.topic.clone()),
                            ("channel_description", channel.description.clone()),
                            (
                                "channel_flag_permanent",
                                if channel.kind.is_permanent() {
                                    String::from("1")
                                } else {
                                    String::from("0")
                                },
                            ),
                            (
                                "channel_flag_semi_permanent",
                                if channel.kind.is_semi_permanent() {
                                    String::from("1")
                                } else {
                                    String::from("0")
                                },
                            ),
                            ("invokerid", invoker_id.to_string()),
                            ("invokername", invoker_name.clone()),
                        ])],
                    )
                    .expect("notifychanneledited should encode"),
                });
                broadcasts
            }
            TransportNotification::ChannelCreated {
                server_id,
                channel,
                invoker_id,
                invoker_name,
            } => vec![BlackTeaWebFrameBroadcast::Server {
                server_id: *server_id,
                exclude_client_id: origin_client_id,
                frame: command_frame(
                    "notifychannelcreated",
                    vec![row_map([
                        ("cid", channel.id.to_string()),
                        ("cpid", channel.parent_id.to_string()),
                        ("channel_order", channel.order.to_string()),
                        ("channel_name", channel.name.clone()),
                        ("channel_topic", channel.topic.clone()),
                        ("channel_description", channel.description.clone()),
                        (
                            "channel_flag_permanent",
                            if channel.kind.is_permanent() {
                                String::from("1")
                            } else {
                                String::from("0")
                            },
                        ),
                        (
                            "channel_flag_semi_permanent",
                            if channel.kind.is_semi_permanent() {
                                String::from("1")
                            } else {
                                String::from("0")
                            },
                        ),
                        ("invokerid", invoker_id.to_string()),
                        ("invokername", invoker_name.clone()),
                    ])],
                )
                .expect("notifychannelcreated should encode"),
            }],
            TransportNotification::ChannelDeleted {
                server_id,
                channel,
                invoker_id,
                invoker_name,
            } => vec![BlackTeaWebFrameBroadcast::Server {
                server_id: *server_id,
                exclude_client_id: origin_client_id,
                frame: command_frame(
                    "notifychanneldeleted",
                    vec![row_map([
                        ("cid", channel.id.to_string()),
                        ("cpid", channel.parent_id.to_string()),
                        ("invokerid", invoker_id.to_string()),
                        ("invokername", invoker_name.clone()),
                    ])],
                )
                .expect("notifychanneldeleted should encode"),
            }],
            TransportNotification::ChannelMoved {
                server_id,
                channel,
                invoker_id,
                invoker_name,
                ..
            } => vec![BlackTeaWebFrameBroadcast::Server {
                server_id: *server_id,
                exclude_client_id: origin_client_id,
                frame: command_frame(
                    "notifychannelmoved",
                    vec![row_map([
                        ("cid", channel.id.to_string()),
                        ("cpid", channel.parent_id.to_string()),
                        ("order", channel.order.to_string()),
                        ("channel_name", channel.name.clone()),
                        ("invokerid", invoker_id.to_string()),
                        ("invokername", invoker_name.clone()),
                    ])],
                )
                .expect("notifychannelmoved should encode"),
            }],
            TransportNotification::TalkStatus {
                server_id,
                channel_id: _,
                client_id,
                is_talking,
                is_whisper: _,
                whisper_targets: _,
            } => vec![BlackTeaWebFrameBroadcast::Server {
                server_id: *server_id,
                exclude_client_id: origin_client_id,
                frame: command_frame(
                    "notifytalkstatus",
                    vec![row_map([
                        ("clid", client_id.to_string()),
                        ("status", if *is_talking { "1".to_string() } else { "0".to_string() }),
                    ])],
                )
                .expect("notifytalkstatus should encode"),
            }],
            TransportNotification::TextMessage {
                target,
                invoker_id,
                invoker_name,
                invoker_uid,
            } => match target.target_mode {
                1 => vec![BlackTeaWebFrameBroadcast::Client {
                    client_id: target.target_client_id.unwrap_or_default(),
                    frame: command_frame(
                        "notifytextmessage",
                        vec![text_message_row(
                            target,
                            *invoker_id,
                            invoker_name,
                            invoker_uid,
                            bridge_timestamp(),
                        )],
                    )
                    .expect("notifytextmessage should encode"),
                }],
                2 | 3 => vec![BlackTeaWebFrameBroadcast::Server {
                    server_id: target.server_id,
                    exclude_client_id: origin_client_id,
                    frame: command_frame(
                        "notifytextmessage",
                        vec![text_message_row(
                            target,
                            *invoker_id,
                            invoker_name,
                            invoker_uid,
                            bridge_timestamp(),
                        )],
                    )
                    .expect("notifytextmessage should encode"),
                }],
                _ => Vec::new(),
            },
        };
        frames.append(&mut notification_frames);
    }
    frames
}

pub(crate) fn derive_direct_frame(
    before_presence: &Option<BlackTeaWebPresence>,
    after_presence: &Option<BlackTeaWebPresence>,
) -> Result<Option<String>> {
    match (before_presence, after_presence) {
        (Some(before), Some(after))
            if before.server_id == after.server_id && before.channel_id != after.channel_id =>
        {
            Ok(Some(command_frame(
                "notifyclientmoved",
                vec![presence_move_row(
                    after,
                    before.channel_id,
                    0,
                    "changed channel",
                )],
            )?))
        }
        _ => Ok(None),
    }
}

pub(crate) fn derive_query_notifications_from_presence(
    before_presence: &Option<BlackTeaWebPresence>,
    after_presence: &Option<BlackTeaWebPresence>,
) -> Vec<TransportNotification> {
    let before_presence = before_presence
        .as_ref()
        .map(session_presence_from_blackteaweb_presence);
    let after_presence = after_presence
        .as_ref()
        .map(session_presence_from_blackteaweb_presence);

    match (before_presence, after_presence) {
        (Some(before), Some(after))
            if before.client_id == after.client_id
                && before.server_id == after.server_id
                && before.channel_id == after.channel_id =>
        {
            Vec::new()
        }
        (Some(before), Some(after)) => vec![
            TransportNotification::ClientLeftView {
                presence: before.clone(),
                to_channel_id: Some(after.channel_id),
                reason_id: 0,
                reason_message: String::from("changed channel"),
                invoker_id: before.client_id,
                invoker_name: before.login_name.clone(),
                invoker_uid: before.unique_identifier.clone(),
                ban_time: None,
            },
            TransportNotification::ClientEnterView {
                presence: after,
                from_channel_id: Some(before.channel_id),
                reason_id: 0,
            },
        ],
        (Some(before), None) => vec![TransportNotification::ClientLeftView {
            presence: before,
            to_channel_id: None,
            reason_id: 8,
            reason_message: String::from("left server"),
            invoker_id: 0,
            invoker_name: String::new(),
            invoker_uid: String::new(),
            ban_time: None,
        }],
        (None, Some(after)) => vec![TransportNotification::ClientEnterView {
            presence: after,
            from_channel_id: None,
            reason_id: 0,
        }],
        (None, None) => Vec::new(),
    }
}

pub(crate) fn derive_peer_frames(
    before_presence: &Option<BlackTeaWebPresence>,
    after_presence: &Option<BlackTeaWebPresence>,
) -> Result<Vec<PresenceBroadcast>> {
    match (before_presence, after_presence) {
        (None, Some(after)) => Ok(vec![PresenceBroadcast::PeerEnter {
            server_id: after.server_id,
            exclude_client_id: Some(after.client_id),
            presence: after.clone(),
            from_channel_id: None,
            reason_id: 0,
        }]),
        (Some(before), Some(after))
            if before.server_id == after.server_id && before.channel_id != after.channel_id =>
        {
            Ok(vec![PresenceBroadcast::PeerMove {
                server_id: after.server_id,
                exclude_client_id: Some(after.client_id),
                presence: after.clone(),
                from_channel_id: before.channel_id,
                reason_id: 0,
                reason_message: String::from("changed channel"),
            }])
        }
        (Some(before), Some(after)) if before.server_id == after.server_id => {
            if presence_update_row(before, after).is_none() {
                return Ok(Vec::new());
            }

            Ok(vec![PresenceBroadcast::PeerUpdate {
                server_id: after.server_id,
                exclude_client_id: Some(after.client_id),
                before: before.clone(),
                after: after.clone(),
            }])
        }
        _ => Ok(Vec::new()),
    }
}

pub(crate) fn broadcast_frames_for_presence_change(
    sessions: &SharedBlackTeaWebSessions,
    broadcasts: &[PresenceBroadcast],
) -> Result<()> {
    let mut recipients = Vec::new();
    {
        let sessions = sessions
            .lock()
            .map_err(|_| io::Error::other("BlackTeaWeb session registry lock poisoned"))?;
        for broadcast in broadcasts {
            match broadcast {
                PresenceBroadcast::PeerEnter {
                    server_id,
                    exclude_client_id,
                    presence,
                    from_channel_id,
                    reason_id,
                } => {
                    recipients.extend(
                        sessions
                            .values()
                            .filter(|session| {
                                session.presence.server_id == *server_id
                                    && *exclude_client_id != Some(session.presence.client_id)
                                    && session.visible_channel_ids.contains(&presence.channel_id)
                            })
                            .map(|session| {
                                (
                                    Arc::clone(&session.pending_frames),
                                    command_frame(
                                        "notifycliententerview",
                                        vec![presence_enter_view_row(
                                            presence,
                                            *from_channel_id,
                                            *reason_id,
                                        )],
                                    )
                                    .expect("notifycliententerview should encode"),
                                )
                            }),
                    );
                }
                PresenceBroadcast::PeerMove {
                    server_id,
                    exclude_client_id,
                    presence,
                    from_channel_id,
                    reason_id,
                    reason_message,
                } => {
                    recipients.extend(sessions.values().filter_map(|session| {
                        if session.presence.server_id != *server_id
                            || *exclude_client_id == Some(session.presence.client_id)
                        {
                            return None;
                        }

                        let source_visible = session.visible_channel_ids.contains(from_channel_id);
                        let target_visible = session.visible_channel_ids.contains(&presence.channel_id);
                        let frame = if source_visible && target_visible {
                            command_frame(
                                "notifyclientmoved",
                                vec![presence_move_row(
                                    presence,
                                    *from_channel_id,
                                    *reason_id,
                                    reason_message,
                                )],
                            )
                            .expect("notifyclientmoved should encode")
                        } else if source_visible {
                            command_frame(
                                "notifyclientleftview",
                                vec![presence_left_view_row(
                                    &presence_with_channel_id(presence, *from_channel_id),
                                    Some(presence.channel_id),
                                    *reason_id,
                                    reason_message,
                                )],
                            )
                            .expect("notifyclientleftview should encode")
                        } else if target_visible {
                            command_frame(
                                "notifycliententerview",
                                vec![presence_enter_view_row(
                                    presence,
                                    Some(*from_channel_id),
                                    *reason_id,
                                )],
                            )
                            .expect("notifycliententerview should encode")
                        } else {
                            return None;
                        };

                        Some((Arc::clone(&session.pending_frames), frame))
                    }));
                }
                PresenceBroadcast::PeerUpdate {
                    server_id,
                    exclude_client_id,
                    before,
                    after,
                } => {
                    recipients.extend(sessions.values().filter_map(|session| {
                        if session.presence.server_id != *server_id
                            || *exclude_client_id == Some(session.presence.client_id)
                            || !session.visible_channel_ids.contains(&after.channel_id)
                        {
                            return None;
                        }

                        let row = presence_update_row(before, after)?;
                        Some((
                            Arc::clone(&session.pending_frames),
                            command_frame("notifyclientupdated", vec![row])
                                .expect("notifyclientupdated should encode"),
                        ))
                    }));
                }
                PresenceBroadcast::PeerLeft {
                    server_id,
                    exclude_client_id,
                    presence,
                    to_channel_id,
                    reason_id,
                    reason_message,
                } => {
                    recipients.extend(sessions.values().filter_map(|session| {
                        if session.presence.server_id != *server_id
                            || *exclude_client_id == Some(session.presence.client_id)
                            || !session.visible_channel_ids.contains(&presence.channel_id)
                        {
                            return None;
                        }

                        Some((
                            Arc::clone(&session.pending_frames),
                            command_frame(
                                "notifyclientleftview",
                                vec![presence_left_view_row(
                                    presence,
                                    *to_channel_id,
                                    *reason_id,
                                    reason_message,
                                )],
                            )
                            .expect("notifyclientleftview should encode"),
                        ))
                    }));
                }
            }
        }
    }

    for (pending_frames, frame) in recipients {
        pending_frames
            .lock()
            .map_err(|_| io::Error::other("BlackTeaWeb session pending-queue lock poisoned"))?
            .push(frame);
    }
    Ok(())
}

pub(crate) fn broadcast_queued_frames(
    sessions: &SharedBlackTeaWebSessions,
    broadcasts: &[BlackTeaWebFrameBroadcast],
) -> Result<()> {
    let mut recipients = Vec::new();
    {
        let sessions = sessions
            .lock()
            .map_err(|_| io::Error::other("BlackTeaWeb session registry lock poisoned"))?;
        for broadcast in broadcasts {
            match broadcast {
                BlackTeaWebFrameBroadcast::Server {
                    server_id,
                    exclude_client_id,
                    frame,
                } => {
                    recipients.extend(
                        sessions
                            .values()
                            .filter(|session| {
                                session.presence.server_id == *server_id
                                    && *exclude_client_id != Some(session.presence.client_id)
                            })
                            .map(|session| (Arc::clone(&session.pending_frames), frame.clone())),
                    );
                }
                BlackTeaWebFrameBroadcast::Client { client_id, frame } => {
                    if let Some(session) = sessions.get(client_id) {
                        recipients.push((Arc::clone(&session.pending_frames), frame.clone()));
                    }
                }
            }
        }
    }

    for (pending_frames, frame) in recipients {
        pending_frames
            .lock()
            .map_err(|_| io::Error::other("BlackTeaWeb session pending-queue lock poisoned"))?
            .push(frame);
    }
    Ok(())
}

pub(crate) fn install_file_transfer_notifier(
    file_transfers: &Arc<FileTransferRegistry>,
    sessions: &SharedBlackTeaWebSessions,
) {
    let sessions = Arc::clone(sessions);
    file_transfers.add_notifier(Arc::new(move |event| {
        let broadcast = match file_transfer_event_broadcast(event.clone()) {
            Ok(broadcast) => broadcast,
            Err(error) => {
                eprintln!("BlackTeaWeb file transfer notify encode error: {error:#}");
                return;
            }
        };

        if let Err(error) = broadcast_queued_frames(&sessions, &[broadcast]) {
            eprintln!("BlackTeaWeb file transfer notify broadcast error: {error:#}");
        }
    }));
}

pub(crate) fn file_transfer_event_broadcast(event: FileTransferEvent) -> Result<BlackTeaWebFrameBroadcast> {
    match event {
        FileTransferEvent::Started {
            client_id,
            client_transfer_id,
        } => Ok(BlackTeaWebFrameBroadcast::Client {
            client_id,
            frame: command_frame(
                "notifyfiletransferstarted",
                vec![transfer_started_row(&client_transfer_id)],
            )?,
        }),
        FileTransferEvent::Progress {
            client_id,
            client_transfer_id,
            file_bytes_transferred,
            file_current_offset,
            file_start_offset,
            file_total_size,
            network_bytes_received,
            network_bytes_send,
            network_current_speed,
            network_average_speed,
        } => Ok(BlackTeaWebFrameBroadcast::Client {
            client_id,
            frame: command_frame(
                "notifyfiletransferprogress",
                vec![transfer_progress_row(
                    &client_transfer_id,
                    file_bytes_transferred,
                    file_current_offset,
                    file_start_offset,
                    file_total_size,
                    network_bytes_received,
                    network_bytes_send,
                    network_current_speed,
                    network_average_speed,
                )],
            )?,
        }),
        FileTransferEvent::Status {
            client_id,
            client_transfer_id,
            status,
            message,
        } => Ok(BlackTeaWebFrameBroadcast::Client {
            client_id,
            frame: command_frame(
                "notifystatusfiletransfer",
                vec![transfer_status_row(&client_transfer_id, status, &message)],
            )?,
        }),
    }
}

pub(crate) fn broadcast_permission_refreshes(
    sessions: &SharedBlackTeaWebSessions,
    runtime: &BaselineRuntime,
    refreshes: &[BlackTeaWebPermissionRefresh],
) -> Result<()> {
    let mut scopes_by_server = BTreeMap::<u32, PermissionRefreshScope>::new();
    for refresh in refreshes {
        scopes_by_server
            .entry(refresh.server_id)
            .or_default()
            .merge(refresh.scope);
    }

    let mut recipients = Vec::new();
    {
        let mut sessions = sessions
            .lock()
            .map_err(|_| io::Error::other("BlackTeaWeb session registry lock poisoned"))?;
        for (server_id, scope) in scopes_by_server {
            let server_groups_frame = if scope.server_groups {
                Some(command_frame(
                    "notifyservergrouplist",
                    runtime.web_server_group_rows(),
                )?)
            } else {
                None
            };
            let channel_groups_frame = if scope.channel_groups {
                Some(command_frame(
                    "notifychannelgrouplist",
                    runtime.web_channel_group_rows(),
                )?)
            } else {
                None
            };

            for session in sessions.values_mut().filter(|session| session.presence.server_id == server_id)
            {
                if let Some(frame) = server_groups_frame.as_ref() {
                    recipients.push((Arc::clone(&session.pending_frames), frame.clone()));
                }
                if let Some(frame) = channel_groups_frame.as_ref() {
                    recipients.push((Arc::clone(&session.pending_frames), frame.clone()));
                }
                if scope.needed_permissions
                    && let Some(frame) =
                        connected_needed_permission_frame_for_presence(runtime, &session.presence)?
                {
                    recipients.push((Arc::clone(&session.pending_frames), frame));
                }

                let after_visible_channel_ids = runtime.web_visible_channel_ids_for_client(
                    session.presence.server_id,
                    session.client_database_id,
                    Some(session.presence.channel_id),
                );
                let visibility_frames = visibility_transition_frames(
                    runtime,
                    server_id,
                    session.presence.client_id,
                    &session.visible_channel_ids,
                    &after_visible_channel_ids,
                    &BTreeSet::new(),
                    &BTreeSet::new(),
                )?;
                session.visible_channel_ids = after_visible_channel_ids;
                recipients.extend(
                    visibility_frames
                        .into_iter()
                        .map(|frame| (Arc::clone(&session.pending_frames), frame)),
                );
            }
        }
    }

    for (pending_frames, frame) in recipients {
        pending_frames
            .lock()
            .map_err(|_| io::Error::other("BlackTeaWeb session pending-queue lock poisoned"))?
            .push(frame);
    }
    Ok(())
}

