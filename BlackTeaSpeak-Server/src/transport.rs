use std::collections::HashMap;
use std::io::{self, BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result};

use crate::query::{
    CommandRequest, QueryResponse, encode_query_value, parse_request_line, render_response,
};
use crate::runtime::{
    BaselineRuntime, ChannelSnapshot, NotificationEventKind, OnlineClientSnapshot,
    QuerySessionState, ServerSnapshot, TextMessageTarget, create_baseline_runtime,
    create_baseline_runtime_with_state_path, stable_query_client_unique_identifier,
};

pub const DEFAULT_QUERY_BIND: &str = "127.0.0.1:10101";



#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionPresence {
    pub client_id: u64,
    pub login_name: String,
    pub unique_identifier: String,
    pub client_type: u32,
    pub server_id: u32,
    pub channel_id: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransportNotification {
    ClientEnterView {
        presence: SessionPresence,
        from_channel_id: Option<u32>,
        reason_id: u32,
    },
    ClientUpdated {
        server_id: u32,
        before: OnlineClientSnapshot,
        after: OnlineClientSnapshot,
    },
    ClientPoke {
        server_id: u32,
        target_client_id: u64,
        invoker_id: u64,
        invoker_name: String,
        invoker_uid: String,
        message: String,
    },
    ClientMoved {
        presence: SessionPresence,
        from_channel_id: u32,
        reason_id: u32,
        reason_message: String,
        invoker_id: u64,
        invoker_name: String,
        invoker_uid: String,
    },
    ClientLeftView {
        presence: SessionPresence,
        to_channel_id: Option<u32>,
        reason_id: u32,
        reason_message: String,
        invoker_id: u64,
        invoker_name: String,
        invoker_uid: String,
        ban_time: Option<u32>,
    },
    ChannelEdited {
        server_id: u32,
        channel: ChannelSnapshot,
        description_changed: bool,
        invoker_id: u64,
        invoker_name: String,
    },
    ChannelCreated {
        server_id: u32,
        channel: ChannelSnapshot,
        invoker_id: u64,
        invoker_name: String,
    },
    ChannelDeleted {
        server_id: u32,
        channel: ChannelSnapshot,
        invoker_id: u64,
        invoker_name: String,
    },
    ChannelMoved {
        server_id: u32,
        previous_parent_id: u32,
        channel: ChannelSnapshot,
        invoker_id: u64,
        invoker_name: String,
    },
    ServerEdited {
        server_id: u32,
        before: ServerSnapshot,
        after: ServerSnapshot,
        invoker_id: u64,
        invoker_name: String,
    },
    TalkStatus {
        server_id: u32,
        channel_id: u32,
        client_id: u64,
        is_talking: bool,
    },
    TextMessage {
        target: TextMessageTarget,
        invoker_id: u64,
        invoker_name: String,
        invoker_uid: String,
    },
}




pub fn execute_request_with_notifications(
    runtime: &mut BaselineRuntime,
    request: &CommandRequest,
    before_session: &QuerySessionState,
    session: &mut QuerySessionState,
) -> (QueryResponse, Vec<TransportNotification>) {
    let before_channel_snapshot =
        channel_snapshot_before_request(runtime, request, before_session);
    let before_server_snapshot =
        server_snapshot_before_request(runtime, request, before_session);
    let before_client_snapshot =
        client_snapshot_before_request(runtime, request, before_session);
    let response = runtime.execute_request(request.clone(), session);
    let after_channel_snapshot =
        channel_snapshot_after_request(runtime, request, session, &response);
    let after_server_snapshot =
        server_snapshot_after_request(runtime, request, session, &response);
    let after_client_snapshot =
        client_snapshot_after_request(runtime, request, session, &response);
    let cleanup_channel_ids = cleanup_candidate_channel_ids(
        request,
        before_client_snapshot.as_ref(),
    );
    let mut runtime_notifications = derive_runtime_notifications(
        runtime,
        request,
        before_session,
        session,
        &response,
        before_channel_snapshot,
        after_channel_snapshot,
        before_server_snapshot,
        after_server_snapshot,
        before_client_snapshot,
        after_client_snapshot,
    );

    if let Some(server_id) = session.selected_virtual_server_id {
        for notif in &runtime_notifications {
            runtime.broadcast_event(server_id, notif);
        }
    }

    if response.error_id == 0
        && let Some(server_id) = session
            .selected_virtual_server_id
            .or(before_session.selected_virtual_server_id)
    {
        let cleanup_notifications = cleanup_notifications_from_runtime(
            server_id,
            runtime.cleanup_temporary_channels(server_id, &cleanup_channel_ids),
            session.client_id,
            session
                .authenticated_login
                .clone()
                .unwrap_or_else(|| String::from("anonymous")),
            runtime.query_session_unique_identifier(session),
        );
        runtime_notifications.extend(cleanup_notifications);
    }
    (response, runtime_notifications)
}
















fn derive_runtime_notifications(
    runtime: &BaselineRuntime,
    request: &CommandRequest,
    before_session: &QuerySessionState,
    after_session: &QuerySessionState,
    response: &QueryResponse,
    before_channel_snapshot: Option<ChannelSnapshot>,
    after_channel_snapshot: Option<ChannelSnapshot>,
    before_server_snapshot: Option<ServerSnapshot>,
    after_server_snapshot: Option<ServerSnapshot>,
    before_client_snapshot: Option<OnlineClientSnapshot>,
    after_client_snapshot: Option<OnlineClientSnapshot>,
) -> Vec<TransportNotification> {
    if !response_is_ok(response) {
        return Vec::new();
    }

    let invoker_id = after_session.client_id;
    let invoker_name = after_session
        .authenticated_login
        .clone()
        .unwrap_or_else(|| String::from("anonymous"));
    let invoker_uid = runtime.query_session_unique_identifier(after_session);
    let before_channel_snapshot_ref = before_channel_snapshot.as_ref();
    let after_channel_snapshot_ref = after_channel_snapshot.as_ref();
    let before_server_snapshot_ref = before_server_snapshot.as_ref();
    let after_server_snapshot_ref = after_server_snapshot.as_ref();
    let before_client_snapshot_ref = before_client_snapshot.as_ref();
    let after_client_snapshot_ref = after_client_snapshot.as_ref();

    match request.command.as_str() {
        "channelcreate" => {
            if let (Some(server_id), Some(channel)) = (
                after_session.selected_virtual_server_id,
                after_channel_snapshot_ref,
            ) {
                return vec![TransportNotification::ChannelCreated {
                    server_id,
                    channel: channel.clone(),
                    invoker_id,
                    invoker_name,
                }];
            }
        }
        "channeldelete" => {
            if let (Some(server_id), Some(channel)) = (
                before_session.selected_virtual_server_id,
                before_channel_snapshot_ref,
            ) {
                return vec![TransportNotification::ChannelDeleted {
                    server_id,
                    channel: channel.clone(),
                    invoker_id,
                    invoker_name,
                }];
            }
        }
        "channeledit"
            if before_channel_snapshot_ref != after_channel_snapshot_ref
                && let (Some(server_id), Some(channel)) = (
                    after_session.selected_virtual_server_id,
                    after_channel_snapshot_ref,
                ) =>
        {
            return vec![TransportNotification::ChannelEdited {
                server_id,
                channel: channel.clone(),
                description_changed: before_channel_snapshot_ref
                    .map(|before| before.description != channel.description)
                    .unwrap_or(false),
                invoker_id,
                invoker_name,
            }];
        }
        "channelmove"
            if before_channel_snapshot_ref != after_channel_snapshot_ref
                && let (Some(server_id), Some(before_channel), Some(channel)) = (
                    after_session.selected_virtual_server_id,
                    before_channel_snapshot_ref,
                    after_channel_snapshot_ref,
                ) =>
        {
            return vec![TransportNotification::ChannelMoved {
                server_id,
                previous_parent_id: before_channel.parent_id,
                channel: channel.clone(),
                invoker_id,
                invoker_name,
            }];
        }
        "serveredit"
            if before_server_snapshot_ref != after_server_snapshot_ref
                && let (Some(server_id), Some(before), Some(after)) = (
                    after_session.selected_virtual_server_id,
                    before_server_snapshot_ref,
                    after_server_snapshot_ref,
                )
                && server_edited_fields(before, after, invoker_id, &invoker_name).len() > 4 =>
        {
            return vec![TransportNotification::ServerEdited {
                server_id,
                before: before.clone(),
                after: after.clone(),
                invoker_id,
                invoker_name,
            }];
        }
        "sendtextmessage"
            if let Ok(target) = runtime.text_message_target(request, after_session) =>
        {
            return vec![TransportNotification::TextMessage {
                target,
                invoker_id,
                invoker_name,
                invoker_uid: runtime.query_session_unique_identifier(after_session),
            }];
        }
        "clientpoke"
            if let (Some(server_id), Some(target_client_id)) = (
                after_session.selected_virtual_server_id,
                request
                    .named_args
                    .get("clid")
                    .and_then(|value| value.parse::<u64>().ok()),
            ) =>
        {
            return vec![TransportNotification::ClientPoke {
                server_id,
                target_client_id,
                invoker_id,
                invoker_name,
                invoker_uid,
                message: request.named_args.get("msg").cloned().unwrap_or_default(),
            }];
        }
        "clientmove"
            if before_client_snapshot_ref != after_client_snapshot_ref
                && let (Some(before), Some(after)) =
                    (before_client_snapshot_ref, after_client_snapshot_ref)
                && before.channel_id != after.channel_id =>
        {
            let reason_id = if invoker_id == after.id { 0 } else { 1 };
            return vec![TransportNotification::ClientMoved {
                presence: session_presence_from_snapshot(after),
                from_channel_id: before.channel_id,
                reason_id,
                reason_message: request.named_args.get("reasonmsg").cloned().unwrap_or_default(),
                invoker_id,
                invoker_name,
                invoker_uid,
            }];
        }
        "clientkick"
            if let Some(reason_id) = request
                .named_args
                .get("reasonid")
                .and_then(|value| value.parse::<u32>().ok()) =>
        {
            let reason_message = request.named_args.get("reasonmsg").cloned().unwrap_or_default();
            return match reason_id {
                4 if let (Some(before), Some(after)) =
                    (before_client_snapshot_ref, after_client_snapshot_ref)
                    && before.channel_id != after.channel_id =>
                {
                    vec![TransportNotification::ClientMoved {
                        presence: session_presence_from_snapshot(after),
                        from_channel_id: before.channel_id,
                        reason_id,
                        reason_message,
                        invoker_id,
                        invoker_name,
                        invoker_uid,
                    }]
                }
                5 if let Some(before) = before_client_snapshot_ref => {
                    vec![TransportNotification::ClientLeftView {
                        presence: session_presence_from_snapshot(before),
                        to_channel_id: None,
                        reason_id,
                        reason_message,
                        invoker_id,
                        invoker_name,
                        invoker_uid,
                        ban_time: None,
                    }]
                }
                _ => Vec::new(),
            };
        }
        "banclient" if let Some(before) = before_client_snapshot_ref => {
            return vec![TransportNotification::ClientLeftView {
                presence: session_presence_from_snapshot(before),
                to_channel_id: None,
                reason_id: 6,
                reason_message: request.named_args.get("banreason").cloned().unwrap_or_default(),
                invoker_id,
                invoker_name,
                invoker_uid,
                ban_time: request
                    .named_args
                    .get("time")
                    .and_then(|value| value.parse::<u32>().ok()),
            }];
        }
        "clientupdate"
            if before_client_snapshot_ref != after_client_snapshot_ref
                && let (Some(server_id), Some(before), Some(after)) = (
                    after_session.selected_virtual_server_id,
                    before_client_snapshot_ref,
                    after_client_snapshot_ref,
                )
                && client_update_fields(before, after).len() > 1 =>
        {
            return vec![TransportNotification::ClientUpdated {
                server_id,
                before: before.clone(),
                after: after.clone(),
            }];
        }
        "musicbotcreate" if let Some(after) = after_client_snapshot_ref => {
            return vec![TransportNotification::ClientEnterView {
                presence: session_presence_from_snapshot(after),
                from_channel_id: None,
                reason_id: 0,
            }];
        }
        "musicbotdelete" if let Some(before) = before_client_snapshot_ref => {
            return vec![TransportNotification::ClientLeftView {
                presence: session_presence_from_snapshot(before),
                to_channel_id: None,
                reason_id: 5,
                reason_message: String::from("music bot deleted"),
                invoker_id,
                invoker_name,
                invoker_uid,
                ban_time: None,
            }];
        }
        _ => {}
    }

    Vec::new()
}

fn response_is_ok(response: &QueryResponse) -> bool {
    response.error_id == 0
}

fn channel_snapshot_before_request(
    runtime: &BaselineRuntime,
    request: &CommandRequest,
    session: &QuerySessionState,
) -> Option<ChannelSnapshot> {
    let server_id = session.selected_virtual_server_id?;
    match request.command.as_str() {
        "channeledit" | "channeldelete" | "channelmove" => {
            let channel_id = channel_id_from_request(request)?;
            runtime.snapshot_channel(server_id, channel_id)
        }
        _ => None,
    }
}

fn channel_snapshot_after_request(
    runtime: &BaselineRuntime,
    request: &CommandRequest,
    session: &QuerySessionState,
    response: &QueryResponse,
) -> Option<ChannelSnapshot> {
    let server_id = session.selected_virtual_server_id?;
    match request.command.as_str() {
        "channeledit" | "channelmove" => {
            let channel_id = channel_id_from_request(request)?;
            runtime.snapshot_channel(server_id, channel_id)
        }
        "channelcreate" => response
            .rows
            .first()
            .and_then(|row| row.get("cid"))
            .and_then(|value| value.parse::<u32>().ok())
            .and_then(|channel_id| runtime.snapshot_channel(server_id, channel_id)),
        _ => None,
    }
}

fn server_snapshot_before_request(
    runtime: &BaselineRuntime,
    request: &CommandRequest,
    session: &QuerySessionState,
) -> Option<ServerSnapshot> {
    if request.command != "serveredit" {
        return None;
    }

    let server_id = session.selected_virtual_server_id?;
    runtime.snapshot_server(server_id)
}

fn server_snapshot_after_request(
    runtime: &BaselineRuntime,
    request: &CommandRequest,
    session: &QuerySessionState,
    response: &QueryResponse,
) -> Option<ServerSnapshot> {
    if request.command != "serveredit" || !response_is_ok(response) {
        return None;
    }

    let server_id = session.selected_virtual_server_id?;
    runtime.snapshot_server(server_id)
}

fn client_snapshot_before_request(
    runtime: &BaselineRuntime,
    request: &CommandRequest,
    session: &QuerySessionState,
) -> Option<OnlineClientSnapshot> {
    let server_id = session.selected_virtual_server_id?;
    match request.command.as_str() {
        "clientupdate" => {
            if session.client_id == 0 {
                return None;
            }
            runtime.online_client_snapshot(server_id, session.client_id)
        }
        "clientmove" => request
            .named_args
            .get("clid")
            .and_then(|value| value.parse::<u64>().ok())
            .or(Some(session.client_id).filter(|client_id| *client_id != 0))
            .and_then(|client_id| runtime.online_client_snapshot(server_id, client_id)),
        "clientkick" | "banclient" => request
            .named_args
            .get("clid")
            .and_then(|value| value.parse::<u64>().ok())
            .and_then(|client_id| runtime.online_client_snapshot(server_id, client_id)),
        "musicbotdelete" => request
            .named_args
            .get("botid")
            .or_else(|| request.named_args.get("bot_id"))
            .and_then(|value| value.parse::<u64>().ok())
            .and_then(|bot_identifier| {
                runtime.music_bot_client_snapshot_by_identifier(server_id, bot_identifier)
            }),
        _ => None,
    }
}

fn client_snapshot_after_request(
    runtime: &BaselineRuntime,
    request: &CommandRequest,
    session: &QuerySessionState,
    response: &QueryResponse,
) -> Option<OnlineClientSnapshot> {
    if !response_is_ok(response) {
        return None;
    }

    let server_id = session.selected_virtual_server_id?;
    match request.command.as_str() {
        "clientupdate" => {
            if session.client_id == 0 {
                return None;
            }
            runtime.online_client_snapshot(server_id, session.client_id)
        }
        "clientmove" => request
            .named_args
            .get("clid")
            .and_then(|value| value.parse::<u64>().ok())
            .or(Some(session.client_id).filter(|client_id| *client_id != 0))
            .and_then(|client_id| runtime.online_client_snapshot(server_id, client_id)),
        "clientkick" | "banclient" => request
            .named_args
            .get("clid")
            .and_then(|value| value.parse::<u64>().ok())
            .and_then(|client_id| runtime.online_client_snapshot(server_id, client_id)),
        "musicbotcreate" => response
            .rows
            .first()
            .and_then(|row| row.get("clid"))
            .and_then(|value| value.parse::<u64>().ok())
            .and_then(|client_id| runtime.online_client_snapshot(server_id, client_id)),
        _ => None,
    }
}

fn channel_id_from_request(request: &CommandRequest) -> Option<u32> {
    request.named_args.get("cid")?.parse::<u32>().ok()
}

fn same_view_location(left: &SessionPresence, right: &SessionPresence) -> bool {
    left.client_id == right.client_id
        && left.server_id == right.server_id
        && left.channel_id == right.channel_id
}

fn session_presence(session: &QuerySessionState) -> Option<SessionPresence> {
    let server_id = session.selected_virtual_server_id?;
    let (unique_identifier, client_type) = if session.is_desktop_client {
        (format!("desktop-{}", session.client_id), 0)
    } else {
        let authenticated_login = session.authenticated_login.as_ref()?;
        (stable_query_client_unique_identifier(authenticated_login), 1)
    };
    
    Some(SessionPresence {
        client_id: session.client_id,
        unique_identifier,
        client_type,
        login_name: session.effective_nickname(),
        server_id,
        channel_id: session.current_channel_id.unwrap_or(1),
    })
}

fn session_presence_from_snapshot(snapshot: &OnlineClientSnapshot) -> SessionPresence {
    SessionPresence {
        client_id: snapshot.id,
        login_name: snapshot.nickname.clone(),
        unique_identifier: snapshot.unique_identifier.clone(),
        client_type: snapshot.client_type_exact,
        server_id: snapshot.server_id,
        channel_id: snapshot.channel_id,
    }
}

fn cleanup_candidate_channel_ids(
    request: &CommandRequest,
    before_client_snapshot: Option<&OnlineClientSnapshot>,
) -> Vec<u32> {
    match request.command.as_str() {
        "clientmove" | "clientkick" | "banclient" | "musicbotdelete" => before_client_snapshot
            .map(|snapshot| vec![snapshot.channel_id])
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

fn cleanup_notifications_from_runtime(
    server_id: u32,
    cleanups: Vec<crate::runtime::TemporaryChannelCleanup>,
    invoker_id: u64,
    invoker_name: String,
    invoker_uid: String,
) -> Vec<TransportNotification> {
    let mut notifications = Vec::new();

    for cleanup in cleanups {
        if let Some(client) = cleanup.removed_client {
            notifications.push(TransportNotification::ClientLeftView {
                presence: session_presence_from_snapshot(&client),
                to_channel_id: None,
                reason_id: 5,
                reason_message: String::from("temporary channel cleanup"),
                invoker_id,
                invoker_name: invoker_name.clone(),
                invoker_uid: invoker_uid.clone(),
                ban_time: None,
            });
        }
        if let Some(channel) = cleanup.removed_channel {
            notifications.push(TransportNotification::ChannelDeleted {
                server_id,
                channel,
                invoker_id,
                invoker_name: invoker_name.clone(),
            });
        }
    }

    notifications
}




pub(crate) fn wants_notification(session: &QuerySessionState, notification: &TransportNotification) -> bool {
    match notification {
        TransportNotification::ClientEnterView { presence, .. }
        | TransportNotification::ClientLeftView { presence, .. } => session
            .notification_subscriptions
            .iter()
            .any(|subscription| {
                session.selected_virtual_server_id == Some(presence.server_id)
                    && match subscription.event {
                        NotificationEventKind::Server => true,
                        NotificationEventKind::Channel => subscription
                            .channel_id
                            .is_some_and(|channel_id| channel_id == presence.channel_id),
                        NotificationEventKind::TextServer
                        | NotificationEventKind::TextChannel
                        | NotificationEventKind::TextPrivate => false,
                    }
            }),
        TransportNotification::ClientMoved {
            presence,
            from_channel_id,
            ..
        } => session.notification_subscriptions.iter().any(|subscription| {
            session.selected_virtual_server_id == Some(presence.server_id)
                && match subscription.event {
                    NotificationEventKind::Server => true,
                    NotificationEventKind::Channel => subscription.channel_id.is_some_and(
                        |channel_id| {
                            channel_id == presence.channel_id || channel_id == *from_channel_id
                        },
                    ),
                    NotificationEventKind::TextServer
                    | NotificationEventKind::TextChannel
                    | NotificationEventKind::TextPrivate => false,
                }
        }),
        TransportNotification::ClientUpdated {
            server_id, after, ..
        } => session
            .notification_subscriptions
            .iter()
            .any(|subscription| {
                session.selected_virtual_server_id == Some(*server_id)
                    && match subscription.event {
                        NotificationEventKind::Server => true,
                        NotificationEventKind::Channel => subscription
                            .channel_id
                            .is_some_and(|channel_id| channel_id == after.channel_id),
                        NotificationEventKind::TextServer
                        | NotificationEventKind::TextChannel
                        | NotificationEventKind::TextPrivate => false,
                    }
            }),
        TransportNotification::TalkStatus {
            server_id,
            channel_id,
            ..
        } => {
            session.selected_virtual_server_id == Some(*server_id) && session.current_channel_id == Some(*channel_id)
        }
        TransportNotification::ClientPoke {
            server_id,
            target_client_id,
            ..
        } => session.selected_virtual_server_id == Some(*server_id)
            && session.client_id == *target_client_id,
        TransportNotification::ServerEdited { server_id, .. } => session
            .notification_subscriptions
            .iter()
            .any(|subscription| {
                session.selected_virtual_server_id == Some(*server_id)
                    && matches!(subscription.event, NotificationEventKind::Server)
            }),
        TransportNotification::ChannelEdited {
            server_id, channel, ..
        } => session
            .notification_subscriptions
            .iter()
            .any(|subscription| {
                session.selected_virtual_server_id == Some(*server_id)
                    && match subscription.event {
                        NotificationEventKind::Server => true,
                        NotificationEventKind::Channel => subscription
                            .channel_id
                            .is_some_and(|channel_id| channel_id == channel.id),
                        NotificationEventKind::TextServer
                        | NotificationEventKind::TextChannel
                        | NotificationEventKind::TextPrivate => false,
                    }
            }),
        TransportNotification::ChannelCreated {
            server_id, channel, ..
        }
        | TransportNotification::ChannelDeleted {
            server_id, channel, ..
        } => session
            .notification_subscriptions
            .iter()
            .any(|subscription| {
                session.selected_virtual_server_id == Some(*server_id)
                    && match subscription.event {
                        NotificationEventKind::Server => true,
                        NotificationEventKind::Channel => matches_channel_tree_subscription(
                            subscription.channel_id,
                            channel,
                            None,
                        ),
                        NotificationEventKind::TextServer
                        | NotificationEventKind::TextChannel
                        | NotificationEventKind::TextPrivate => false,
                    }
            }),
        TransportNotification::ChannelMoved {
            server_id,
            previous_parent_id,
            channel,
            ..
        } => session
            .notification_subscriptions
            .iter()
            .any(|subscription| {
                session.selected_virtual_server_id == Some(*server_id)
                    && match subscription.event {
                        NotificationEventKind::Server => true,
                        NotificationEventKind::Channel => matches_channel_tree_subscription(
                            subscription.channel_id,
                            channel,
                            Some(*previous_parent_id),
                        ),
                        NotificationEventKind::TextServer
                        | NotificationEventKind::TextChannel
                        | NotificationEventKind::TextPrivate => false,
                    }
            }),
        TransportNotification::TextMessage { target, .. } => {
            if session.selected_virtual_server_id != Some(target.server_id) {
                return false;
            }

            session
                .notification_subscriptions
                .iter()
                .any(|subscription| match subscription.event {
                    NotificationEventKind::TextServer => target.target_mode == 3,
                    NotificationEventKind::TextChannel => {
                        target.target_mode == 2
                            && subscription
                                .channel_id
                                .is_some_and(|channel_id| Some(channel_id) == target.channel_id)
                    }
                    NotificationEventKind::TextPrivate => {
                        target.target_mode == 1
                            && target.target_client_id == Some(session.client_id)
                    }
                    NotificationEventKind::Server | NotificationEventKind::Channel => false,
                })
        }
    }
}

fn matches_channel_tree_subscription(
    subscription_channel_id: Option<u32>,
    channel: &ChannelSnapshot,
    previous_parent_id: Option<u32>,
) -> bool {
    subscription_channel_id.is_some_and(|channel_id| {
        channel_id == channel.id
            || channel_id == channel.parent_id
            || previous_parent_id == Some(channel_id)
    })
}

pub fn render_notification(notification: &TransportNotification) -> String {
    match notification {
        TransportNotification::ClientEnterView {
            presence,
            from_channel_id,
            reason_id,
        } => render_message(
            "notifycliententerview",
            &[
                ("clid", presence.client_id.to_string()),
                ("client_nickname", presence.login_name.clone()),
                ("client_type", presence.client_type.to_string()),
                ("ctid", presence.channel_id.to_string()),
                ("cfid", from_channel_id.unwrap_or(0).to_string()),
                ("reasonid", reason_id.to_string()),
            ],
        ),
        TransportNotification::ClientUpdated { before, after, .. } => {
            render_message_owned("notifyclientupdated", client_update_fields(before, after))
        }
        TransportNotification::ClientPoke {
            invoker_id,
            invoker_name,
            invoker_uid,
            message,
            ..
        } => render_message(
            "notifyclientpoke",
            &[
                ("invokerid", invoker_id.to_string()),
                ("invokername", invoker_name.clone()),
                ("invokeruid", invoker_uid.clone()),
                ("msg", message.clone()),
            ],
        ),
        TransportNotification::ClientMoved {
            presence,
            from_channel_id,
            reason_id,
            reason_message,
            invoker_id,
            invoker_name,
            invoker_uid,
        } => render_message(
            "notifyclientmoved",
            &[
                ("clid", presence.client_id.to_string()),
                ("cfid", from_channel_id.to_string()),
                ("ctid", presence.channel_id.to_string()),
                ("reasonid", reason_id.to_string()),
                ("reasonmsg", reason_message.clone()),
                ("invokerid", invoker_id.to_string()),
                ("invokername", invoker_name.clone()),
                ("invokeruid", invoker_uid.clone()),
            ],
        ),
        TransportNotification::ClientLeftView {
            presence,
            to_channel_id,
            reason_id,
            reason_message,
            invoker_id,
            invoker_name,
            invoker_uid,
            ban_time,
        } => render_message(
            "notifyclientleftview",
            &[
                ("clid", presence.client_id.to_string()),
                ("cfid", presence.channel_id.to_string()),
                ("ctid", to_channel_id.unwrap_or(0).to_string()),
                ("reasonid", reason_id.to_string()),
                ("reasonmsg", reason_message.clone()),
                ("invokerid", invoker_id.to_string()),
                ("invokername", invoker_name.clone()),
                ("invokeruid", invoker_uid.clone()),
                ("bantime", ban_time.unwrap_or(0).to_string()),
            ],
        ),
        TransportNotification::ChannelEdited {
            channel,
            invoker_id,
            invoker_name,
            ..
        } => render_message(
            "notifychanneledited",
            &[
                ("cid", channel.id.to_string()),
                ("channel_name", channel.name.clone()),
                ("channel_topic", channel.topic.clone()),
                ("invokerid", invoker_id.to_string()),
                ("invokername", invoker_name.clone()),
                ("invokeruid", format!("query-{}", invoker_id)),
            ],
        ),
        TransportNotification::ChannelCreated {
            channel,
            invoker_id,
            invoker_name,
            ..
        } => render_message(
            "notifychannelcreated",
            &[
                ("cid", channel.id.to_string()),
                ("cpid", channel.parent_id.to_string()),
                ("channel_order", channel.order.to_string()),
                ("channel_name", channel.name.clone()),
                ("channel_topic", channel.topic.clone()),
                ("invokerid", invoker_id.to_string()),
                ("invokername", invoker_name.clone()),
                ("invokeruid", format!("query-{}", invoker_id)),
            ],
        ),
        TransportNotification::ChannelDeleted {
            channel,
            invoker_id,
            invoker_name,
            ..
        } => render_message(
            "notifychanneldeleted",
            &[
                ("cid", channel.id.to_string()),
                ("cpid", channel.parent_id.to_string()),
                ("invokerid", invoker_id.to_string()),
                ("invokername", invoker_name.clone()),
                ("invokeruid", format!("query-{}", invoker_id)),
            ],
        ),
        TransportNotification::ChannelMoved {
            channel,
            invoker_id,
            invoker_name,
            ..
        } => render_message(
            "notifychannelmoved",
            &[
                ("cid", channel.id.to_string()),
                ("cpid", channel.parent_id.to_string()),
                ("order", channel.order.to_string()),
                ("channel_name", channel.name.clone()),
                ("invokerid", invoker_id.to_string()),
                ("invokername", invoker_name.clone()),
                ("invokeruid", format!("query-{}", invoker_id)),
            ],
        ),
        TransportNotification::ServerEdited {
            before,
            after,
            invoker_id,
            invoker_name,
            ..
        } => render_message_owned(
            "notifyserveredited",
            server_edited_fields(before, after, *invoker_id, invoker_name),
        ),
        TransportNotification::TalkStatus {
            client_id,
            is_talking,
            ..
        } => render_message(
            "notifytalkstatus",
            &[
                ("clid", client_id.to_string()),
                ("status", if *is_talking { "1".to_string() } else { "0".to_string() }),
            ],
        ),
        TransportNotification::TextMessage {
            target,
            invoker_id,
            invoker_name,
            invoker_uid,
        } => render_message(
            "notifytextmessage",
            &[
                ("targetmode", target.target_mode.to_string()),
                ("msg", target.message.clone()),
                ("invokerid", invoker_id.to_string()),
                ("invokername", invoker_name.clone()),
                ("invokeruid", invoker_uid.clone()),
            ],
        ),
    }
}

fn render_message(name: &str, fields: &[(&str, String)]) -> String {
    let values = fields
        .iter()
        .map(|(key, value)| format!("{}={}", key, encode_query_value(value)))
        .collect::<Vec<_>>()
        .join(" ");
    format!("{} {}", name, values)
}

fn render_message_owned(name: &str, fields: Vec<(String, String)>) -> String {
    let values = fields
        .into_iter()
        .map(|(key, value)| format!("{}={}", key, encode_query_value(&value)))
        .collect::<Vec<_>>()
        .join(" ");
    format!("{} {}", name, values)
}

fn client_update_fields(
    before: &OnlineClientSnapshot,
    after: &OnlineClientSnapshot,
) -> Vec<(String, String)> {
    let mut fields = vec![(String::from("clid"), after.id.to_string())];

    if before.nickname != after.nickname {
        fields.push((String::from("client_nickname"), after.nickname.clone()));
    }
    if before.away != after.away {
        fields.push((
            String::from("client_away"),
            if after.away {
                String::from("1")
            } else {
                String::from("0")
            },
        ));
    }
    if before.away_message != after.away_message {
        fields.push((
            String::from("client_away_message"),
            after.away_message.clone(),
        ));
    }
    if before.input_muted != after.input_muted {
        fields.push((
            String::from("client_input_muted"),
            if after.input_muted {
                String::from("1")
            } else {
                String::from("0")
            },
        ));
    }
    if before.output_muted != after.output_muted {
        fields.push((
            String::from("client_output_muted"),
            if after.output_muted {
                String::from("1")
            } else {
                String::from("0")
            },
        ));
    }

    fields
}

fn server_edited_fields(
    before: &ServerSnapshot,
    after: &ServerSnapshot,
    invoker_id: u64,
    invoker_name: &str,
) -> Vec<(String, String)> {
    let mut fields = vec![(String::from("virtualserver_id"), after.id.to_string())];

    if before.name != after.name {
        fields.push((String::from("virtualserver_name"), after.name.clone()));
    }
    if before.welcome_message != after.welcome_message {
        fields.push((
            String::from("virtualserver_welcomemessage"),
            after.welcome_message.clone(),
        ));
    }
    if before.host_message != after.host_message {
        fields.push((
            String::from("virtualserver_hostmessage"),
            after.host_message.clone(),
        ));
    }
    if before.host_message_mode != after.host_message_mode {
        fields.push((
            String::from("virtualserver_hostmessage_mode"),
            after.host_message_mode.to_string(),
        ));
    }
    if before.ask_for_privilegekey != after.ask_for_privilegekey {
        fields.push((
            String::from("virtualserver_ask_for_privilegekey"),
            after.ask_for_privilegekey.to_string(),
        ));
    }
    if before.max_clients != after.max_clients {
        fields.push((
            String::from("virtualserver_maxclients"),
            after.max_clients.to_string(),
        ));
    }
    if before.antiflood_points_tick_reduce != after.antiflood_points_tick_reduce {
        fields.push((
            String::from("virtualserver_antiflood_points_tick_reduce"),
            after.antiflood_points_tick_reduce.to_string(),
        ));
    }
    if before.antiflood_points_needed_command_block != after.antiflood_points_needed_command_block {
        fields.push((
            String::from("virtualserver_antiflood_points_needed_command_block"),
            after.antiflood_points_needed_command_block.to_string(),
        ));
    }
    if before.antiflood_points_needed_ip_block != after.antiflood_points_needed_ip_block {
        fields.push((
            String::from("virtualserver_antiflood_points_needed_ip_block"),
            after.antiflood_points_needed_ip_block.to_string(),
        ));
    }
    if before.antiflood_ban_time != after.antiflood_ban_time {
        fields.push((
            String::from("virtualserver_antiflood_ban_time"),
            after.antiflood_ban_time.to_string(),
        ));
    }

    fields.push((String::from("invokerid"), invoker_id.to_string()));
    fields.push((String::from("invokername"), invoker_name.to_string()));
    fields.push((String::from("invokeruid"), format!("query-{}", invoker_id)));
    fields
}



