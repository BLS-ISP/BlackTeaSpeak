use std::collections::BTreeMap;

use crate::runtime::musicbot::audio_engine::{request_bot_seek, send_bot_channel_message_locked};
use crate::runtime::musicbot::{MusicBot, MusicBotState};
use crate::runtime::{
    BaselineRuntime, ChannelKind, ChannelSnapshot, OnlineClient, TextMessageTarget,
    current_unix_timestamp, current_unix_timestamp_millis,
};
use crate::transport::{SessionPresence, TransportNotification};

/// Cleans a URL pasted into TeamSpeak 3 / TeaSpeak chat (which wraps links in `[URL]...[/URL]` BBCode).
pub fn clean_chat_url(raw: &str) -> String {
    let mut s = raw.trim().to_string();
    loop {
        let lower = s.to_ascii_lowercase();
        if lower.starts_with("[url=") {
            if let Some(close_bracket) = s.find(']') {
                let target_url = s[5..close_bracket].trim().to_string();
                if !target_url.is_empty() {
                    s = target_url;
                    break;
                }
            }
        }
        if lower.starts_with("[url]") && lower.ends_with("[/url]") && s.len() >= 11 {
            s = s[5..s.len() - 6].trim().to_string();
            continue;
        }
        break;
    }
    s.trim_matches(|c: char| c == '<' || c == '>' || c == '"' || c == '\'')
        .trim()
        .to_string()
}

fn parse_seek_seconds(raw: &str) -> Option<u32> {
    let s = raw.trim().trim_end_matches('s');
    if s.is_empty() {
        return None;
    }
    if let Some((min_str, sec_str)) = s.split_once(':') {
        let mins = min_str.trim().parse::<u32>().ok()?;
        let secs = sec_str.trim().parse::<u32>().ok()?;
        Some(mins.saturating_mul(60).saturating_add(secs))
    } else {
        s.parse::<u32>().ok()
    }
}

impl BaselineRuntime {
    /// Unified Chat Command & Help Interface (`/?`, `/help`, `!?`, `!help`, plus detailed submenus
    /// and direct controls for MusicBot, Queue, Local Files, Channels, User/Profile, Admin/Tokens, and Server).
    pub fn try_handle_musicbot_chat_command(
        &mut self,
        server_id: u32,
        invoker_clid: u64,
        target_mode: u32,
        target_client_id: Option<u64>,
        raw_message: &str,
    ) -> bool {
        let trimmed = raw_message.trim();
        let Some(first_char) = trimmed.chars().next() else {
            return false;
        };
        if first_char != '/' && first_char != '!' {
            return false;
        }

        let invoker_channel_id = self
            .store
            .online_clients
            .get(&invoker_clid)
            .map(|c| c.channel_id)
            .unwrap_or(1);

        let invoker_dbid = self
            .store
            .online_clients
            .get(&invoker_clid)
            .map(|c| c.database_id)
            .unwrap_or(invoker_clid + 1000);

        let body = trimmed[1..].trim();
        if body.is_empty() {
            return false;
        }

        // Normalize `/?<topic>` (e.g. `/?bot`) into `("?", "bot")`
        let (mut cmd_raw, mut arg_raw) = if let Some(after_q) = body.strip_prefix('?') {
            (String::from("?"), after_q.trim())
        } else {
            match body.split_once(char::is_whitespace) {
                Some((c, a)) => (c.trim().to_ascii_lowercase(), a.trim()),
                None => (body.to_ascii_lowercase(), ""),
            }
        };

        // Route `/bot <subcmd> ...` or `/mbot <subcmd> ...` or `/musicbot <subcmd> ...`
        if matches!(cmd_raw.as_str(), "bot" | "mbot" | "musicbot") && !arg_raw.is_empty() {
            let (sub, sub_arg) = match arg_raw.split_once(char::is_whitespace) {
                Some((s, a)) => (s.trim().to_ascii_lowercase(), a.trim()),
                None => (arg_raw.to_ascii_lowercase(), ""),
            };
            match sub.as_str() {
                "?" | "help" | "h" | "menu" | "commands" => {
                    self.send_chat_submenu(
                        server_id,
                        invoker_clid,
                        invoker_channel_id,
                        target_mode,
                        "bot",
                    );
                    return true;
                }
                "queue" | "q" | "playlist" if sub_arg == "?" || sub_arg == "help" => {
                    self.send_chat_submenu(
                        server_id,
                        invoker_clid,
                        invoker_channel_id,
                        target_mode,
                        "queue",
                    );
                    return true;
                }
                "files" | "local" if sub_arg == "?" || sub_arg == "help" => {
                    self.send_chat_submenu(
                        server_id,
                        invoker_clid,
                        invoker_channel_id,
                        target_mode,
                        "files",
                    );
                    return true;
                }
                "list" | "bots" => {
                    cmd_raw = String::from("botlist");
                    arg_raw = sub_arg;
                }
                "rename" | "name" | "nick" => {
                    cmd_raw = String::from("botrename");
                    arg_raw = sub_arg;
                }
                _ => {
                    cmd_raw = sub;
                    arg_raw = sub_arg;
                }
            }
        }

        // Route `/<category> ?` or `/<category> help` to its submenu
        if matches!(arg_raw, "?" | "help" | "-h" | "--help")
            && matches!(
                cmd_raw.as_str(),
                "bot"
                    | "mbot"
                    | "musicbot"
                    | "music"
                    | "queue"
                    | "q"
                    | "playlist"
                    | "files"
                    | "local"
                    | "channel"
                    | "ch"
                    | "room"
                    | "user"
                    | "client"
                    | "me"
                    | "admin"
                    | "mod"
                    | "perm"
                    | "token"
                    | "server"
                    | "srv"
            )
        {
            let topic = cmd_raw.clone();
            self.send_chat_submenu(
                server_id,
                invoker_clid,
                invoker_channel_id,
                target_mode,
                &topic,
            );
            return true;
        }

        // Top-level help & submenu dispatcher: `/? [topic]`, `/help [topic]`, `/commands [topic]`, `/menu [topic]`
        if matches!(cmd_raw.as_str(), "?" | "help" | "h" | "commands" | "menu") {
            let topic = arg_raw.trim_start_matches('/').trim_start_matches('!').trim();
            self.send_chat_submenu(
                server_id,
                invoker_clid,
                invoker_channel_id,
                target_mode,
                topic,
            );
            return true;
        }

        // Category openers when called without arguments
        if arg_raw.is_empty() {
            match cmd_raw.as_str() {
                "bot" | "mbot" | "musicbot" | "music" => {
                    self.send_chat_submenu(
                        server_id,
                        invoker_clid,
                        invoker_channel_id,
                        target_mode,
                        "bot",
                    );
                    return true;
                }
                "channel" | "ch" | "room" => {
                    self.send_chat_submenu(
                        server_id,
                        invoker_clid,
                        invoker_channel_id,
                        target_mode,
                        "channel",
                    );
                    return true;
                }
                "user" | "client" => {
                    self.send_chat_submenu(
                        server_id,
                        invoker_clid,
                        invoker_channel_id,
                        target_mode,
                        "user",
                    );
                    return true;
                }
                "admin" | "mod" | "perm" => {
                    self.send_chat_submenu(
                        server_id,
                        invoker_clid,
                        invoker_channel_id,
                        target_mode,
                        "admin",
                    );
                    return true;
                }
                "server" | "srv" => {
                    self.send_chat_submenu(
                        server_id,
                        invoker_clid,
                        invoker_channel_id,
                        target_mode,
                        "server",
                    );
                    return true;
                }
                _ => {}
            }
        }

        // Handle `/ch <subcmd>` / `/channel <subcmd>`
        if matches!(cmd_raw.as_str(), "channel" | "ch" | "room") {
            return self.handle_channel_chat_subcommand(
                server_id,
                invoker_clid,
                invoker_channel_id,
                target_mode,
                arg_raw,
            );
        }

        // Handle `/server <subcmd>` / `/srv <subcmd>`
        if matches!(cmd_raw.as_str(), "server" | "srv") {
            return self.handle_server_chat_subcommand(
                server_id,
                invoker_clid,
                invoker_channel_id,
                target_mode,
                arg_raw,
            );
        }

        // Find MusicBot in the invoker's current channel (or targeted in private message)
        let channel_bot_id = if target_mode == 1 {
            target_client_id
                .and_then(|tclid| self.music_bot_id_by_client(server_id, tclid))
                .or_else(|| self.find_music_bot_in_channel(server_id, invoker_channel_id))
        } else {
            self.find_music_bot_in_channel(server_id, invoker_channel_id)
        };
        let channel_bot_clid = channel_bot_id.and_then(|bid| self.bot_client_id(bid));

        match cmd_raw.as_str() {
            // =========================================================================
            // 1. MUSICBOT INSTANCE & PLAYBACK COMMANDS
            // =========================================================================
            "spawn" | "create" => {
                let custom_name = if arg_raw.is_empty() {
                    None
                } else {
                    Some(clean_chat_url(arg_raw))
                };
                let bot_id =
                    self.spawn_music_bot_in_channel(server_id, invoker_channel_id, custom_name);
                let bot_clid = self.bot_client_id(bot_id);
                self.send_chat_interface_reply(
                    server_id,
                    invoker_clid,
                    invoker_channel_id,
                    target_mode,
                    bot_clid,
                    "🤖 MusicBot wurde im Raum erstellt! Nutze [b]/play <YouTube-URL>[/b], [b]/files[/b] oder [b]/? bot[/b].",
                );
                true
            }

            "join" | "summon" => {
                if let Some(existing_bot_id) = channel_bot_id {
                    let bot_clid = self.bot_client_id(existing_bot_id);
                    self.send_chat_interface_reply(
                        server_id,
                        invoker_clid,
                        invoker_channel_id,
                        target_mode,
                        bot_clid,
                        "✅ Ich bin bereits in deinem Channel! (Übersicht: [b]/? bot[/b])",
                    );
                    return true;
                }
                if let Some(any_bot_id) = self.find_any_music_bot_on_server(server_id) {
                    if let Some(bot_clid) = self.bot_client_id(any_bot_id) {
                        let old_cid = self
                            .store
                            .online_clients
                            .get(&bot_clid)
                            .map(|c| c.channel_id)
                            .unwrap_or(1);
                        if let Some(client) = self.store.online_clients.get_mut(&bot_clid) {
                            client.channel_id = invoker_channel_id;
                        }
                        if let Some(snap) = self.online_client_snapshot(server_id, bot_clid) {
                            self.broadcast_event(
                                server_id,
                                &TransportNotification::ClientMoved {
                                    presence: SessionPresence {
                                        client_id: snap.id,
                                        login_name: snap.nickname,
                                        unique_identifier: snap.unique_identifier,
                                        client_type: snap.client_type_exact,
                                        server_id,
                                        channel_id: invoker_channel_id,
                                    },
                                    from_channel_id: old_cid,
                                    reason_id: 1,
                                    reason_message: String::new(),
                                    invoker_id: 0,
                                    invoker_name: String::from("Chat"),
                                    invoker_uid: String::new(),
                                },
                            );
                        }
                        self.send_chat_interface_reply(
                            server_id,
                            invoker_clid,
                            invoker_channel_id,
                            target_mode,
                            Some(bot_clid),
                            "🤖 MusicBot ist in deinen Channel gewechselt!",
                        );
                    }
                } else {
                    let bot_id =
                        self.spawn_music_bot_in_channel(server_id, invoker_channel_id, None);
                    let bot_clid = self.bot_client_id(bot_id);
                    self.send_chat_interface_reply(
                        server_id,
                        invoker_clid,
                        invoker_channel_id,
                        target_mode,
                        bot_clid,
                        "🤖 MusicBot erstellt und Channel beigetreten! Nutze [b]/play <URL>[/b] oder [b]/? bot[/b].",
                    );
                }
                true
            }

            "leave" | "delete" | "remove" => {
                let target_bot_id = if !arg_raw.is_empty() {
                    arg_raw
                        .trim_start_matches('#')
                        .trim()
                        .parse::<u32>()
                        .ok()
                        .filter(|id| {
                            self.store
                                .music_bots
                                .get(id)
                                .is_some_and(|b| b.server_id == server_id)
                        })
                        .or(channel_bot_id)
                } else {
                    channel_bot_id
                };
                if let Some(bot_id) = target_bot_id {
                    let bot_clid = self.bot_client_id(bot_id);
                    self.send_chat_interface_reply(
                        server_id,
                        invoker_clid,
                        invoker_channel_id,
                        target_mode,
                        bot_clid,
                        &format!("👋 MusicBot #{} wurde entfernt.", bot_id),
                    );
                    if let Some(mut removed_bot) = self.store.music_bots.remove(&bot_id) {
                        Self::stop_music_bot_playback(&mut removed_bot);
                    }
                    if let Some(bot_clid) = bot_clid {
                        self.remove_session_client(bot_clid, 8, "MusicBot removed".to_string());
                    }
                    self.persist_state_if_configured();
                } else {
                    self.send_chat_interface_reply(
                        server_id,
                        invoker_clid,
                        invoker_channel_id,
                        target_mode,
                        None,
                        "ℹ️ Kein MusicBot in deinem aktuellen Raum gefunden. (Oder nutze [b]/bot delete <Bot-ID>[/b])",
                    );
                }
                true
            }

            "botlist" | "bots" => {
                let mut lines = Vec::new();
                for bot in self.store.music_bots.values().filter(|b| b.server_id == server_id) {
                    let (nick, cid) = bot
                        .linked_client_id
                        .and_then(|clid| self.store.online_clients.get(&clid))
                        .map(|c| (c.nickname.clone(), c.channel_id))
                        .unwrap_or_else(|| (format!("MusicBot {}", bot.id), 0));
                    let ch_name = self
                        .store
                        .channels
                        .get(&server_id)
                        .and_then(|chs| chs.iter().find(|c| c.id == cid))
                        .map(|c| c.name.as_str())
                        .unwrap_or("Unbekannt");
                    let state_str = match bot.state {
                        MusicBotState::Playing => "▶️ Spielt",
                        MusicBotState::Paused => "⏸️ Pausiert",
                        MusicBotState::Stopped => "⏹️ Gestoppt",
                    };
                    lines.push(format!(
                        "• [b]#{} {}[/b] – Raum: {} (cid={}) | {} | Queue: {} Titel | Vol: {}%",
                        bot.id,
                        nick,
                        ch_name,
                        cid,
                        state_str,
                        bot.queue.len(),
                        Self::format_music_bot_volume_percent(&bot.player_volume)
                    ));
                }
                let msg = if lines.is_empty() {
                    String::from("🤖 Aktuell sind keine MusikBots aktiv. Erstelle einen mit [b]/bot spawn[/b].")
                } else {
                    format!(
                        "\n🤖 [b]Aktive MusikBots auf dem Server ({}):[/b]\n{}",
                        lines.len(),
                        lines.join("\n")
                    )
                };
                self.send_chat_interface_reply(
                    server_id,
                    invoker_clid,
                    invoker_channel_id,
                    target_mode,
                    channel_bot_clid,
                    &msg,
                );
                true
            }

            "botrename" => {
                let new_name = arg_raw.trim();
                if new_name.is_empty() {
                    self.send_chat_interface_reply(
                        server_id,
                        invoker_clid,
                        invoker_channel_id,
                        target_mode,
                        channel_bot_clid,
                        "ℹ️ Nutzung: [b]/bot rename <NeuerName>[/b]",
                    );
                    return true;
                }
                if let Some(bot_clid) = channel_bot_clid {
                    let before = self.online_client_snapshot(server_id, bot_clid);
                    if let Some(client) = self.store.online_clients.get_mut(&bot_clid) {
                        client.nickname = new_name.to_string();
                    }
                    let after = self.online_client_snapshot(server_id, bot_clid);
                    if let (Some(before), Some(after)) = (before, after) {
                        self.broadcast_event(
                            server_id,
                            &TransportNotification::ClientUpdated {
                                server_id,
                                before,
                                after,
                            },
                        );
                    }
                    self.send_chat_interface_reply(
                        server_id,
                        invoker_clid,
                        invoker_channel_id,
                        target_mode,
                        Some(bot_clid),
                        &format!("✅ Bot-Name geändert zu: [b]{}[/b]", new_name),
                    );
                } else {
                    self.send_chat_interface_reply(
                        server_id,
                        invoker_clid,
                        invoker_channel_id,
                        target_mode,
                        None,
                        "ℹ️ Kein MusicBot im Raum. Nutze zuerst [b]/bot spawn[/b] oder [b]/bot join[/b].",
                    );
                }
                true
            }

            "play" | "p" => {
                let bot_id = channel_bot_id.unwrap_or_else(|| {
                    self.spawn_music_bot_in_channel(server_id, invoker_channel_id, None)
                });
                let Some(bot_clid) = self.bot_client_id(bot_id) else {
                    return true;
                };

                let url = clean_chat_url(arg_raw);
                if url.is_empty() {
                    let mut resumed_title = None;
                    if let Some(bot) = self.store.music_bots.get_mut(&bot_id) {
                        if bot.current_song_id.is_none() {
                            bot.current_song_id = bot.queue.first().map(|s| s.id);
                        }
                        if let Some(sid) = bot.current_song_id {
                            bot.state = MusicBotState::Playing;
                            Self::resume_music_bot_playback(bot);
                            resumed_title = bot
                                .queue
                                .iter()
                                .find(|s| s.id == sid)
                                .map(|s| s.title.clone());
                        }
                    }
                    self.sync_music_bot_client_state(bot_id);
                    if let Some(title) = resumed_title {
                        self.send_chat_interface_reply(
                            server_id,
                            invoker_clid,
                            invoker_channel_id,
                            target_mode,
                            Some(bot_clid),
                            &format!("▶️ Fortgesetzt: [b]{}[/b]", title),
                        );
                    } else {
                        self.send_chat_interface_reply(
                            server_id,
                            invoker_clid,
                            invoker_channel_id,
                            target_mode,
                            Some(bot_clid),
                            "ℹ️ Keine URL angegeben und Warteschlange ist leer. Nutzung: [b]/play <YouTube-Link | Stream-URL | Datei-Nr>[/b] (Hilfe: [b]/? bot[/b])",
                        );
                    }
                    return true;
                }

                self.enqueue_and_play_bot_url(
                    bot_id,
                    server_id,
                    bot_clid,
                    invoker_dbid,
                    &url,
                    true,
                );
                true
            }

            "add" => {
                let url = clean_chat_url(arg_raw);
                if url.is_empty() {
                    self.send_chat_interface_reply(
                        server_id,
                        invoker_clid,
                        invoker_channel_id,
                        target_mode,
                        channel_bot_clid,
                        "ℹ️ Nutzung: [b]/add <YouTube-Link | Stream-URL | Datei-Nr>[/b]",
                    );
                    return true;
                }
                let bot_id = channel_bot_id.unwrap_or_else(|| {
                    self.spawn_music_bot_in_channel(server_id, invoker_channel_id, None)
                });
                let Some(bot_clid) = self.bot_client_id(bot_id) else {
                    return true;
                };
                self.enqueue_and_play_bot_url(
                    bot_id,
                    server_id,
                    bot_clid,
                    invoker_dbid,
                    &url,
                    false,
                );
                true
            }

            "pause" => {
                if let Some(bot_id) = channel_bot_id {
                    if let Some(bot_clid) = self.bot_client_id(bot_id) {
                        if let Some(bot) = self.store.music_bots.get_mut(&bot_id) {
                            bot.state = MusicBotState::Paused;
                            Self::pause_music_bot_playback(bot);
                        }
                        self.sync_music_bot_client_state(bot_id);
                        self.send_chat_interface_reply(
                            server_id,
                            invoker_clid,
                            invoker_channel_id,
                            target_mode,
                            Some(bot_clid),
                            "⏸️ Wiedergabe pausiert. (Fortsetzen mit [b]/resume[/b])",
                        );
                    }
                }
                true
            }

            "resume" => {
                if let Some(bot_id) = channel_bot_id {
                    if let Some(bot_clid) = self.bot_client_id(bot_id) {
                        if let Some(bot) = self.store.music_bots.get_mut(&bot_id) {
                            if bot.current_song_id.is_none() {
                                bot.current_song_id = bot.queue.first().map(|s| s.id);
                            }
                            if bot.current_song_id.is_some() {
                                bot.state = MusicBotState::Playing;
                                Self::resume_music_bot_playback(bot);
                            }
                        }
                        self.sync_music_bot_client_state(bot_id);
                        self.send_chat_interface_reply(
                            server_id,
                            invoker_clid,
                            invoker_channel_id,
                            target_mode,
                            Some(bot_clid),
                            "▶️ Wiedergabe fortgesetzt.",
                        );
                    }
                }
                true
            }

            "stop" | "s" => {
                if let Some(bot_id) = channel_bot_id {
                    if let Some(bot_clid) = self.bot_client_id(bot_id) {
                        if let Some(bot) = self.store.music_bots.get_mut(&bot_id) {
                            bot.state = MusicBotState::Stopped;
                            Self::stop_music_bot_playback(bot);
                        }
                        self.sync_music_bot_client_state(bot_id);
                        self.send_chat_interface_reply(
                            server_id,
                            invoker_clid,
                            invoker_channel_id,
                            target_mode,
                            Some(bot_clid),
                            "⏹️ Wiedergabe gestoppt.",
                        );
                    }
                }
                true
            }

            "skip" | "next" | "n" => {
                if let Some(bot_id) = channel_bot_id {
                    if let Some(bot_clid) = self.bot_client_id(bot_id) {
                        let next_title = self.step_bot_track(bot_id, true);
                        let msg = if let Some(title) = next_title {
                            format!("⏭️ Übersprungen – Spiele jetzt: [b]{}[/b]", title)
                        } else {
                            String::from("⏹️ Ende der Warteschlange erreicht.")
                        };
                        self.send_chat_interface_reply(
                            server_id,
                            invoker_clid,
                            invoker_channel_id,
                            target_mode,
                            Some(bot_clid),
                            &msg,
                        );
                    }
                }
                true
            }

            "prev" | "previous" => {
                if let Some(bot_id) = channel_bot_id {
                    if let Some(bot_clid) = self.bot_client_id(bot_id) {
                        let prev_title = self.step_bot_track(bot_id, false);
                        if let Some(title) = prev_title {
                            self.send_chat_interface_reply(
                                server_id,
                                invoker_clid,
                                invoker_channel_id,
                                target_mode,
                                Some(bot_clid),
                                &format!("⏮️ Vorheriger Titel: [b]{}[/b]", title),
                            );
                        }
                    }
                }
                true
            }

            "seek" | "ff" => {
                if let Some(bot_id) = channel_bot_id {
                    if let Some(bot_clid) = self.bot_client_id(bot_id) {
                        if let Some(seek_secs) = parse_seek_seconds(arg_raw) {
                            if let Some(bot) = self.store.music_bots.get_mut(&bot_id) {
                                bot.current_song_progress_millis = seek_secs.saturating_mul(1000);
                                bot.current_song_started_at_millis = Some(current_unix_timestamp_millis());
                            }
                            request_bot_seek(bot_id, seek_secs);
                            self.sync_music_bot_client_state(bot_id);
                            let fmt_time = crate::runtime::musicbot::audio_engine::format_duration_mm_ss(seek_secs);
                            self.send_chat_interface_reply(
                                server_id,
                                invoker_clid,
                                invoker_channel_id,
                                target_mode,
                                Some(bot_clid),
                                &format!("⏩ Springe zu Position [b]{}[/b]...", fmt_time),
                            );
                        } else {
                            self.send_chat_interface_reply(
                                server_id,
                                invoker_clid,
                                invoker_channel_id,
                                target_mode,
                                Some(bot_clid),
                                "ℹ️ Nutzung: [b]/seek <mm:ss | Sekunden>[/b] (z.B. [b]/seek 1:30[/b] oder [b]/seek 90[/b])",
                            );
                        }
                    }
                }
                true
            }

            "vol" | "volume" | "v" => {
                if let Some(bot_id) = channel_bot_id {
                    if let Some(bot_clid) = self.bot_client_id(bot_id) {
                        if let Ok(vol) = arg_raw.trim_end_matches('%').trim().parse::<u32>() {
                            let clamped = vol.min(200);
                            let normalized = Self::format_music_bot_volume_factor((clamped as f32) / 100.0);
                            if let Some(bot) = self.store.music_bots.get_mut(&bot_id) {
                                if bot.player_volume != normalized {
                                    bot.player_volume = normalized;
                                    if matches!(bot.state, MusicBotState::Playing) {
                                        let replay_ms = Self::music_bot_current_song_replay_millis(bot);
                                        bot.current_song_progress_millis = replay_ms;
                                        bot.current_song_started_at_millis =
                                            Some(current_unix_timestamp_millis());
                                        request_bot_seek(bot_id, replay_ms / 1000);
                                    }
                                }
                            }
                            self.sync_music_bot_client_state(bot_id);
                            self.send_chat_interface_reply(
                                server_id,
                                invoker_clid,
                                invoker_channel_id,
                                target_mode,
                                Some(bot_clid),
                                &format!("🔊 Lautstärke auf [b]{}%[/b] gesetzt.", clamped),
                            );
                        } else {
                            let cur_vol = self
                                .store
                                .music_bots
                                .get(&bot_id)
                                .map(|b| Self::format_music_bot_volume_percent(&b.player_volume).to_string())
                                .unwrap_or_else(|| String::from("100"));
                            self.send_chat_interface_reply(
                                server_id,
                                invoker_clid,
                                invoker_channel_id,
                                target_mode,
                                Some(bot_clid),
                                &format!(
                                    "🔊 Aktuelle Lautstärke: [b]{}%[/b] (Ändern mit [b]/vol 0-100[/b])",
                                    cur_vol
                                ),
                            );
                        }
                    }
                }
                true
            }

            "np" | "nowplaying" | "status" | "info" => {
                if let Some(bot_id) = channel_bot_id {
                    if let Some(bot_clid) = self.bot_client_id(bot_id) {
                        let status_text = if let Some(bot) = self.store.music_bots.get(&bot_id) {
                            let song_title = Self::music_bot_current_song(bot)
                                .map(|s| s.title.clone())
                                .unwrap_or_else(|| String::from("Kein Titel geladen"));
                            let is_loading = matches!(bot.state, MusicBotState::Playing)
                                && bot.current_song_started_at_millis.is_none();
                            let state_label = if is_loading {
                                "⏳ LÄDT STREAM"
                            } else {
                                match bot.state {
                                    MusicBotState::Playing => "▶️ PLAYING",
                                    MusicBotState::Paused => "⏸️ PAUSED",
                                    MusicBotState::Stopped => "⏹️ STOPPED",
                                }
                            };
                            let replay_secs =
                                Self::music_bot_current_song_replay_millis(bot) / 1000;
                            let max_secs = Self::music_bot_current_song_max_millis(bot) / 1000;
                            let time_info = if max_secs > 0 {
                                let rem_secs = max_secs.saturating_sub(replay_secs);
                                format!(
                                    " | Zeit: {} / {} (noch {})",
                                    crate::runtime::musicbot::audio_engine::format_duration_mm_ss(
                                        replay_secs
                                    ),
                                    crate::runtime::musicbot::audio_engine::format_duration_mm_ss(
                                        max_secs
                                    ),
                                    crate::runtime::musicbot::audio_engine::format_duration_mm_ss(
                                        rem_secs
                                    ),
                                )
                            } else if matches!(bot.state, MusicBotState::Playing) && !is_loading {
                                format!(
                                    " | Laufzeit: {} (LIVE)",
                                    crate::runtime::musicbot::audio_engine::format_duration_mm_ss(
                                        replay_secs
                                    )
                                )
                            } else {
                                String::new()
                            };
                            let loop_label = match bot.playlist_replay_mode {
                                1 => "Loop: Alle",
                                2 => "Loop: Einzel",
                                _ => "Loop: Aus",
                            };
                            format!(
                                "🎵 Status: {} | Titel: [b]{}[/b]{} | Queue: {} Titel | {} | Vol: {}%",
                                state_label,
                                song_title,
                                time_info,
                                bot.queue.len(),
                                loop_label,
                                Self::format_music_bot_volume_percent(&bot.player_volume)
                            )
                        } else {
                            String::from("Kein aktiver MusicBot.")
                        };
                        self.send_chat_interface_reply(
                            server_id,
                            invoker_clid,
                            invoker_channel_id,
                            target_mode,
                            Some(bot_clid),
                            &status_text,
                        );
                    }
                } else {
                    self.send_chat_interface_reply(
                        server_id,
                        invoker_clid,
                        invoker_channel_id,
                        target_mode,
                        None,
                        "ℹ️ Kein MusicBot in diesem Raum. Starte einen mit [b]/bot spawn[/b] oder [b]/play <URL>[/b].",
                    );
                }
                true
            }

            // =========================================================================
            // 2. QUEUE & PLAYLIST COMMANDS
            // =========================================================================
            "queue" | "q" | "list" | "playlist" => {
                if !arg_raw.is_empty() && cmd_raw == "queue" {
                    let url = clean_chat_url(arg_raw);
                    let bot_id = channel_bot_id.unwrap_or_else(|| {
                        self.spawn_music_bot_in_channel(server_id, invoker_channel_id, None)
                    });
                    if let Some(bot_clid) = self.bot_client_id(bot_id) {
                        self.enqueue_and_play_bot_url(
                            bot_id,
                            server_id,
                            bot_clid,
                            invoker_dbid,
                            &url,
                            false,
                        );
                    }
                    return true;
                }
                if let Some(bot_id) = channel_bot_id {
                    if let Some(bot_clid) = self.bot_client_id(bot_id) {
                        let queue_msg = if let Some(bot) = self.store.music_bots.get(&bot_id) {
                            if bot.queue.is_empty() {
                                String::from(
                                    "📋 Die Warteschlange ist aktuell leer. Füge Titel mit [b]/add <URL/Nr>[/b] hinzu (Submenü: [b]/? queue[/b]).",
                                )
                            } else {
                                let loop_badge = match bot.playlist_replay_mode {
                                    1 => "🔁 Loop: Alle",
                                    2 => "🔂 Loop: Einzel",
                                    _ => "➡️ Loop: Aus",
                                };
                                let mut lines = vec![format!(
                                    "\n📋 [b]Warteschlange ({} Titel · {}):[/b]",
                                    bot.queue.len(),
                                    loop_badge
                                )];
                                for (idx, entry) in bot.queue.iter().enumerate().take(12) {
                                    let marker = if bot.current_song_id == Some(entry.id) {
                                        "▶️ "
                                    } else {
                                        "   "
                                    };
                                    let dur_badge = if entry.length_seconds > 0 {
                                        crate::runtime::musicbot::audio_engine::format_duration_mm_ss(
                                            entry.length_seconds,
                                        )
                                    } else if entry.live_stream {
                                        String::from("LIVE")
                                    } else {
                                        entry.url_loader.clone()
                                    };
                                    lines.push(format!(
                                        "{}[b]{}.[/b] {} [{}]",
                                        marker,
                                        idx + 1,
                                        entry.title,
                                        dur_badge
                                    ));
                                }
                                lines.push(String::from(
                                    "💡 Steuerung: [b]/jump <Nr>[/b] · [b]/rm <Nr>[/b] · [b]/qmove <Von> <Nach>[/b] · [b]/shuffle[/b] · [b]/loop <off|all|one>[/b]",
                                ));
                                lines.join("\n")
                            }
                        } else {
                            String::from("Kein MusicBot im Channel.")
                        };
                        self.send_chat_interface_reply(
                            server_id,
                            invoker_clid,
                            invoker_channel_id,
                            target_mode,
                            Some(bot_clid),
                            &queue_msg,
                        );
                    }
                } else {
                    self.send_chat_interface_reply(
                        server_id,
                        invoker_clid,
                        invoker_channel_id,
                        target_mode,
                        None,
                        "ℹ️ Kein MusicBot im Raum. Nutze [b]/? queue[/b] für alle Warteschlangen-Befehle.",
                    );
                }
                true
            }

            "jump" | "select" => {
                if let Some(bot_id) = channel_bot_id {
                    if let Some(bot_clid) = self.bot_client_id(bot_id) {
                        let target_idx = arg_raw
                            .trim_start_matches('#')
                            .trim()
                            .parse::<usize>()
                            .ok()
                            .and_then(|n| n.checked_sub(1));
                        let mut jumped_title = None;
                        if let (Some(idx), Some(bot)) =
                            (target_idx, self.store.music_bots.get_mut(&bot_id))
                        {
                            if let Some(entry) = bot.queue.get(idx) {
                                let sid = entry.id;
                                jumped_title = Some(entry.title.clone());
                                bot.current_song_id = Some(sid);
                                bot.state = MusicBotState::Playing;
                                Self::reset_music_bot_playback_progress(bot);
                            }
                        }
                        self.sync_music_bot_client_state(bot_id);
                        let msg = if let Some(title) = jumped_title {
                            format!("▶️ Springe zu Queue-Titel: [b]{}[/b]", title)
                        } else {
                            String::from(
                                "ℹ️ Ungültige Queue-Nummer. Nutze [b]/queue[/b] und dann [b]/jump <Nr>[/b].",
                            )
                        };
                        self.send_chat_interface_reply(
                            server_id,
                            invoker_clid,
                            invoker_channel_id,
                            target_mode,
                            Some(bot_clid),
                            &msg,
                        );
                    }
                }
                true
            }

            "rm" | "del" => {
                if let Some(bot_id) = channel_bot_id {
                    if let Some(bot_clid) = self.bot_client_id(bot_id) {
                        let target_idx = arg_raw
                            .trim_start_matches('#')
                            .trim()
                            .parse::<usize>()
                            .ok()
                            .and_then(|n| n.checked_sub(1));
                        let mut removed_title = None;
                        if let (Some(idx), Some(bot)) =
                            (target_idx, self.store.music_bots.get_mut(&bot_id))
                        {
                            if idx < bot.queue.len() {
                                let removed = bot.queue.remove(idx);
                                removed_title = Some(removed.title.clone());
                                if bot.current_song_id == Some(removed.id) {
                                    bot.current_song_id = bot
                                        .queue
                                        .get(idx)
                                        .or_else(|| bot.queue.first())
                                        .map(|s| s.id);
                                    if bot.current_song_id.is_some() {
                                        Self::reset_music_bot_playback_progress(bot);
                                    } else {
                                        bot.state = MusicBotState::Stopped;
                                        Self::stop_music_bot_playback(bot);
                                    }
                                }
                            }
                        }
                        self.sync_music_bot_client_state(bot_id);
                        let msg = if let Some(title) = removed_title {
                            format!("🗑️ Aus der Warteschlange entfernt: [b]{}[/b]", title)
                        } else {
                            String::from(
                                "ℹ️ Ungültige Queue-Nummer. Nutze [b]/queue[/b] und dann [b]/rm <Nr>[/b].",
                            )
                        };
                        self.send_chat_interface_reply(
                            server_id,
                            invoker_clid,
                            invoker_channel_id,
                            target_mode,
                            Some(bot_clid),
                            &msg,
                        );
                    }
                }
                true
            }

            "qmove" => {
                if let Some(bot_id) = channel_bot_id {
                    if let Some(bot_clid) = self.bot_client_id(bot_id) {
                        let parts: Vec<&str> = arg_raw.split_whitespace().collect();
                        let parsed = if parts.len() >= 2 {
                            match (
                                parts[0].trim_start_matches('#').parse::<usize>(),
                                parts[1].trim_start_matches('#').parse::<usize>(),
                            ) {
                                (Ok(f), Ok(t)) if f >= 1 && t >= 1 => Some((f - 1, t - 1)),
                                _ => None,
                            }
                        } else {
                            None
                        };
                        let mut moved_title = None;
                        if let (Some((from_idx, to_idx)), Some(bot)) =
                            (parsed, self.store.music_bots.get_mut(&bot_id))
                        {
                            if from_idx < bot.queue.len() && to_idx < bot.queue.len() {
                                let item = bot.queue.remove(from_idx);
                                moved_title = Some((item.title.clone(), to_idx + 1));
                                bot.queue.insert(to_idx, item);
                            }
                        }
                        self.sync_music_bot_client_state(bot_id);
                        let msg = if let Some((title, new_pos)) = moved_title {
                            format!(
                                "🔀 Titel [b]{}[/b] auf Position [b]#{}[/b] verschoben.",
                                title, new_pos
                            )
                        } else {
                            String::from("ℹ️ Nutzung: [b]/qmove <Von-Nr> <Nach-Nr>[/b] (z.B. [b]/qmove 3 1[/b])")
                        };
                        self.send_chat_interface_reply(
                            server_id,
                            invoker_clid,
                            invoker_channel_id,
                            target_mode,
                            Some(bot_clid),
                            &msg,
                        );
                    }
                }
                true
            }

            "shuffle" => {
                if let Some(bot_id) = channel_bot_id {
                    if let Some(bot_clid) = self.bot_client_id(bot_id) {
                        let mut count = 0usize;
                        if let Some(bot) = self.store.music_bots.get_mut(&bot_id) {
                            count = bot.queue.len();
                            if count > 1 {
                                let mut seed = current_unix_timestamp_millis() ^ (bot_id as u64 * 0x9E3779B9);
                                for i in (1..count).rev() {
                                    seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                                    let j = (seed as usize) % (i + 1);
                                    bot.queue.swap(i, j);
                                }
                            }
                        }
                        self.sync_music_bot_client_state(bot_id);
                        self.send_chat_interface_reply(
                            server_id,
                            invoker_clid,
                            invoker_channel_id,
                            target_mode,
                            Some(bot_clid),
                            &format!("🔀 Warteschlange gemischt ({} Titel).", count),
                        );
                    }
                }
                true
            }

            "loop" | "repeat" => {
                if let Some(bot_id) = channel_bot_id {
                    if let Some(bot_clid) = self.bot_client_id(bot_id) {
                        let mode_arg = arg_raw.to_ascii_lowercase();
                        let new_mode = match mode_arg.as_str() {
                            "off" | "0" | "none" | "aus" => Some((0u32, "➡️ Aus (Stopp nach Ende)")),
                            "all" | "queue" | "1" | "alle" => {
                                Some((1u32, "🔁 Alle (Gesamte Warteschlange wiederholen)"))
                            }
                            "one" | "track" | "song" | "single" | "2" | "eins" => {
                                Some((2u32, "🔂 Einzelner Titel (Endlosschleife)"))
                            }
                            _ => None,
                        };
                        if let Some((mode_val, label)) = new_mode {
                            if let Some(bot) = self.store.music_bots.get_mut(&bot_id) {
                                bot.playlist_replay_mode = mode_val;
                            }
                            self.sync_music_bot_client_state(bot_id);
                            self.send_chat_interface_reply(
                                server_id,
                                invoker_clid,
                                invoker_channel_id,
                                target_mode,
                                Some(bot_clid),
                                &format!("🔁 Wiederholungsmodus gesetzt auf: [b]{}[/b]", label),
                            );
                        } else {
                            self.send_chat_interface_reply(
                                server_id,
                                invoker_clid,
                                invoker_channel_id,
                                target_mode,
                                Some(bot_clid),
                                "ℹ️ Nutzung: [b]/loop <off | all | one>[/b] (`off` = Aus, `all` = Gesamte Queue, `one` = Aktueller Song)",
                            );
                        }
                    }
                }
                true
            }

            "clear" => {
                if let Some(bot_id) = channel_bot_id {
                    if let Some(bot_clid) = self.bot_client_id(bot_id) {
                        if let Some(bot) = self.store.music_bots.get_mut(&bot_id) {
                            bot.queue.clear();
                            bot.current_song_id = None;
                            bot.state = MusicBotState::Stopped;
                            Self::stop_music_bot_playback(bot);
                        }
                        self.sync_music_bot_client_state(bot_id);
                        self.send_chat_interface_reply(
                            server_id,
                            invoker_clid,
                            invoker_channel_id,
                            target_mode,
                            Some(bot_clid),
                            "🗑️ Warteschlange geleert und Wiedergabe gestoppt.",
                        );
                    }
                }
                true
            }

            // =========================================================================
            // 3. LOCAL FILES COMMANDS
            // =========================================================================
            "files" | "local" | "localfiles" => {
                let filter_term = arg_raw
                    .strip_prefix("search")
                    .unwrap_or(arg_raw)
                    .trim()
                    .to_ascii_lowercase();
                let local_files = self.discover_local_music_files();
                let msg = if local_files.is_empty() {
                    String::from(
                        "📂 Keine lokalen Audiodateien gefunden. Lege Dateien in [b]data/music/[/b] oder über den Datei-Browser im Channel ab.",
                    )
                } else {
                    let mut lines = vec![format!(
                        "\n📂 [b]Verfügbare lokale Audiodateien ({} gesamt):[/b]",
                        local_files.len()
                    )];
                    for (idx, (name, _url, source, size)) in local_files.iter().enumerate() {
                        if !filter_term.is_empty()
                            && !name.to_ascii_lowercase().contains(&filter_term)
                        {
                            continue;
                        }
                        let kb = (*size as f64 / 1024.0).round() as u64;
                        lines.push(format!(
                            "   [b]{}.[/b] {} [{} · {} KB]",
                            idx + 1,
                            name,
                            source,
                            kb
                        ));
                        if lines.len() >= 18 {
                            break;
                        }
                    }
                    lines.push(String::from(
                        "💡 Abspielen mit [b]/play <Nr/Name>[/b] oder zur Queue hinzufügen mit [b]/add <Nr/Name>[/b]",
                    ));
                    lines.join("\n")
                };
                self.send_chat_interface_reply(
                    server_id,
                    invoker_clid,
                    invoker_channel_id,
                    target_mode,
                    channel_bot_clid,
                    &msg,
                );
                true
            }

            // =========================================================================
            // 4. CHANNEL SHORTCUT COMMANDS
            // =========================================================================
            "channels" | "chlist" => self.handle_channel_chat_subcommand(
                server_id,
                invoker_clid,
                invoker_channel_id,
                target_mode,
                "list",
            ),
            "joinch" => self.handle_channel_chat_subcommand(
                server_id,
                invoker_clid,
                invoker_channel_id,
                target_mode,
                &format!("join {arg_raw}"),
            ),

            // =========================================================================
            // 5. USER & PROFILE COMMANDS
            // =========================================================================
            "whoami" | "me" => {
                let info = if let Some(c) = self.store.online_clients.get(&invoker_clid) {
                    let ch_name = self
                        .store
                        .channels
                        .get(&server_id)
                        .and_then(|chs| chs.iter().find(|ch| ch.id == c.channel_id))
                        .map(|ch| ch.name.clone())
                        .unwrap_or_else(|| format!("Channel #{}", c.channel_id));
                    let sg_names: Vec<String> = c
                        .server_groups
                        .iter()
                        .map(|sgid| {
                            let gname = self
                                .store
                                .server_groups
                                .get(sgid)
                                .map(|g| g.name.as_str())
                                .unwrap_or("Group");
                            format!("{} (sgid={})", gname, sgid)
                        })
                        .collect();
                    format!(
                        "\n👤 [b]Deine Sitzungs- & Benutzerdaten:[/b]\n• [b]Nickname:[/b] {} (CLID: [b]{}[/b] · DBID: [b]{}[/b])\n• [b]Unique-ID:[/b] {}\n• [b]Aktueller Raum:[/b] {} (cid={})\n• [b]Servergruppen:[/b] {}\n• [b]Client / Plattform:[/b] {} ({})",
                        c.nickname,
                        c.id,
                        c.database_id,
                        c.unique_identifier,
                        ch_name,
                        c.channel_id,
                        if sg_names.is_empty() {
                            String::from("Normal (sgid=8)")
                        } else {
                            sg_names.join(", ")
                        },
                        c.version,
                        c.platform
                    )
                } else {
                    format!("👤 Client-ID: {}", invoker_clid)
                };
                self.send_chat_interface_reply(
                    server_id,
                    invoker_clid,
                    invoker_channel_id,
                    target_mode,
                    None,
                    &info,
                );
                true
            }

            "nick" | "nickname" => {
                let new_nick = arg_raw.trim();
                if new_nick.is_empty() {
                    self.send_chat_interface_reply(
                        server_id,
                        invoker_clid,
                        invoker_channel_id,
                        target_mode,
                        None,
                        "ℹ️ Nutzung: [b]/nick <NeuerNickname>[/b]",
                    );
                    return true;
                }
                let before = self.online_client_snapshot(server_id, invoker_clid);
                if let Some(c) = self.store.online_clients.get_mut(&invoker_clid) {
                    c.nickname = new_nick.to_string();
                }
                let after = self.online_client_snapshot(server_id, invoker_clid);
                if let (Some(before), Some(after)) = (before, after) {
                    self.broadcast_event(
                        server_id,
                        &TransportNotification::ClientUpdated {
                            server_id,
                            before,
                            after,
                        },
                    );
                }
                self.send_chat_interface_reply(
                    server_id,
                    invoker_clid,
                    invoker_channel_id,
                    target_mode,
                    None,
                    &format!("✅ Dein Nickname wurde geändert zu: [b]{}[/b]", new_nick),
                );
                true
            }

            "away" | "afk" => {
                let before = self.online_client_snapshot(server_id, invoker_clid);
                let mut is_now_away = true;
                if let Some(c) = self.store.online_clients.get_mut(&invoker_clid) {
                    if arg_raw.is_empty() && c.away {
                        c.away = false;
                        c.away_message.clear();
                        is_now_away = false;
                    } else {
                        c.away = true;
                        c.away_message = arg_raw.to_string();
                        is_now_away = true;
                    }
                }
                let after = self.online_client_snapshot(server_id, invoker_clid);
                if let (Some(before), Some(after)) = (before, after) {
                    self.broadcast_event(
                        server_id,
                        &TransportNotification::ClientUpdated {
                            server_id,
                            before,
                            after,
                        },
                    );
                }
                let msg = if is_now_away {
                    if arg_raw.is_empty() {
                        String::from("💤 Abwesenheitsstatus aktiviert. (Zurückmelden mit [b]/back[/b])")
                    } else {
                        format!("💤 Abwesend gesetzt: [b]{}[/b] (Zurückmelden mit [b]/back[/b])", arg_raw)
                    }
                } else {
                    String::from("🟢 Du bist wieder als anwesend markiert.")
                };
                self.send_chat_interface_reply(
                    server_id,
                    invoker_clid,
                    invoker_channel_id,
                    target_mode,
                    None,
                    &msg,
                );
                true
            }

            "back" => {
                let before = self.online_client_snapshot(server_id, invoker_clid);
                if let Some(c) = self.store.online_clients.get_mut(&invoker_clid) {
                    c.away = false;
                    c.away_message.clear();
                }
                let after = self.online_client_snapshot(server_id, invoker_clid);
                if let (Some(before), Some(after)) = (before, after) {
                    self.broadcast_event(
                        server_id,
                        &TransportNotification::ClientUpdated {
                            server_id,
                            before,
                            after,
                        },
                    );
                }
                self.send_chat_interface_reply(
                    server_id,
                    invoker_clid,
                    invoker_channel_id,
                    target_mode,
                    None,
                    "🟢 Willkommen zurück! Abwesenheitsstatus aufgehoben.",
                );
                true
            }

            "online" | "who" | "users" => {
                let mut lines = Vec::new();
                for c in self
                    .store
                    .online_clients
                    .values()
                    .filter(|c| c.server_id == server_id)
                {
                    let ch_name = self
                        .store
                        .channels
                        .get(&server_id)
                        .and_then(|chs| chs.iter().find(|ch| ch.id == c.channel_id))
                        .map(|ch| ch.name.as_str())
                        .unwrap_or("Channel");
                    let is_bot = c.extra_properties.get("client_type_exact").is_some_and(|v| v == "4");
                    let icon = if is_bot { "🤖" } else if c.away { "💤" } else { "🟢" };
                    lines.push(format!(
                        "{} [b]{}[/b] (clid={}) – Raum: {} (cid={}) [{}]",
                        icon, c.nickname, c.id, ch_name, c.channel_id, c.platform
                    ));
                }
                let msg = format!(
                    "\n👥 [b]Verbundene Clients ({}):[/b]\n{}",
                    lines.len(),
                    lines.join("\n")
                );
                self.send_chat_interface_reply(
                    server_id,
                    invoker_clid,
                    invoker_channel_id,
                    target_mode,
                    None,
                    &msg,
                );
                true
            }

            "poke" => {
                let Some((target_str, poke_msg)) = arg_raw.split_once(char::is_whitespace) else {
                    self.send_chat_interface_reply(
                        server_id,
                        invoker_clid,
                        invoker_channel_id,
                        target_mode,
                        None,
                        "ℹ️ Nutzung: [b]/poke <Name|CLID> <Nachricht>[/b]",
                    );
                    return true;
                };
                if let Some(target_clid) = self.find_online_client_by_name_or_id(server_id, target_str) {
                    let (inv_name, inv_uid) = self
                        .store
                        .online_clients
                        .get(&invoker_clid)
                        .map(|c| (c.nickname.clone(), c.unique_identifier.clone()))
                        .unwrap_or_else(|| (String::from("User"), String::new()));
                    let target_name = self
                        .store
                        .online_clients
                        .get(&target_clid)
                        .map(|c| c.nickname.clone())
                        .unwrap_or_else(|| target_clid.to_string());
                    self.broadcast_event(
                        server_id,
                        &TransportNotification::ClientPoke {
                            server_id,
                            target_client_id: target_clid,
                            invoker_id: invoker_clid,
                            invoker_name: inv_name,
                            invoker_uid: inv_uid,
                            message: poke_msg.trim().to_string(),
                        },
                    );
                    self.send_chat_interface_reply(
                        server_id,
                        invoker_clid,
                        invoker_channel_id,
                        target_mode,
                        None,
                        &format!("⚡ [b]{}[/b] wurde angestupst: \"{}\"", target_name, poke_msg.trim()),
                    );
                } else {
                    self.send_chat_interface_reply(
                        server_id,
                        invoker_clid,
                        invoker_channel_id,
                        target_mode,
                        None,
                        &format!("❌ Client \"{}\" nicht gefunden. Nutze [b]/online[/b] für alle CLIDs.", target_str),
                    );
                }
                true
            }

            "msg" | "pm" | "tell" => {
                let Some((target_str, pm_text)) = arg_raw.split_once(char::is_whitespace) else {
                    self.send_chat_interface_reply(
                        server_id,
                        invoker_clid,
                        invoker_channel_id,
                        target_mode,
                        None,
                        "ℹ️ Nutzung: [b]/msg <Name|CLID> <Nachricht>[/b]",
                    );
                    return true;
                };
                if let Some(target_clid) = self.find_online_client_by_name_or_id(server_id, target_str) {
                    let (inv_name, inv_uid) = self
                        .store
                        .online_clients
                        .get(&invoker_clid)
                        .map(|c| (c.nickname.clone(), c.unique_identifier.clone()))
                        .unwrap_or_else(|| (String::from("User"), String::new()));
                    let target_name = self
                        .store
                        .online_clients
                        .get(&target_clid)
                        .map(|c| c.nickname.clone())
                        .unwrap_or_else(|| target_clid.to_string());
                    let target = TextMessageTarget {
                        target_mode: 1,
                        server_id,
                        channel_id: None,
                        target_client_id: Some(target_clid),
                        message: pm_text.trim().to_string(),
                    };
                    self.broadcast_event(
                        server_id,
                        &TransportNotification::TextMessage {
                            target,
                            invoker_id: invoker_clid,
                            invoker_name: inv_name,
                            invoker_uid: inv_uid,
                        },
                    );
                    self.send_chat_interface_reply(
                        server_id,
                        invoker_clid,
                        invoker_channel_id,
                        target_mode,
                        None,
                        &format!("✉️ Privatnachricht an [b]{}[/b] gesendet.", target_name),
                    );
                } else {
                    self.send_chat_interface_reply(
                        server_id,
                        invoker_clid,
                        invoker_channel_id,
                        target_mode,
                        None,
                        &format!("❌ Client \"{}\" nicht gefunden.", target_str),
                    );
                }
                true
            }

            // =========================================================================
            // 6. ADMIN, MODERATION, GROUPS & PRIVILEGE KEYS
            // =========================================================================
            "token" | "key" | "privilegekey" | "tokenuse" => {
                let token_val = arg_raw.trim();
                if token_val.is_empty() {
                    self.send_chat_interface_reply(
                        server_id,
                        invoker_clid,
                        invoker_channel_id,
                        target_mode,
                        None,
                        "ℹ️ Nutzung: [b]/token <Berechtigungsschlüssel>[/b] (z.B. aus der Server-Startkonsole). Submenü: [b]/? admin[/b]",
                    );
                    return true;
                }
                let before = self.online_client_snapshot(server_id, invoker_clid);
                let mut req = crate::query::CommandRequest {
                    command: String::from("tokenuse"),
                    positional_args: Vec::new(),
                    named_args: BTreeMap::new(),
                    option_groups: Vec::new(),
                    flags: std::collections::BTreeSet::new(),
                };
                req.named_args
                    .insert(String::from("token"), token_val.to_string());
                let qsession = crate::runtime::QuerySessionState {
                    client_id: invoker_clid,
                    connection_ip: String::from("127.0.0.1"),
                    authenticated_login: None,
                    selected_virtual_server_id: Some(server_id),
                    current_channel_id: Some(invoker_channel_id),
                    virtual_mode: false,
                    notification_subscriptions: Vec::new(),
                    actor_client_database_id_override: Some(invoker_dbid),
                    client_nickname: self
                        .store
                        .online_clients
                        .get(&invoker_clid)
                        .map(|c| c.nickname.clone())
                        .unwrap_or_default(),
                    client_away: false,
                    client_away_message: String::new(),
                    client_input_muted: false,
                    client_output_muted: false,
                    is_desktop_client: true,
                    whisper_targets: None,
                    ignored_clients: Vec::new(),
                    subscribed_channels: std::collections::BTreeSet::new(),
                    points: 0,
                    blocked_until_millis: 0,
                    last_points_decay_millis: 0,
                };
                let resp = self.handle_tokenuse(&req, &qsession);
                if resp.error_id == 0 {
                    let after = self.online_client_snapshot(server_id, invoker_clid);
                    if let (Some(before), Some(after)) = (before, after) {
                        self.broadcast_event(
                            server_id,
                            &TransportNotification::ClientUpdated {
                                server_id,
                                before,
                                after,
                            },
                        );
                    }
                    self.send_chat_interface_reply(
                        server_id,
                        invoker_clid,
                        invoker_channel_id,
                        target_mode,
                        None,
                        "🔑 [b]Berechtigungsschlüssel erfolgreich eingelöst![/b] Deine Servergruppen wurden aktualisiert.",
                    );
                } else {
                    self.send_chat_interface_reply(
                        server_id,
                        invoker_clid,
                        invoker_channel_id,
                        target_mode,
                        None,
                        &format!(
                            "❌ Schlüssel konnte nicht eingelöst werden: {} (Fehler {})",
                            resp.message, resp.error_id
                        ),
                    );
                }
                true
            }

            "tokencreate" | "keycreate" => {
                let sgid = arg_raw.trim().parse::<u32>().unwrap_or(9);
                let mut req = crate::query::CommandRequest {
                    command: String::from("tokenadd"),
                    positional_args: Vec::new(),
                    named_args: BTreeMap::new(),
                    option_groups: Vec::new(),
                    flags: std::collections::BTreeSet::new(),
                };
                req.named_args
                    .insert(String::from("tokentype"), String::from("0"));
                req.named_args
                    .insert(String::from("tokenid1"), sgid.to_string());
                req.named_args
                    .insert(String::from("tokenid2"), String::from("0"));
                req.named_args.insert(
                    String::from("tokendescription"),
                    format!("Created via Chat by clid={invoker_clid}"),
                );
                let qsession = self.build_chat_admin_query_session(
                    server_id,
                    invoker_clid,
                    invoker_channel_id,
                    invoker_dbid,
                );
                let resp = self.handle_tokenadd(&req, &qsession);
                let msg = if resp.error_id == 0 {
                    let tok = resp
                        .rows
                        .first()
                        .and_then(|r| r.get("token"))
                        .cloned()
                        .unwrap_or_default();
                    format!(
                        "🔑 Neuer Berechtigungsschlüssel (sgid={}) erstellt:\n[b]{}[/b]\nEinlösbar mit [b]/token {}[/b]",
                        sgid, tok, tok
                    )
                } else {
                    format!(
                        "❌ Keine Berechtigung oder Fehler beim Erstellen: {} (Code {})",
                        resp.message, resp.error_id
                    )
                };
                self.send_chat_interface_reply(
                    server_id,
                    invoker_clid,
                    invoker_channel_id,
                    target_mode,
                    None,
                    &msg,
                );
                true
            }

            "tokenlist" | "keys" => {
                let mut lines = Vec::new();
                for tok in self.store.tokens.values().filter(|t| t.server_id == server_id) {
                    let sgid = tok.actions.first().map(|a| a.action_id1).unwrap_or(9);
                    lines.push(format!(
                        "• [b]{}[/b] (Ziel-Gruppe sgid={} · Nutzungen: {}/{})",
                        tok.token, tok.actions.first().map(|a| a.action_id1).unwrap_or(sgid), tok.uses, tok.max_uses
                    ));
                }
                let msg = if lines.is_empty() {
                    String::from("🔑 Keine offenen Berechtigungsschlüssel vorhanden. Erstelle einen mit [b]/tokencreate [sgid][/b].")
                } else {
                    format!("\n🔑 [b]Offene Berechtigungsschlüssel:[/b]\n{}", lines.join("\n"))
                };
                self.send_chat_interface_reply(
                    server_id,
                    invoker_clid,
                    invoker_channel_id,
                    target_mode,
                    None,
                    &msg,
                );
                true
            }

            "groups" | "grouplist" => {
                let mut sg_lines = Vec::new();
                for g in self.store.server_groups.values() {
                    sg_lines.push(format!("• [b]sgid={}[/b] – {}", g.id, g.name));
                }
                let mut cg_lines = Vec::new();
                for g in self.store.channel_groups.values() {
                    cg_lines.push(format!("• [b]cgid={}[/b] – {}", g.id, g.name));
                }
                let msg = format!(
                    "\n🛡️ [b]Servergruppen:[/b]\n{}\n\n📁 [b]Channel-Gruppen:[/b]\n{}\n💡 Vergeben mit [b]/addgroup <Name|CLID> <SGID>[/b] oder [b]/delgroup <Name|CLID> <SGID>[/b]",
                    sg_lines.join("\n"),
                    cg_lines.join("\n")
                );
                self.send_chat_interface_reply(
                    server_id,
                    invoker_clid,
                    invoker_channel_id,
                    target_mode,
                    None,
                    &msg,
                );
                true
            }

            "addgroup" | "delgroup" => {
                let parts: Vec<&str> = arg_raw.split_whitespace().collect();
                if parts.len() < 2 {
                    self.send_chat_interface_reply(
                        server_id,
                        invoker_clid,
                        invoker_channel_id,
                        target_mode,
                        None,
                        &format!("ℹ️ Nutzung: [b]/{} <Name|CLID> <SGID>[/b] (Gruppen anzeigen mit [b]/groups[/b])", cmd_raw),
                    );
                    return true;
                }
                let Some(target_clid) = self.find_online_client_by_name_or_id(server_id, parts[0]) else {
                    self.send_chat_interface_reply(
                        server_id,
                        invoker_clid,
                        invoker_channel_id,
                        target_mode,
                        None,
                        &format!("❌ Client \"{}\" nicht gefunden.", parts[0]),
                    );
                    return true;
                };
                let Ok(sgid) = parts[1].parse::<u32>() else {
                    self.send_chat_interface_reply(
                        server_id,
                        invoker_clid,
                        invoker_channel_id,
                        target_mode,
                        None,
                        "❌ Ungültige Servergruppen-ID (SGID).",
                    );
                    return true;
                };
                if !self.store.server_groups.contains_key(&sgid) {
                    self.send_chat_interface_reply(
                        server_id,
                        invoker_clid,
                        invoker_channel_id,
                        target_mode,
                        None,
                        &format!("❌ Servergruppe sgid={} existiert nicht. Nutze [b]/groups[/b].", sgid),
                    );
                    return true;
                }
                let before = self.online_client_snapshot(server_id, target_clid);
                let mut target_name = String::new();
                let mut target_dbid = 0u64;
                let mut updated_groups = Vec::new();
                if let Some(c) = self.store.online_clients.get_mut(&target_clid) {
                    target_name = c.nickname.clone();
                    target_dbid = c.database_id;
                    if cmd_raw == "addgroup" {
                        if !c.server_groups.contains(&sgid) {
                            c.server_groups.push(sgid);
                            c.server_groups.sort_unstable();
                        }
                    } else {
                        c.server_groups.retain(|&g| g != sgid);
                        if c.server_groups.is_empty() {
                            c.server_groups.push(8);
                        }
                    }
                    updated_groups = c.server_groups.clone();
                }
                if target_dbid != 0 {
                    self.persist_web_client_server_groups(target_dbid, &updated_groups);
                }
                let after = self.online_client_snapshot(server_id, target_clid);
                if let (Some(before), Some(after)) = (before, after) {
                    self.broadcast_event(
                        server_id,
                        &TransportNotification::ClientUpdated {
                            server_id,
                            before,
                            after,
                        },
                    );
                }
                let gname = self
                    .store
                    .server_groups
                    .get(&sgid)
                    .map(|g| g.name.as_str())
                    .unwrap_or("Gruppe");
                let action_str = if cmd_raw == "addgroup" {
                    "zugewiesen"
                } else {
                    "entzogen"
                };
                self.send_chat_interface_reply(
                    server_id,
                    invoker_clid,
                    invoker_channel_id,
                    target_mode,
                    None,
                    &format!(
                        "✅ Servergruppe [b]{} (sgid={})[/b] für [b]{}[/b] {}.",
                        gname, sgid, target_name, action_str
                    ),
                );
                true
            }

            "kick" | "chkick" => {
                let (target_str, reason) = match arg_raw.split_once(char::is_whitespace) {
                    Some((t, r)) => (t.trim(), r.trim()),
                    None => (arg_raw.trim(), "Gekickt per Chat-Befehl"),
                };
                if target_str.is_empty() {
                    self.send_chat_interface_reply(
                        server_id,
                        invoker_clid,
                        invoker_channel_id,
                        target_mode,
                        None,
                        &format!("ℹ️ Nutzung: [b]/{} <Name|CLID> [Grund][/b]", cmd_raw),
                    );
                    return true;
                }
                let Some(target_clid) = self.find_online_client_by_name_or_id(server_id, target_str) else {
                    self.send_chat_interface_reply(
                        server_id,
                        invoker_clid,
                        invoker_channel_id,
                        target_mode,
                        None,
                        &format!("❌ Client \"{}\" nicht gefunden.", target_str),
                    );
                    return true;
                };
                if cmd_raw == "chkick" {
                    let old_cid = self
                        .store
                        .online_clients
                        .get(&target_clid)
                        .map(|c| c.channel_id)
                        .unwrap_or(1);
                    if let Some(c) = self.store.online_clients.get_mut(&target_clid) {
                        c.channel_id = 1;
                    }
                    if let Some(snap) = self.online_client_snapshot(server_id, target_clid) {
                        let target_name = snap.nickname.clone();
                        self.broadcast_event(
                            server_id,
                            &TransportNotification::ClientMoved {
                                presence: SessionPresence {
                                    client_id: snap.id,
                                    login_name: snap.nickname,
                                    unique_identifier: snap.unique_identifier,
                                    client_type: snap.client_type_exact,
                                    server_id,
                                    channel_id: 1,
                                },
                                from_channel_id: old_cid,
                                reason_id: 4,
                                reason_message: reason.to_string(),
                                invoker_id: 0,
                                invoker_name: String::from("System"),
                                invoker_uid: String::new(),
                            },
                        );
                        self.send_chat_interface_reply(
                            server_id,
                            invoker_clid,
                            invoker_channel_id,
                            target_mode,
                            None,
                            &format!("🥾 [b]{}[/b] wurde aus dem Channel gekickt ({}).", target_name, reason),
                        );
                    }
                } else if let Some(snap) = self.online_client_snapshot(server_id, target_clid) {
                    let target_name = snap.nickname.clone();
                    self.remove_session_client(target_clid, 5, reason.to_string());
                    self.broadcast_event(
                        server_id,
                        &TransportNotification::ClientLeftView {
                            presence: SessionPresence {
                                client_id: snap.id,
                                login_name: snap.nickname,
                                unique_identifier: snap.unique_identifier,
                                client_type: snap.client_type_exact,
                                server_id,
                                channel_id: snap.channel_id,
                            },
                            to_channel_id: None,
                            reason_id: 5,
                            reason_message: reason.to_string(),
                            invoker_id: invoker_clid,
                            invoker_name: String::from("Admin"),
                            invoker_uid: String::new(),
                            ban_time: None,
                        },
                    );
                    self.send_chat_interface_reply(
                        server_id,
                        invoker_clid,
                        invoker_channel_id,
                        target_mode,
                        None,
                        &format!("🥾 [b]{}[/b] wurde vom Server gekickt ({}).", target_name, reason),
                    );
                }
                true
            }

            "moveclient" => {
                let Some((target_str, ch_str)) = arg_raw.split_once(char::is_whitespace) else {
                    self.send_chat_interface_reply(
                        server_id,
                        invoker_clid,
                        invoker_channel_id,
                        target_mode,
                        None,
                        "ℹ️ Nutzung: [b]/moveclient <Name|CLID> <Channel-ID|Name>[/b]",
                    );
                    return true;
                };
                let Some(target_clid) = self.find_online_client_by_name_or_id(server_id, target_str) else {
                    self.send_chat_interface_reply(
                        server_id,
                        invoker_clid,
                        invoker_channel_id,
                        target_mode,
                        None,
                        &format!("❌ Client \"{}\" nicht gefunden.", target_str),
                    );
                    return true;
                };
                let Some((dest_cid, dest_name)) = self.find_channel_by_name_or_id(server_id, ch_str.trim()) else {
                    self.send_chat_interface_reply(
                        server_id,
                        invoker_clid,
                        invoker_channel_id,
                        target_mode,
                        None,
                        &format!("❌ Channel \"{}\" nicht gefunden.", ch_str.trim()),
                    );
                    return true;
                };
                let old_cid = self
                    .store
                    .online_clients
                    .get(&target_clid)
                    .map(|c| c.channel_id)
                    .unwrap_or(1);
                if let Some(c) = self.store.online_clients.get_mut(&target_clid) {
                    c.channel_id = dest_cid;
                }
                if let Some(snap) = self.online_client_snapshot(server_id, target_clid) {
                    let target_name = snap.nickname.clone();
                    self.broadcast_event(
                        server_id,
                        &TransportNotification::ClientMoved {
                            presence: SessionPresence {
                                client_id: snap.id,
                                login_name: snap.nickname,
                                unique_identifier: snap.unique_identifier,
                                client_type: snap.client_type_exact,
                                server_id,
                                channel_id: dest_cid,
                            },
                            from_channel_id: old_cid,
                            reason_id: 1,
                            reason_message: String::new(),
                            invoker_id: 0,
                            invoker_name: String::from("Admin"),
                            invoker_uid: String::new(),
                        },
                    );
                    self.send_chat_interface_reply(
                        server_id,
                        invoker_clid,
                        invoker_channel_id,
                        target_mode,
                        None,
                        &format!(
                            "➡️ [b]{}[/b] wurde in Raum [b]{}[/b] (cid={}) verschoben.",
                            target_name, dest_name, dest_cid
                        ),
                    );
                }
                true
            }

            // =========================================================================
            // 7. SERVER SHORTCUTS
            // =========================================================================
            "serverinfo" => self.handle_server_chat_subcommand(
                server_id,
                invoker_clid,
                invoker_channel_id,
                target_mode,
                "info",
            ),
            "ports" => self.handle_server_chat_subcommand(
                server_id,
                invoker_clid,
                invoker_channel_id,
                target_mode,
                "ports",
            ),

            _ => false,
        }
    }

    fn send_chat_submenu(
        &mut self,
        server_id: u32,
        invoker_clid: u64,
        invoker_channel_id: u32,
        target_mode: u32,
        topic_raw: &str,
    ) {
        let topic = topic_raw.trim().to_ascii_lowercase();
        let channel_bot_clid = self
            .find_music_bot_in_channel(server_id, invoker_channel_id)
            .and_then(|bid| self.bot_client_id(bid));

        let msg = match topic.as_str() {
            "" | "all" | "main" | "overview" => concat!(
                "\n🧭 [b]BlackTeaSpeak Chat-Interface – Befehlsübersicht (/?)[/b]\n",
                "Wähle ein Submenü für detaillierte Einzelsteuerungen:\n",
                "• [b]/? bot[/b]      – 🎵 [b]MusikBot-Steuerung[/b] (Spawn, Play, Pause, Skip, Seek, Vol, Status)\n",
                "• [b]/? queue[/b]    – 📋 [b]Warteschlange & Playlist[/b] (Queue, Jump, Move, Shuffle, Loop, Clear)\n",
                "• [b]/? files[/b]    – 📂 [b]Lokale Audiodateien[/b] (Server-Musik auflisten, suchen & abspielen)\n",
                "• [b]/? channel[/b]  – 💬 [b]Channel- & Raumsteuerung[/b] (Info, Liste, Join, Erstellen, Topic, Passwort)\n",
                "• [b]/? user[/b]     – 👤 [b]Benutzer & Eigensteuerung[/b] (Whoami, Nickname, Away, Online-Liste, Poke, PM)\n",
                "• [b]/? admin[/b]    – 🛡️ [b]Moderation & Schlüssel[/b] (Privilege-Key einlösen/erstellen, Gruppen, Kick, Move)\n",
                "• [b]/? server[/b]   – 🖥️ [b]Server-Info & Diagnose[/b] (Serverstatus, aktive Ports & Protokolle)\n",
                "⚡ [b]Schnellbefehle:[/b] [b]/play <URL>[/b] · [b]/pause[/b] · [b]/skip[/b] · [b]/vol <0-100>[/b] · [b]/np[/b] · [b]/token <Key>[/b] · [b]/whoami[/b]\n",
                "💡 [i]Tipp: Alle Befehle funktionieren sowohl mit [b]/[/b] als auch mit [b]![/b] (z.B. /? oder !?, /play oder !play).[/i]"
            ),

            "bot" | "mbot" | "musicbot" | "music" => concat!(
                "\n🎵 [b]Submenü: MusikBot-Steuerung (/? bot)[/b]\n",
                "[b]1. Bot-Instanz & Raum:[/b]\n",
                "• [b]/bot spawn [Name][/b] ([b]/spawn[/b]) – Neuen MusikBot im aktuellen Raum erstellen\n",
                "• [b]/bot join[/b] ([b]/join[/b]) – Vorhandenen MusikBot in deinen Raum holen\n",
                "• [b]/bot leave[/b] ([b]/leave[/b]) – MusikBot aus dem Raum entfernen\n",
                "• [b]/bot rename <Name>[/b] – Namen des MusikBots ändern\n",
                "• [b]/bot list[/b] ([b]/bots[/b]) – Alle MusikBots auf dem Server anzeigen\n",
                "[b]2. Wiedergabe & Stream-Kontrolle:[/b]\n",
                "• [b]/play <URL/Nr/Datei>[/b] ([b]/p[/b]) – YouTube-Link, Stream oder lokale Datei sofort starten\n",
                "• [b]/add <URL/Nr/Datei>[/b] – Titel ans Ende der Warteschlange anhängen\n",
                "• [b]/pause[/b] · [b]/resume[/b] · [b]/stop[/b] ([b]/s[/b]) – Pausieren, Fortsetzen, Stoppen\n",
                "• [b]/skip[/b] ([b]/next[/b]) · [b]/prev[/b] ([b]/back[/b]) – Nächster / vorheriger Track\n",
                "• [b]/seek <mm:ss | Sek>[/b] – Im aktuellen Titel spulen (z.B. [b]/seek 1:30[/b])\n",
                "• [b]/vol <0-100>[/b] ([b]/v[/b]) – Lautstärke anzeigen oder setzen (z.B. [b]/vol 75[/b])\n",
                "• [b]/np[/b] ([b]/status[/b]) – Aktueller Titel, Fortschritt, Restzeit & Lautstärke\n",
                "👉 [b]Weitere Bot-Menüs:[/b] [b]/? queue[/b] (Warteschlange/Loop) · [b]/? files[/b] (Lokale Dateien)"
            ),

            "queue" | "q" | "playlist" => concat!(
                "\n📋 [b]Submenü: MusikBot-Warteschlange & Playlist (/? queue)[/b]\n",
                "• [b]/queue[/b] ([b]/q[/b], [b]/list[/b]) – Aktuelle Warteschlange mit Position & Dauer anzeigen\n",
                "• [b]/add <URL/Nr/Datei>[/b] – Titel ans Ende der Warteschlange anfügen\n",
                "• [b]/jump <Nr>[/b] ([b]/select <Nr>[/b]) – Sofort zu Track [b]#Nr[/b] in der Warteschlange springen\n",
                "• [b]/rm <Nr>[/b] ([b]/del <Nr>[/b]) – Track [b]#Nr[/b] aus der Warteschlange entfernen\n",
                "• [b]/qmove <Von> <Nach>[/b] – Track von Position #Von auf #Nach verschieben (z.B. [b]/qmove 3 1[/b])\n",
                "• [b]/shuffle[/b] – Warteschlange zufällig mischen\n",
                "• [b]/loop <off|all|one>[/b] ([b]/repeat[/b]) – Wiederholungsmodus (`off` = Aus, `all` = Queue, `one` = Track)\n",
                "• [b]/clear[/b] – Gesamte Warteschlange leeren und Wiedergabe stoppen\n",
                "👉 Zurück zur Hauptübersicht mit [b]/?[/b] oder Bot-Steuerung mit [b]/? bot[/b]"
            ),

            "files" | "local" | "localfiles" => concat!(
                "\n📂 [b]Submenü: Lokale Audiodateien & Channel-Speicher (/? files)[/b]\n",
                "• [b]/files[/b] ([b]/local[/b]) – Alle lokalen Audiodateien (`data/music/` & Channel-Dateien) mit #Nr auflisten\n",
                "• [b]/files <Suchbegriff>[/b] – Lokale Audiodateien nach Dateinamen durchsuchen\n",
                "• [b]/play <Datei-Nr | Name>[/b] – Lokale Datei direkt per Nummer (z.B. [b]/play 1[/b]) oder Name abspielen\n",
                "• [b]/add <Datei-Nr | Name>[/b] – Lokale Datei per Nummer oder Name in die Warteschlange legen\n",
                "👉 Zurück zur Hauptübersicht mit [b]/?[/b] oder Queue-Menü mit [b]/? queue[/b]"
            ),

            "channel" | "ch" | "room" => concat!(
                "\n💬 [b]Submenü: Channel- & Raumsteuerung (/? channel)[/b]\n",
                "• [b]/ch info[/b] – Details zum aktuellen Raum (ID, Codec, Qualität, Talk-Power, Max-Clients)\n",
                "• [b]/ch list[/b] ([b]/channels[/b]) – Alle Räume auf dem Server mit ID & Nutzeranzahl anzeigen\n",
                "• [b]/ch join <ID|Name>[/b] ([b]/joinch[/b]) – Dich selbst in den angegebenen Raum verschieben\n",
                "• [b]/ch create <Name> [perm|semi|temp][/b] – Neuen Channel erstellen (Standard: `perm`)\n",
                "• [b]/ch topic <Text>[/b] – Thema (Topic) des aktuellen Raums ändern\n",
                "• [b]/ch pass <Passwort|off>[/b] – Channel-Passwort setzen oder mit `off` entfernen\n",
                "• [b]/ch talkpower <0-100>[/b] – Benötigte Talk-Power im aktuellen Raum festlegen\n",
                "👉 Zurück zur Hauptübersicht mit [b]/?[/b]"
            ),

            "user" | "client" | "me" => concat!(
                "\n👤 [b]Submenü: Benutzer- & Eigensteuerung (/? user)[/b]\n",
                "• [b]/whoami[/b] ([b]/me[/b]) – Eigene CLID, DBID, UID, aktuellen Raum & Servergruppen anzeigen\n",
                "• [b]/nick <NeuerName>[/b] – Eigenen Nickname sofort ändern\n",
                "• [b]/away [Nachricht][/b] · [b]/back[/b] – Abwesenheitsstatus setzen oder zurückmelden\n",
                "• [b]/online[/b] ([b]/who[/b], [b]/users[/b]) – Alle verbundenen Clients & Bots mit Raum & Plattform auflisten\n",
                "• [b]/poke <Name|CLID> <Text>[/b] – Einen anderen Client mit Popup-Nachricht anstupsen\n",
                "• [b]/msg <Name|CLID> <Text>[/b] – Private Nachricht an einen Client senden\n",
                "👉 Zurück zur Hauptübersicht mit [b]/?[/b]"
            ),

            "admin" | "mod" | "perm" | "token" => concat!(
                "\n🛡️ [b]Submenü: Moderation, Gruppen & Berechtigungsschlüssel (/? admin)[/b]\n",
                "• [b]/token <Schlüssel>[/b] ([b]/key[/b]) – Berechtigungsschlüssel (Privilege Key) direkt im Chat einlösen\n",
                "• [b]/tokencreate [SGID][/b] – Neuen Berechtigungsschlüssel generieren (Standard: `9` = Server Admin)\n",
                "• [b]/tokenlist[/b] – Alle offenen Berechtigungsschlüssel anzeigen\n",
                "• [b]/groups[/b] – Alle Servergruppen (`sgid`) und Channel-Gruppen (`cgid`) auflisten\n",
                "• [b]/addgroup <Name|CLID> <SGID>[/b] – Einem Client eine Servergruppe zuweisen\n",
                "• [b]/delgroup <Name|CLID> <SGID>[/b] – Einem Client eine Servergruppe entziehen\n",
                "• [b]/kick <Name|CLID> [Grund][/b] – Client vom Server kicken\n",
                "• [b]/chkick <Name|CLID> [Grund][/b] – Client aus dem Channel in die Lobby verschieben\n",
                "• [b]/moveclient <Name|CLID> <Raum-ID|Name>[/b] – Client in einen anderen Raum verschieben\n",
                "👉 Zurück zur Hauptübersicht mit [b]/?[/b]"
            ),

            "server" | "srv" => concat!(
                "\n🖥️ [b]Submenü: Server-Info & Diagnose (/? server)[/b]\n",
                "• [b]/server info[/b] ([b]/serverinfo[/b]) – Servername, Version, Clients, MusikBots & Kanäle anzeigen\n",
                "• [b]/server ports[/b] ([b]/ports[/b]) – Aktive Ports & Protokoll-Listener anzeigen\n",
                "👉 Zurück zur Hauptübersicht mit [b]/?[/b]"
            ),

            _ => concat!(
                "\n❓ Unbekanntes Submenü. Verfügbare Themen:\n",
                "• [b]/?[/b] (Hauptübersicht) · [b]/? bot[/b] · [b]/? queue[/b] · [b]/? files[/b] · [b]/? channel[/b] · [b]/? user[/b] · [b]/? admin[/b] · [b]/? server[/b]"
            ),
        };

        let sender_bot = if matches!(
            topic.as_str(),
            "bot" | "mbot" | "musicbot" | "music" | "queue" | "q" | "playlist" | "files" | "local"
        ) {
            channel_bot_clid
        } else {
            None
        };

        self.send_chat_interface_reply(
            server_id,
            invoker_clid,
            invoker_channel_id,
            target_mode,
            sender_bot,
            msg,
        );
    }

    fn handle_channel_chat_subcommand(
        &mut self,
        server_id: u32,
        invoker_clid: u64,
        invoker_channel_id: u32,
        target_mode: u32,
        arg_raw: &str,
    ) -> bool {
        let (sub, sub_arg) = match arg_raw.split_once(char::is_whitespace) {
            Some((s, a)) => (s.trim().to_ascii_lowercase(), a.trim()),
            None => (arg_raw.trim().to_ascii_lowercase(), ""),
        };

        match sub.as_str() {
            "" | "?" | "help" => {
                self.send_chat_submenu(
                    server_id,
                    invoker_clid,
                    invoker_channel_id,
                    target_mode,
                    "channel",
                );
                true
            }
            "info" => {
                let info_msg = if let Some(ch) = self
                    .store
                    .channels
                    .get(&server_id)
                    .and_then(|chs| chs.iter().find(|c| c.id == invoker_channel_id))
                {
                    let users_in_ch = self
                        .store
                        .online_clients
                        .values()
                        .filter(|c| c.server_id == server_id && c.channel_id == ch.id)
                        .count();
                    let kind_str = match ch.kind {
                        ChannelKind::Permanent => "Permanent",
                        ChannelKind::SemiPermanent => "Semi-Permanent",
                        ChannelKind::Temporary => "Temporär",
                    };
                    format!(
                        "\n💬 [b]Raum-Info: {} (cid={})[/b]\n• [b]Typ:[/b] {}{}\n• [b]Thema (Topic):[/b] {}\n• [b]Codec / Qualität:[/b] Opus ({}) / Stufe {}\n• [b]Benötigte Talk-Power:[/b] {}\n• [b]Clients im Raum:[/b] {} (Max: {})",
                        ch.name,
                        ch.id,
                        kind_str,
                        if ch.flag_default { " · Standard-Raum" } else { "" },
                        if ch.topic.is_empty() { "—" } else { &ch.topic },
                        ch.codec,
                        ch.codec_quality,
                        ch.needed_talk_power(),
                        users_in_ch,
                        if ch.maxclients < 0 {
                            String::from("Unbegrenzt")
                        } else {
                            ch.maxclients.to_string()
                        }
                    )
                } else {
                    format!("💬 Aktueller Raum: cid={}", invoker_channel_id)
                };
                self.send_chat_interface_reply(
                    server_id,
                    invoker_clid,
                    invoker_channel_id,
                    target_mode,
                    None,
                    &info_msg,
                );
                true
            }
            "list" | "ls" => {
                let mut lines = Vec::new();
                if let Some(channels) = self.store.channels.get(&server_id) {
                    for ch in channels {
                        let count = self
                            .store
                            .online_clients
                            .values()
                            .filter(|c| c.server_id == server_id && c.channel_id == ch.id)
                            .count();
                        let marker = if ch.id == invoker_channel_id {
                            "📍 "
                        } else {
                            "   "
                        };
                        let pass_icon = if ch.flag_password || !ch.password.is_empty() {
                            " 🔒"
                        } else {
                            ""
                        };
                        lines.push(format!(
                            "{}[b]#{}: {}[/b]{} ({} Nutzer)",
                            marker, ch.id, ch.name, pass_icon, count
                        ));
                    }
                }
                let msg = format!(
                    "\n💬 [b]Channels auf dem Server ({}):[/b]\n{}\n💡 Wechseln mit [b]/ch join <ID|Name>[/b]",
                    lines.len(),
                    lines.join("\n")
                );
                self.send_chat_interface_reply(
                    server_id,
                    invoker_clid,
                    invoker_channel_id,
                    target_mode,
                    None,
                    &msg,
                );
                true
            }
            "join" | "move" | "goto" => {
                if sub_arg.is_empty() {
                    self.send_chat_interface_reply(
                        server_id,
                        invoker_clid,
                        invoker_channel_id,
                        target_mode,
                        None,
                        "ℹ️ Nutzung: [b]/ch join <Channel-ID | Name>[/b] (Liste: [b]/ch list[/b])",
                    );
                    return true;
                }
                if let Some((dest_cid, dest_name)) =
                    self.find_channel_by_name_or_id(server_id, sub_arg)
                {
                    if let Some(c) = self.store.online_clients.get_mut(&invoker_clid) {
                        c.channel_id = dest_cid;
                    }
                    if let Some(snap) = self.online_client_snapshot(server_id, invoker_clid) {
                        self.broadcast_event(
                            server_id,
                            &TransportNotification::ClientMoved {
                                presence: SessionPresence {
                                    client_id: snap.id,
                                    login_name: snap.nickname,
                                    unique_identifier: snap.unique_identifier,
                                    client_type: snap.client_type_exact,
                                    server_id,
                                    channel_id: dest_cid,
                                },
                                from_channel_id: invoker_channel_id,
                                reason_id: 0,
                                reason_message: String::new(),
                                invoker_id: 0,
                                invoker_name: String::from("Chat"),
                                invoker_uid: String::new(),
                            },
                        );
                    }
                    self.send_chat_interface_reply(
                        server_id,
                        invoker_clid,
                        dest_cid,
                        target_mode,
                        None,
                        &format!("✅ Gewechselt in Raum [b]{}[/b] (cid={}).", dest_name, dest_cid),
                    );
                } else {
                    self.send_chat_interface_reply(
                        server_id,
                        invoker_clid,
                        invoker_channel_id,
                        target_mode,
                        None,
                        &format!(
                            "❌ Raum \"{}\" nicht gefunden. Nutze [b]/ch list[/b].",
                            sub_arg
                        ),
                    );
                }
                true
            }
            "create" | "add" => {
                if sub_arg.is_empty() {
                    self.send_chat_interface_reply(
                        server_id,
                        invoker_clid,
                        invoker_channel_id,
                        target_mode,
                        None,
                        "ℹ️ Nutzung: [b]/ch create <Name> [perm|semi|temp][/b]",
                    );
                    return true;
                }
                let (ch_name, kind) = if let Some(stripped) = sub_arg.strip_suffix(" temp") {
                    (stripped.trim(), ChannelKind::Temporary)
                } else if let Some(stripped) = sub_arg.strip_suffix(" semi") {
                    (stripped.trim(), ChannelKind::SemiPermanent)
                } else if let Some(stripped) = sub_arg.strip_suffix(" perm") {
                    (stripped.trim(), ChannelKind::Permanent)
                } else {
                    (sub_arg.trim(), ChannelKind::Permanent)
                };
                let mut req = crate::query::CommandRequest {
                    command: String::from("channelcreate"),
                    positional_args: Vec::new(),
                    named_args: BTreeMap::new(),
                    option_groups: Vec::new(),
                    flags: std::collections::BTreeSet::new(),
                };
                req.named_args
                    .insert(String::from("channel_name"), ch_name.to_string());
                req.named_args.insert(
                    String::from("channel_flag_permanent"),
                    if matches!(kind, ChannelKind::Permanent) {
                        String::from("1")
                    } else {
                        String::from("0")
                    },
                );
                req.named_args.insert(
                    String::from("channel_flag_semi_permanent"),
                    if matches!(kind, ChannelKind::SemiPermanent) {
                        String::from("1")
                    } else {
                        String::from("0")
                    },
                );
                let invoker_dbid = self
                    .store
                    .online_clients
                    .get(&invoker_clid)
                    .map(|c| c.database_id)
                    .unwrap_or(invoker_clid + 1000);
                let mut qsession = self.build_chat_admin_query_session(
                    server_id,
                    invoker_clid,
                    invoker_channel_id,
                    invoker_dbid,
                );
                let (resp, notifs) = self.execute_command_for_chat(req, &mut qsession);
                for n in &notifs {
                    self.broadcast_event(server_id, n);
                }
                let msg = if resp.error_id == 0 {
                    let new_cid = resp
                        .rows
                        .first()
                        .and_then(|r| r.get("cid"))
                        .cloned()
                        .unwrap_or_default();
                    format!(
                        "✅ Channel [b]{}[/b] (cid={}) erfolgreich erstellt!",
                        ch_name, new_cid
                    )
                } else {
                    format!(
                        "❌ Channel konnte nicht erstellt werden: {} (Code {})",
                        resp.message, resp.error_id
                    )
                };
                self.send_chat_interface_reply(
                    server_id,
                    invoker_clid,
                    invoker_channel_id,
                    target_mode,
                    None,
                    &msg,
                );
                true
            }
            "topic" => {
                let mut snapshot = None;
                if let Some(channels) = self.store.channels.get_mut(&server_id) {
                    if let Some(ch) = channels.iter_mut().find(|c| c.id == invoker_channel_id) {
                        ch.topic = sub_arg.to_string();
                        let ch_clone = ch.clone();
                        let _ = self.db.save_channel(server_id, &ch_clone);
                        snapshot = Some(ChannelSnapshot::from_channel(ch, 0));
                    }
                }
                if let Some(channel) = snapshot {
                    self.broadcast_event(
                        server_id,
                        &TransportNotification::ChannelEdited {
                            server_id,
                            channel,
                            description_changed: false,
                            invoker_id: 0,
                            invoker_name: String::from("Chat"),
                        },
                    );
                    self.send_chat_interface_reply(
                        server_id,
                        invoker_clid,
                        invoker_channel_id,
                        target_mode,
                        None,
                        &format!("✅ Channel-Thema aktualisiert auf: [b]{}[/b]", sub_arg),
                    );
                }
                true
            }
            "pass" | "password" => {
                let clear_pw = sub_arg.is_empty() || matches!(sub_arg, "off" | "none" | "clear");
                if let Some(channels) = self.store.channels.get_mut(&server_id) {
                    if let Some(ch) = channels.iter_mut().find(|c| c.id == invoker_channel_id) {
                        if clear_pw {
                            ch.password.clear();
                            ch.flag_password = false;
                        } else {
                            ch.password = sub_arg.to_string();
                            ch.flag_password = true;
                        }
                        let ch_clone = ch.clone();
                        let _ = self.db.save_channel(server_id, &ch_clone);
                    }
                }
                let msg = if clear_pw {
                    "🔓 Channel-Passwort wurde entfernt."
                } else {
                    "🔒 Channel-Passwort wurde gesetzt."
                };
                self.send_chat_interface_reply(
                    server_id,
                    invoker_clid,
                    invoker_channel_id,
                    target_mode,
                    None,
                    msg,
                );
                true
            }
            "talkpower" | "tp" => {
                let tp = sub_arg.parse::<i64>().unwrap_or(0).max(0);
                if let Some(channels) = self.store.channels.get_mut(&server_id) {
                    if let Some(ch) = channels.iter_mut().find(|c| c.id == invoker_channel_id) {
                        ch.permissions.insert(
                            String::from("i_channel_needed_talk_power"),
                            crate::runtime::PermissionAssignment {
                                value: tp,
                                skipped: false,
                                negated: false,
                            },
                        );
                        let ch_clone = ch.clone();
                        let _ = self.db.save_channel(server_id, &ch_clone);
                    }
                }
                self.send_chat_interface_reply(
                    server_id,
                    invoker_clid,
                    invoker_channel_id,
                    target_mode,
                    None,
                    &format!(
                        "🎙️ Benötigte Talk-Power im aktuellen Raum auf [b]{}[/b] gesetzt.",
                        tp
                    ),
                );
                true
            }
            _ => {
                self.send_chat_submenu(
                    server_id,
                    invoker_clid,
                    invoker_channel_id,
                    target_mode,
                    "channel",
                );
                true
            }
        }
    }

    fn handle_server_chat_subcommand(
        &mut self,
        server_id: u32,
        invoker_clid: u64,
        invoker_channel_id: u32,
        target_mode: u32,
        arg_raw: &str,
    ) -> bool {
        let sub = arg_raw.trim().to_ascii_lowercase();
        match sub.as_str() {
            "" | "?" | "help" => {
                self.send_chat_submenu(
                    server_id,
                    invoker_clid,
                    invoker_channel_id,
                    target_mode,
                    "server",
                );
                true
            }
            "info" | "stats" | "status" => {
                let total_clients = self
                    .store
                    .online_clients
                    .values()
                    .filter(|c| c.server_id == server_id)
                    .count();
                let total_bots = self
                    .store
                    .music_bots
                    .values()
                    .filter(|b| b.server_id == server_id && b.linked_client_id.is_some())
                    .count();
                let total_channels = self
                    .store
                    .channels
                    .get(&server_id)
                    .map(|c| c.len())
                    .unwrap_or(1);
                let srv_name = self
                    .store
                    .virtual_servers
                    .get(&server_id)
                    .map(|s| s.name.as_str())
                    .unwrap_or("BlackTeaSpeak Server");
                let msg = format!(
                    "\n🖥️ [b]Server-Informationen ({}):[/b]\n• [b]Version:[/b] BlackTeaSpeak 1.5.6 (Rust Multi-Protocol Engine)\n• [b]Online-Clients:[/b] {} gesamt ({} Nutzer · {} MusikBots)\n• [b]Aktive Räume:[/b] {} Channels\n• [b]Kompatibilität:[/b] TeaSpeak Client · TeamSpeak 3 · BlackTeaWeb · BlackTeaSpeak Client",
                    srv_name,
                    total_clients,
                    total_clients.saturating_sub(total_bots),
                    total_bots,
                    total_channels
                );
                self.send_chat_interface_reply(
                    server_id,
                    invoker_clid,
                    invoker_channel_id,
                    target_mode,
                    None,
                    &msg,
                );
                true
            }
            "ports" | "network" => {
                let msg = concat!(
                    "\n🌐 [b]Aktive Server-Ports & Protokolle:[/b]\n",
                    "• [b]UDP/TCP 9987:[/b] TeaSpeak & TeamSpeak 3 Voice/Command Transport\n",
                    "• [b]UDP/TCP 9988:[/b] BlackTeaSpeak Desktop Transport\n",
                    "• [b]UDP/QUIC 9989:[/b] BlackTeaWeb WebTransport\n",
                    "• [b]HTTPS 8080 / HTTP 8081:[/b] Web-Client Oberfläche\n",
                    "• [b]TCP 30303:[/b] File-Transfer Server\n",
                    "• [b]TCP 10011 / 10022:[/b] ServerQuery (Telnet & SSH)"
                );
                self.send_chat_interface_reply(
                    server_id,
                    invoker_clid,
                    invoker_channel_id,
                    target_mode,
                    None,
                    msg,
                );
                true
            }
            _ => {
                self.send_chat_submenu(
                    server_id,
                    invoker_clid,
                    invoker_channel_id,
                    target_mode,
                    "server",
                );
                true
            }
        }
    }

    pub(crate) fn send_chat_interface_reply(
        &mut self,
        server_id: u32,
        invoker_clid: u64,
        invoker_channel_id: u32,
        target_mode: u32,
        bot_clid_opt: Option<u64>,
        text: &str,
    ) {
        let (sender_id, sender_name, sender_uid, sender_dbid) = if let Some(bot_clid) = bot_clid_opt
            && let Some(bot_client) = self.store.online_clients.get(&bot_clid)
        {
            (
                bot_clid,
                bot_client.nickname.clone(),
                bot_client.unique_identifier.clone(),
                bot_client.database_id,
            )
        } else {
            (
                0,
                String::from("BlackTeaSpeak System"),
                String::from("Server"),
                0,
            )
        };

        let reply_mode = if matches!(target_mode, 1 | 2 | 3) {
            target_mode
        } else {
            2
        };

        let target = TextMessageTarget {
            target_mode: reply_mode,
            server_id,
            channel_id: if reply_mode == 2 {
                Some(invoker_channel_id)
            } else {
                None
            },
            target_client_id: if reply_mode == 1 {
                Some(invoker_clid)
            } else {
                None
            },
            message: text.to_string(),
        };

        if reply_mode != 1 {
            self.record_text_message(
                &target,
                sender_dbid,
                sender_uid.clone(),
                sender_name.clone(),
            );
        }
        self.broadcast_event(
            server_id,
            &TransportNotification::TextMessage {
                target,
                invoker_id: sender_id,
                invoker_name: sender_name,
                invoker_uid: sender_uid,
            },
        );
    }

    fn build_chat_admin_query_session(
        &self,
        server_id: u32,
        invoker_clid: u64,
        invoker_channel_id: u32,
        invoker_dbid: u64,
    ) -> crate::runtime::QuerySessionState {
        crate::runtime::QuerySessionState {
            client_id: invoker_clid,
            connection_ip: String::from("127.0.0.1"),
            authenticated_login: Some(String::from("serveradmin")),
            selected_virtual_server_id: Some(server_id),
            current_channel_id: Some(invoker_channel_id),
            virtual_mode: false,
            notification_subscriptions: Vec::new(),
            actor_client_database_id_override: Some(invoker_dbid),
            client_nickname: self
                .store
                .online_clients
                .get(&invoker_clid)
                .map(|c| c.nickname.clone())
                .unwrap_or_default(),
            client_away: false,
            client_away_message: String::new(),
            client_input_muted: false,
            client_output_muted: false,
            is_desktop_client: true,
            whisper_targets: None,
            ignored_clients: Vec::new(),
            subscribed_channels: std::collections::BTreeSet::new(),
            points: 0,
            blocked_until_millis: 0,
            last_points_decay_millis: 0,
        }
    }

    fn execute_command_for_chat(
        &mut self,
        req: crate::query::CommandRequest,
        qsession: &mut crate::runtime::QuerySessionState,
    ) -> (crate::query::QueryResponse, Vec<TransportNotification>) {
        let before = qsession.clone();
        crate::transport::execute_request_with_notifications(self, &req, &before, qsession)
    }

    fn find_online_client_by_name_or_id(&self, server_id: u32, query: &str) -> Option<u64> {
        let trimmed = query.trim();
        if let Ok(clid) = trimmed.trim_start_matches('#').parse::<u64>() {
            if self
                .store
                .online_clients
                .get(&clid)
                .is_some_and(|c| c.server_id == server_id)
            {
                return Some(clid);
            }
        }
        let lower = trimmed.to_ascii_lowercase();
        self.store
            .online_clients
            .values()
            .find(|c| c.server_id == server_id && c.nickname.eq_ignore_ascii_case(trimmed))
            .or_else(|| {
                self.store.online_clients.values().find(|c| {
                    c.server_id == server_id && c.nickname.to_ascii_lowercase().contains(&lower)
                })
            })
            .map(|c| c.id)
    }

    fn find_channel_by_name_or_id(&self, server_id: u32, query: &str) -> Option<(u32, String)> {
        let trimmed = query.trim();
        let channels = self.store.channels.get(&server_id)?;
        if let Ok(cid) = trimmed.trim_start_matches('#').parse::<u32>() {
            if let Some(ch) = channels.iter().find(|c| c.id == cid) {
                return Some((ch.id, ch.name.clone()));
            }
        }
        let lower = trimmed.to_ascii_lowercase();
        channels
            .iter()
            .find(|c| c.name.eq_ignore_ascii_case(trimmed))
            .or_else(|| {
                channels
                    .iter()
                    .find(|c| c.name.to_ascii_lowercase().contains(&lower))
            })
            .map(|c| (c.id, c.name.clone()))
    }

    fn find_music_bot_in_channel(&self, server_id: u32, channel_id: u32) -> Option<u32> {
        self.store
            .music_bots
            .values()
            .find(|bot| {
                bot.server_id == server_id
                    && bot
                        .linked_client_id
                        .and_then(|clid| self.store.online_clients.get(&clid))
                        .is_some_and(|c| c.channel_id == channel_id)
            })
            .map(|bot| bot.id)
    }

    fn find_any_music_bot_on_server(&self, server_id: u32) -> Option<u32> {
        self.store
            .music_bots
            .values()
            .find(|bot| bot.server_id == server_id && bot.linked_client_id.is_some())
            .map(|bot| bot.id)
    }

    fn bot_client_id(&self, bot_id: u32) -> Option<u64> {
        self.store
            .music_bots
            .get(&bot_id)
            .and_then(|bot| bot.linked_client_id)
    }

    fn spawn_music_bot_in_channel(
        &mut self,
        server_id: u32,
        channel_id: u32,
        custom_name: Option<String>,
    ) -> u32 {
        let bot_id = self
            .store
            .music_bots
            .keys()
            .copied()
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        let client_id = self.allocate_query_client_id();
        let client_database_id = self.allocate_client_database_id();
        let nickname = custom_name
            .filter(|n| !n.trim().is_empty())
            .unwrap_or_else(|| format!("MusicBot {bot_id}"));

        let music_bot = MusicBot {
            id: bot_id,
            server_id,
            client_database_id,
            linked_client_id: Some(client_id),
            nickname: nickname.clone(),
            channel_id,
            playlist_id: bot_id,
            current_song_id: None,
            next_song_id: 1,
            state: MusicBotState::Stopped,
            player_volume: String::from("100"),
            playlist_title: String::new(),
            playlist_description: String::new(),
            playlist_flag_delete_played: false,
            playlist_flag_finished: false,
            playlist_replay_mode: 0,
            playlist_max_songs: 0,
            permissions: BTreeMap::new(),
            client_permissions: Vec::new(),
            current_song_started_at_millis: None,
            current_song_progress_millis: 0,
            queue: Vec::new(),
        };

        self.store.online_clients.insert(
            client_id,
            OnlineClient {
                id: client_id,
                database_id: client_database_id,
                unique_identifier: format!("compat-musicbot-{client_database_id}"),
                nickname: nickname.clone(),
                away: false,
                away_message: String::new(),
                input_muted: false,
                output_muted: false,
                server_id,
                channel_id,
                client_type: 0,
                version: String::from("BlackTeaSpeak 1.5.6 compat-musicbot"),
                platform: String::from("compat-rust"),
                country: String::from("ZZ"),
                connection_ip: String::new(),
                server_groups: vec![8],
                connected_at: current_unix_timestamp(),
                last_seen_at: current_unix_timestamp(),
                whisper_targets: None,
                extra_properties: Self::music_bot_extra_properties(&music_bot),
            },
        );
        self.store.music_bots.insert(bot_id, music_bot);

        self.broadcast_event(
            server_id,
            &TransportNotification::ClientEnterView {
                presence: SessionPresence {
                    client_id,
                    login_name: nickname,
                    unique_identifier: format!("compat-musicbot-{client_database_id}"),
                    client_type: 4,
                    server_id,
                    channel_id,
                },
                from_channel_id: None,
                reason_id: 0,
            },
        );

        bot_id
    }

    fn resolve_chat_url_or_local_file(&self, raw: &str) -> String {
        let trimmed = raw.trim();
        let lower = trimmed.to_ascii_lowercase();
        if lower.starts_with("http://")
            || lower.starts_with("https://")
            || lower.starts_with("channel://")
            || lower.starts_with("local://")
            || lower.starts_with("/music/")
        {
            return trimmed.to_string();
        }
        let local_files = self.discover_local_music_files();
        if let Ok(num) = trimmed.trim_start_matches('#').parse::<usize>() {
            if num >= 1 && num <= local_files.len() {
                return local_files[num - 1].1.clone();
            }
        }
        if let Some((_, file_url, _, _)) = local_files
            .iter()
            .find(|(name, _, _, _)| name.eq_ignore_ascii_case(trimmed))
            .or_else(|| {
                local_files
                    .iter()
                    .find(|(name, _, _, _)| name.to_ascii_lowercase().contains(&lower))
            })
        {
            return file_url.clone();
        }
        trimmed.to_string()
    }

    fn enqueue_and_play_bot_url(
        &mut self,
        bot_id: u32,
        server_id: u32,
        bot_clid: u64,
        invoker_dbid: u64,
        url: &str,
        play_immediately: bool,
    ) {
        let resolved_url = self.resolve_chat_url_or_local_file(url);
        let Some(song_id) = self
            .store
            .music_bots
            .get(&bot_id)
            .map(|b| b.next_song_id.max(1))
        else {
            return;
        };

        let entry =
            self.build_music_queue_entry(song_id, &resolved_url, Some("any"), invoker_dbid);
        let entry_title = entry.title.clone();

        let mut will_play_now = play_immediately;
        let mut queue_pos = 1usize;
        if let Some(bot) = self.store.music_bots.get_mut(&bot_id) {
            bot.next_song_id = song_id.saturating_add(1);
            let prev_id = bot.queue.last().map(|s| s.id).unwrap_or(0);
            let _ = Self::insert_music_queue_entry(bot, entry, prev_id);
            queue_pos = bot.queue.len();
            if play_immediately || bot.current_song_id == Some(song_id) {
                will_play_now = true;
                bot.current_song_id = Some(song_id);
                bot.state = MusicBotState::Playing;
                bot.current_song_started_at_millis = None;
                bot.current_song_progress_millis = 0;
            }
        }
        self.sync_music_bot_client_state(bot_id);

        if !will_play_now {
            send_bot_channel_message_locked(
                self,
                server_id,
                bot_clid,
                &format!(
                    "➕ Zur Warteschlange hinzugefügt (Position #{}): [b]{}[/b]",
                    queue_pos, entry_title
                ),
            );
        }
    }

    fn step_bot_track(&mut self, bot_id: u32, forward: bool) -> Option<String> {
        let mut next_title = None;
        if let Some(bot) = self.store.music_bots.get_mut(&bot_id) {
            let next_song_id = if forward {
                bot.current_song_id.and_then(|cur| {
                    bot.queue
                        .iter()
                        .position(|s| s.id == cur)
                        .and_then(|idx| bot.queue.get(idx + 1))
                        .map(|s| s.id)
                })
            } else {
                bot.current_song_id
                    .and_then(|cur| {
                        bot.queue
                            .iter()
                            .position(|s| s.id == cur)
                            .and_then(|idx| idx.checked_sub(1))
                            .and_then(|idx| bot.queue.get(idx))
                            .map(|s| s.id)
                    })
                    .or_else(|| bot.queue.first().map(|s| s.id))
            };
            bot.current_song_id = next_song_id;
            if let Some(sid) = next_song_id {
                bot.state = MusicBotState::Playing;
                Self::reset_music_bot_playback_progress(bot);
                next_title = bot
                    .queue
                    .iter()
                    .find(|s| s.id == sid)
                    .map(|s| s.title.clone());
            } else {
                bot.state = MusicBotState::Stopped;
                Self::stop_music_bot_playback(bot);
            }
        }
        self.sync_music_bot_client_state(bot_id);
        next_title
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::{QuerySessionState, create_baseline_runtime_with_state_path};
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn create_test_runtime(label: &str) -> BaselineRuntime {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let state_path = std::env::temp_dir()
            .join(format!("bts-musicbot-chat-{label}-{}-{ts}", std::process::id()))
            .join("runtime-state.json");
        let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        create_baseline_runtime_with_state_path(workspace_root, &state_path).unwrap()
    }

    #[test]
    fn clean_chat_url_strips_ts3_bbcode_tags() {
        assert_eq!(
            clean_chat_url("[URL]https://www.youtube.com/watch?v=dQw4w9WgXcQ[/URL]"),
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ"
        );
        assert_eq!(
            clean_chat_url("[url=https://youtu.be/dQw4w9WgXcQ]YouTube Video[/url]"),
            "https://youtu.be/dQw4w9WgXcQ"
        );
        assert_eq!(
            clean_chat_url("  <https://stream.example.com/live.mp3> "),
            "https://stream.example.com/live.mp3"
        );
    }

    #[test]
    fn ts3_channel_chat_commands_control_musicbot_in_same_room() {
        let mut rt = create_test_runtime("same-room-control");
        assert!(rt.try_handle_musicbot_chat_command(1, 10, 2, None, "!join"));
        let bot_clid = rt.store.music_bots.get(&1).unwrap().linked_client_id.unwrap();
        assert_eq!(rt.store.online_clients.get(&bot_clid).unwrap().channel_id, 1);

        assert!(rt.try_handle_musicbot_chat_command(
            1,
            10,
            2,
            None,
            "/play [URL]https://www.youtube.com/watch?v=aqz-KE-bpKQ[/URL]"
        ));
        {
            let bot = rt.store.music_bots.get(&1).unwrap();
            assert!(matches!(bot.state, MusicBotState::Playing));
            assert_eq!(bot.queue.len(), 1);
            assert_eq!(bot.queue[0].url, "https://www.youtube.com/watch?v=aqz-KE-bpKQ");
            assert_eq!(bot.queue[0].url_loader, "youtube");
        }

        assert!(rt.try_handle_musicbot_chat_command(1, 10, 2, None, "/vol 65"));
        assert_eq!(rt.store.music_bots.get(&1).unwrap().player_volume, "0.65");
        assert_eq!(
            BaselineRuntime::format_music_bot_volume_percent(
                &rt.store.music_bots.get(&1).unwrap().player_volume
            ),
            65
        );

        assert!(rt.try_handle_musicbot_chat_command(1, 10, 2, None, "/loop all"));
        assert_eq!(rt.store.music_bots.get(&1).unwrap().playlist_replay_mode, 1);

        assert!(rt.try_handle_musicbot_chat_command(1, 10, 2, None, "/pause"));
        assert!(matches!(
            rt.store.music_bots.get(&1).unwrap().state,
            MusicBotState::Paused
        ));
        assert!(rt.try_handle_musicbot_chat_command(1, 10, 2, None, "/resume"));
        assert!(matches!(
            rt.store.music_bots.get(&1).unwrap().state,
            MusicBotState::Playing
        ));

        assert!(rt.try_handle_musicbot_chat_command(1, 10, 2, None, "/stop"));
        assert!(matches!(
            rt.store.music_bots.get(&1).unwrap().state,
            MusicBotState::Stopped
        ));
    }

    #[test]
    fn slash_question_mark_and_submenus_work() {
        let mut rt = create_test_runtime("slash-help-submenus");
        assert!(rt.try_handle_musicbot_chat_command(1, 10, 3, None, "/?"));
        assert!(rt.try_handle_musicbot_chat_command(1, 10, 3, None, "/? bot"));
        assert!(rt.try_handle_musicbot_chat_command(1, 10, 3, None, "/? queue"));
        assert!(rt.try_handle_musicbot_chat_command(1, 10, 3, None, "/? files"));
        assert!(rt.try_handle_musicbot_chat_command(1, 10, 3, None, "/? channel"));
        assert!(rt.try_handle_musicbot_chat_command(1, 10, 3, None, "/? user"));
        assert!(rt.try_handle_musicbot_chat_command(1, 10, 3, None, "/? admin"));
        assert!(rt.try_handle_musicbot_chat_command(1, 10, 3, None, "/? server"));
        assert!(rt.try_handle_musicbot_chat_command(1, 10, 2, None, "/bot ?"));
        assert!(rt.try_handle_musicbot_chat_command(1, 10, 2, None, "/whoami"));
    }

    #[test]
    fn teaspeak_gui_musicbot_player_and_playlist_controls_work() {
        let mut rt = create_test_runtime("teaspeak-gui-controls");
        let mut session = QuerySessionState {
            selected_virtual_server_id: Some(1),
            current_channel_id: Some(1),
            actor_client_database_id_override: Some(1),
            client_id: 10,
            ..QuerySessionState::default()
        };

        let create_resp = rt.execute("musicbotcreate cid=1", &mut session);
        assert!(create_resp.contains("id=0 msg=ok"), "musicbotcreate failed: {create_resp}");

        let bot_id = 1_u32;
        let bot_dbid = rt.store.music_bots.get(&bot_id).unwrap().client_database_id;
        let bot_clid = rt.store.music_bots.get(&bot_id).unwrap().linked_client_id.unwrap();
        let playlist_id = rt.store.music_bots.get(&bot_id).unwrap().playlist_id;

        // Add two songs via playlistsongadd without previous (defaults to 0 = append to end in TeaSpeak)
        let add1 = rt.execute(
            &format!("playlistsongadd playlist_id={playlist_id} url=https://example.com/song1.mp3"),
            &mut session,
        );
        assert!(add1.contains("song_id=1"), "playlistsongadd 1 failed: {add1}");
        let add2 = rt.execute(
            &format!("playlistsongadd playlist_id={playlist_id} url=https://example.com/song2.mp3"),
            &mut session,
        );
        assert!(add2.contains("song_id=2"), "playlistsongadd 2 failed: {add2}");

        {
            let bot = rt.store.music_bots.get_mut(&bot_id).unwrap();
            assert_eq!(bot.queue.len(), 2);
            assert_eq!(bot.queue[0].id, 1);
            assert_eq!(bot.queue[1].id, 2);
            assert_eq!(bot.queue[1].previous_song_id, 1);
            bot.queue[0].length_seconds = 222;
        }

        // Start playback via musicbotplayeraction action=1 (using bot_id = client_database_id as TeaSpeak GUI sends)
        let play_resp = rt.execute(
            &format!("musicbotplayeraction bot_id={bot_dbid} action=1"),
            &mut session,
        );
        assert!(play_resp.contains("id=0 msg=ok"), "play failed: {play_resp}");
        assert_eq!(rt.store.music_bots.get(&bot_id).unwrap().current_song_id, Some(1));
        assert!(matches!(rt.store.music_bots.get(&bot_id).unwrap().state, MusicBotState::Playing));

        // Next song (action=3) and wrap-around
        let next_resp = rt.execute(
            &format!("musicbotplayeraction bot_id={bot_dbid} action=3"),
            &mut session,
        );
        assert!(next_resp.contains("id=0 msg=ok"), "next failed: {next_resp}");
        assert_eq!(rt.store.music_bots.get(&bot_id).unwrap().current_song_id, Some(2));

        let next_wrap = rt.execute(
            &format!("musicbotplayeraction bot_id={bot_dbid} action=3"),
            &mut session,
        );
        assert!(next_wrap.contains("id=0 msg=ok"), "next wrap failed: {next_wrap}");
        assert_eq!(rt.store.music_bots.get(&bot_id).unwrap().current_song_id, Some(1));

        // Seek via TeaSpeak GUI timeline (action=5 units=45000)
        let seek_resp = rt.execute(
            &format!("musicbotplayeraction bot_id={bot_dbid} action=5 units=45000"),
            &mut session,
        );
        assert!(seek_resp.contains("id=0 msg=ok"), "seek failed: {seek_resp}");
        assert!(rt.store.music_bots.get(&bot_id).unwrap().current_song_progress_millis >= 45_000);

        // Volume slider via clientedit clid=... player_volume=0.35
        let vol_resp = rt.execute(
            &format!("clientedit clid={bot_clid} player_volume=0.35"),
            &mut session,
        );
        assert!(vol_resp.contains("id=0 msg=ok"), "clientedit volume failed: {vol_resp}");
        assert_eq!(rt.store.music_bots.get(&bot_id).unwrap().player_volume, "0.35");

        // Verify music_bot_notify_payload has song_length in ms and metadata JSON with type/url/length
        let payload = rt.music_bot_notify_payload_by_identifier(1, bot_dbid).unwrap();
        assert_eq!(payload.player_info.get("player_volume").map(String::as_str), Some("0.35"));
        assert_eq!(payload.player_info.get("bot_volume").map(String::as_str), Some("35"));
        assert_eq!(payload.player_info.get("song_length").map(String::as_str), Some("222000"));
        let meta = payload.loaded_songs[0].get("song_metadata").unwrap();
        assert!(meta.contains("\"length\":222000"), "unexpected metadata: {meta}");
        assert!(meta.contains("\"url\":\"https://example.com/song1.mp3\""), "unexpected metadata: {meta}");
    }
}
