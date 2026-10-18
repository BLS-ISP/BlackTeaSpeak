use std::collections::BTreeMap;
use super::*;

mod queue_and_loader;
mod bot_commands;
mod playlist_commands;
pub mod audio_engine;
pub mod chat_commands;
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MusicBotNotifyPayload {
    pub client_id: u64,
    pub playlist_id: u32,
    pub client_updates: BTreeMap<String, String>,
    pub song_change: BTreeMap<String, String>,
    pub status_update: BTreeMap<String, String>,
    pub player_info: BTreeMap<String, String>,
    pub loaded_songs: Vec<BTreeMap<String, String>>,
}

#[derive(Debug, Clone)]
pub(crate) struct MusicQueueEntry {
    pub(crate) id: u32,
    pub(crate) previous_song_id: u32,
    pub(crate) url: String,
    pub(crate) url_loader: String,
    pub(crate) invoker_database_id: u64,
    pub(crate) loaded: bool,
    pub(crate) metadata: String,
    pub(crate) title: String,
    pub(crate) description: String,
    pub(crate) thumbnail: String,
    pub(crate) length_seconds: u32,
    pub(crate) seekable: bool,
    pub(crate) live_stream: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct PlaylistClientPermissionTarget {
    pub(crate) client_database_id: u64,
    pub(crate) permissions: BTreeMap<String, PermissionAssignment>,
}

#[derive(Debug, Clone)]
pub(crate) struct ResolvedMusicMetadata {
    pub(crate) loaded: bool,
    pub(crate) title: String,
    pub(crate) description: String,
    pub(crate) thumbnail: String,
    pub(crate) length_seconds: u32,
    pub(crate) seekable: bool,
    pub(crate) live_stream: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct MusicBot {
    pub(crate) id: u32,
    pub(crate) server_id: u32,
    pub(crate) client_database_id: u64,
    pub(crate) linked_client_id: Option<u64>,
    pub(crate) nickname: String,
    pub(crate) channel_id: u32,
    pub(crate) playlist_id: u32,
    pub(crate) current_song_id: Option<u32>,
    pub(crate) next_song_id: u32,
    pub(crate) state: MusicBotState,
    pub(crate) player_volume: String,
    pub(crate) playlist_title: String,
    pub(crate) playlist_description: String,
    pub(crate) playlist_flag_delete_played: bool,
    pub(crate) playlist_flag_finished: bool,
    pub(crate) playlist_replay_mode: u32,
    pub(crate) playlist_max_songs: u32,
    pub(crate) permissions: BTreeMap<String, PermissionAssignment>,
    pub(crate) client_permissions: Vec<PlaylistClientPermissionTarget>,
    pub(crate) current_song_started_at_millis: Option<u64>,
    pub(crate) current_song_progress_millis: u32,
    pub(crate) queue: Vec<MusicQueueEntry>,
}

#[derive(Debug, Clone)]
pub(crate) enum MusicBotState {
    Stopped,
    Playing,
    Paused,
}

impl MusicBotState {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Self::Stopped => "stopped",
            Self::Playing => "playing",
            Self::Paused => "paused",
        }
    }

    pub(crate) fn as_player_state(&self) -> &'static str {
        match self {
            Self::Stopped => "4",
            Self::Playing => "2",
            Self::Paused => "3",
        }
    }
}

impl From<PersistedMusicBotState> for MusicBotState {
    fn from(value: PersistedMusicBotState) -> Self {
        match value {
            PersistedMusicBotState::Stopped => Self::Stopped,
            PersistedMusicBotState::Playing => Self::Playing,
            PersistedMusicBotState::Paused => Self::Paused,
        }
    }
}

impl From<MusicBotState> for PersistedMusicBotState {
    fn from(value: MusicBotState) -> Self {
        match value {
            MusicBotState::Stopped => Self::Stopped,
            MusicBotState::Playing => Self::Playing,
            MusicBotState::Paused => Self::Paused,
        }
    }
}

impl BaselineRuntime {
    pub(crate) fn parse_music_bot_volume_factor(raw: &str) -> f32 {
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed == "1" || trimmed == "1.0" || trimmed == "100" {
            return 1.0;
        }
        if let Ok(val) = trimmed.parse::<f32>() {
            if !val.is_finite() {
                return 1.0;
            }
            if trimmed.contains('.') && val <= 2.0 {
                val.clamp(0.0, 2.0)
            } else if val <= 1.0 {
                val.clamp(0.0, 2.0)
            } else {
                (val / 100.0).clamp(0.0, 2.0)
            }
        } else {
            1.0
        }
    }

    pub(crate) fn format_music_bot_volume_factor(factor: f32) -> String {
        let clamped = factor.clamp(0.0, 2.0);
        if (clamped - 1.0).abs() < 0.001 {
            String::from("1")
        } else if clamped <= 0.0001 {
            String::from("0")
        } else {
            let formatted = format!("{clamped:.2}");
            let trimmed = formatted.trim_end_matches('0').trim_end_matches('.');
            if trimmed.is_empty() {
                String::from("0")
            } else {
                trimmed.to_string()
            }
        }
    }

    pub(crate) fn format_music_bot_volume_percent(raw: &str) -> u32 {
        (Self::parse_music_bot_volume_factor(raw) * 100.0)
            .round()
            .clamp(0.0, 200.0) as u32
    }

    pub fn music_bot_client_snapshot_by_identifier(
        &self,
        server_id: u32,
        bot_identifier: u64,
    ) -> Option<OnlineClientSnapshot> {
        let client_id = self
            .store
            .music_bots
            .values()
            .find(|bot| {
                bot.server_id == server_id
                    && (u64::from(bot.id) == bot_identifier
                        || bot.client_database_id == bot_identifier
                        || bot.linked_client_id == Some(bot_identifier))
            })
            .and_then(|bot| bot.linked_client_id)?;

        self.online_client_snapshot(server_id, client_id)
    }

    pub(crate) fn music_bot_extra_properties(bot: &MusicBot) -> BTreeMap<String, String> {
        let current_song = Self::music_bot_current_song(bot);
        let cur_title = current_song.map(|s| s.title.as_str()).unwrap_or("");
        let cur_len = current_song.map(|s| s.length_seconds).unwrap_or(0);
        let cur_loaded = u8::from(current_song.is_some_and(|s| {
            s.loaded && (s.live_stream || s.url_loader != "youtube" || s.length_seconds > 0)
        }));
        let song_state = if matches!(bot.state, MusicBotState::Stopped) {
            String::from("stopped")
        } else {
            format!(
                "{}:{}:{}:{}",
                bot.current_song_id.unwrap_or(0),
                cur_loaded,
                cur_len,
                cur_title
            )
        };
        let loaded_summary = bot
            .queue
            .iter()
            .filter(|s| s.loaded)
            .map(|s| format!("{}={}", s.id, Self::format_song_metadata_json(s)))
            .collect::<Vec<_>>()
            .join("\n");
        let sync_state = format!(
            "{}:{}:{}:{}:{}",
            bot.state.as_player_state(),
            bot.current_song_id.unwrap_or(0),
            bot.queue.len(),
            bot.current_song_progress_millis,
            bot.current_song_started_at_millis.unwrap_or(0),
        );
        let vol_factor_str = Self::format_music_bot_volume_factor(
            Self::parse_music_bot_volume_factor(&bot.player_volume),
        );

        let mut row = BTreeMap::new();
        row.insert(String::from("client_type_exact"), String::from("4"));
        row.insert(
            String::from("player_state"),
            bot.state.as_player_state().to_string(),
        );
        row.insert(String::from("player_volume"), vol_factor_str);
        row.insert(
            String::from("client_playlist_id"),
            bot.playlist_id.to_string(),
        );
        row.insert(String::from("client_disabled"), String::from("0"));
        row.insert(
            String::from("client_flag_notify_song_change"),
            String::from("0"),
        );
        row.insert(String::from("client_bot_type"), String::from("0"));
        row.insert(String::from("client_uptime_mode"), String::from("0"));
        row.insert(String::from("_music_song_state"), song_state);
        row.insert(String::from("_music_sync_state"), sync_state);
        row.insert(String::from("_music_loaded_songs"), loaded_summary);
        row
    }

    pub(crate) fn music_bot_current_song(bot: &MusicBot) -> Option<&MusicQueueEntry> {
        bot.current_song_id
            .and_then(|song_id| bot.queue.iter().find(|entry| entry.id == song_id))
    }

    pub(crate) fn music_bot_queue_position(bot: &MusicBot) -> u32 {
        bot.current_song_id
            .and_then(|song_id| bot.queue.iter().position(|entry| entry.id == song_id))
            .map(|index| index as u32 + 1)
            .unwrap_or(0)
    }

    pub(crate) fn music_bot_client_name(&self, bot: &MusicBot) -> String {
        bot.linked_client_id
            .and_then(|client_id| self.store.online_clients.get(&client_id))
            .map(|client| client.nickname.clone())
            .or_else(|| (!bot.nickname.trim().is_empty()).then(|| bot.nickname.clone()))
            .unwrap_or_else(|| format!("MusicBot {}", bot.id))
    }

    pub(crate) fn music_bot_playlist_title(&self, bot: &MusicBot) -> String {
        let title = bot.playlist_title.trim();
        if title.is_empty() {
            format!("{} Queue", self.music_bot_client_name(bot))
        } else {
            bot.playlist_title.clone()
        }
    }

    pub(crate) fn music_bot_playlist_description(&self, bot: &MusicBot) -> String {
        let description = bot.playlist_description.trim();
        if description.is_empty() {
            self.music_bot_client_name(bot)
        } else {
            bot.playlist_description.clone()
        }
    }

    pub(crate) fn music_bot_playlist_power(bot: &MusicBot, names: &[&str]) -> i64 {
        permission_value_or_default(&bot.permissions, names)
    }

    pub(crate) fn music_bot_current_song_max_millis(bot: &MusicBot) -> u32 {
        Self::music_bot_current_song(bot)
            .map(|song| song.length_seconds.saturating_mul(1000))
            .unwrap_or(0)
    }

    pub(crate) fn music_bot_current_song_replay_millis(bot: &MusicBot) -> u32 {
        let mut replay_millis = bot.current_song_progress_millis;
        if matches!(bot.state, MusicBotState::Playing)
            && let Some(started_at_millis) = bot.current_song_started_at_millis
        {
            let delta = current_unix_timestamp_millis().saturating_sub(started_at_millis);
            replay_millis = replay_millis.saturating_add(delta.min(u64::from(u32::MAX)) as u32);
        }

        let max_millis = Self::music_bot_current_song_max_millis(bot);
        if max_millis > 0 && Self::music_bot_current_song(bot).is_some_and(|song| !song.live_stream)
        {
            replay_millis.min(max_millis)
        } else {
            replay_millis
        }
    }

    pub(crate) fn music_bot_current_song_buffered_millis(bot: &MusicBot) -> u32 {
        let replay_millis = Self::music_bot_current_song_replay_millis(bot);
        let max_millis = Self::music_bot_current_song_max_millis(bot);
        if max_millis == 0 {
            replay_millis
        } else {
            replay_millis.saturating_add(15_000).min(max_millis)
        }
    }

    pub(crate) fn reset_music_bot_playback_progress(bot: &mut MusicBot) {
        bot.current_song_progress_millis = 0;
        bot.current_song_started_at_millis = if matches!(bot.state, MusicBotState::Playing)
            && bot.current_song_id.is_some()
        {
            Some(current_unix_timestamp_millis())
        } else {
            None
        };
    }

    pub(crate) fn pause_music_bot_playback(bot: &mut MusicBot) {
        bot.current_song_progress_millis = Self::music_bot_current_song_replay_millis(bot);
        bot.current_song_started_at_millis = None;
    }

    pub(crate) fn resume_music_bot_playback(bot: &mut MusicBot) {
        bot.current_song_started_at_millis = if bot.current_song_id.is_some() {
            Some(current_unix_timestamp_millis())
        } else {
            None
        };
    }

    pub(crate) fn stop_music_bot_playback(bot: &mut MusicBot) {
        bot.current_song_progress_millis = 0;
        bot.current_song_started_at_millis = None;
    }

    pub(crate) fn normalize_music_bot_queue(bot: &mut MusicBot) {
        if bot.playlist_id == 0 {
            bot.playlist_id = bot.id;
        }

        bot.client_permissions
            .retain(|target| !target.permissions.is_empty());
        bot.client_permissions
            .sort_by_key(|target| target.client_database_id);

        let mut previous_song_id = 0;
        for entry in &mut bot.queue {
            entry.previous_song_id = previous_song_id;
            previous_song_id = entry.id;
        }

        let previous_current_song_id = bot.current_song_id;
        if bot
            .current_song_id
            .is_some_and(|song_id| !bot.queue.iter().any(|entry| entry.id == song_id))
        {
            bot.current_song_id = None;
        }
        if bot.current_song_id.is_none() {
            bot.current_song_id = bot.queue.first().map(|entry| entry.id);
        }

        if bot.current_song_id != previous_current_song_id {
            if bot.current_song_id.is_some() {
                Self::reset_music_bot_playback_progress(bot);
            } else {
                Self::stop_music_bot_playback(bot);
            }
        } else if bot.current_song_id.is_none() {
            Self::stop_music_bot_playback(bot);
        }

        let next_seed = bot
            .queue
            .iter()
            .map(|entry| entry.id)
            .max()
            .unwrap_or(0)
            .saturating_add(1)
            .max(1);
        bot.next_song_id = bot.next_song_id.max(next_seed);
    }

    pub(crate) fn format_song_metadata_json(entry: &MusicQueueEntry) -> String {
        serde_json::json!({
            "type": if entry.live_stream { 1u32 } else { 0u32 },
            "url": entry.url,
            "title": entry.title,
            "description": entry.description,
            "thumbnail": entry.thumbnail,
            "length": entry.length_seconds.saturating_mul(1000),
            "is_live": entry.live_stream,
            "seekable": entry.seekable,
        })
        .to_string()
    }

    pub(crate) fn append_music_song_fields(row: &mut BTreeMap<String, String>, song: Option<&MusicQueueEntry>) {
        let song_id = song.map(|entry| entry.id).unwrap_or(0);
        let song_url = song.map(|entry| entry.url.clone()).unwrap_or_default();
        let song_invoker = song
            .map(|entry| entry.invoker_database_id)
            .unwrap_or_default();
        let song_loaded = song.is_some_and(|entry| entry.loaded);
        let song_title = song.map(|entry| entry.title.clone()).unwrap_or_default();
        let song_description = song
            .map(|entry| entry.description.clone())
            .unwrap_or_default();
        let song_thumbnail = song.map(|entry| entry.thumbnail.clone()).unwrap_or_default();
        let song_length_ms = song
            .map(|entry| entry.length_seconds.saturating_mul(1000))
            .unwrap_or(0);

        row.insert(String::from("song_id"), song_id.to_string());
        row.insert(String::from("song_url"), song_url);
        row.insert(String::from("song_invoker"), song_invoker.to_string());
        row.insert(
            String::from("song_loaded"),
            if song_loaded {
                String::from("1")
            } else {
                String::from("0")
            },
        );
        row.insert(String::from("song_title"), song_title);
        row.insert(String::from("song_description"), song_description);
        row.insert(String::from("song_thumbnail"), song_thumbnail);
        row.insert(String::from("song_length"), song_length_ms.to_string());
    }

    pub(crate) fn music_queue_entry_row(playlist_id: u32, entry: &MusicQueueEntry) -> BTreeMap<String, String> {
        let mut row = BTreeMap::new();
        row.insert(String::from("playlist_id"), playlist_id.to_string());
        row.insert(String::from("song_id"), entry.id.to_string());
        row.insert(
            String::from("song_previous_song_id"),
            entry.previous_song_id.to_string(),
        );
        row.insert(
            String::from("song_invoker"),
            entry.invoker_database_id.to_string(),
        );
        row.insert(String::from("song_url"), entry.url.clone());
        row.insert(String::from("song_url_loader"), entry.url_loader.clone());
        row.insert(
            String::from("song_loaded"),
            if entry.loaded {
                String::from("1")
            } else {
                String::from("0")
            },
        );
        row.insert(
            String::from("song_metadata"),
            Self::format_song_metadata_json(entry),
        );
        row
    }

    pub(crate) fn music_bot_playerinfo_row(bot: &MusicBot) -> BTreeMap<String, String> {
        let current_song = Self::music_bot_current_song(bot);
        let replay_millis = Self::music_bot_current_song_replay_millis(bot);
        let buffered_millis = Self::music_bot_current_song_buffered_millis(bot);
        let max_millis = Self::music_bot_current_song_max_millis(bot);
        let vol_factor_str = Self::format_music_bot_volume_factor(
            Self::parse_music_bot_volume_factor(&bot.player_volume),
        );
        let vol_percent_str = Self::format_music_bot_volume_percent(&bot.player_volume).to_string();
        let mut row = BTreeMap::new();
        row.insert(String::from("bot_id"), bot.client_database_id.to_string());
        row.insert(String::from("botid"), bot.id.to_string());
        row.insert(
            String::from("player_state"),
            bot.state.as_player_state().to_string(),
        );
        row.insert(String::from("player_volume"), vol_factor_str);
        row.insert(String::from("bot_volume"), vol_percent_str);
        Self::append_music_song_fields(&mut row, current_song);
        row.insert(String::from("player_buffered_index"), buffered_millis.to_string());
        row.insert(String::from("player_replay_index"), replay_millis.to_string());
        row.insert(String::from("player_max_index"), max_millis.to_string());
        row.insert(
            String::from("player_seekable"),
            if current_song.is_some_and(|song| song.seekable) {
                String::from("1")
            } else {
                String::from("0")
            },
        );
        row.insert(
            String::from("player_title"),
            current_song.map(|song| song.title.clone()).unwrap_or_default(),
        );
        row.insert(
            String::from("player_description"),
            current_song
                .map(|song| song.description.clone())
                .unwrap_or_default(),
        );
        if current_song.is_some_and(|song| song.title.starts_with("⏳")) {
            row.insert(String::from("song_loaded"), String::from("0"));
        }
        row
    }

    pub(crate) fn music_bot_song_change_row(bot: &MusicBot) -> BTreeMap<String, String> {
        let current_song = Self::music_bot_current_song(bot);
        let mut row = BTreeMap::new();
        row.insert(String::from("bot_id"), bot.client_database_id.to_string());
        row.insert(String::from("botid"), bot.id.to_string());
        Self::append_music_song_fields(&mut row, current_song);
        row.insert(String::from("song_loaded"), String::from("0"));
        row
    }

    pub(crate) fn music_bot_status_row(bot: &MusicBot) -> BTreeMap<String, String> {
        let mut row = BTreeMap::new();
        row.insert(String::from("bot_id"), bot.client_database_id.to_string());
        row.insert(String::from("botid"), bot.id.to_string());
        row.insert(
            String::from("player_replay_index"),
            Self::music_bot_current_song_replay_millis(bot).to_string(),
        );
        row.insert(
            String::from("player_buffered_index"),
            Self::music_bot_current_song_buffered_millis(bot).to_string(),
        );
        row
    }

    pub(crate) fn music_bot_playlist_list_row(&self, bot: &MusicBot) -> BTreeMap<String, String> {
        let bot_name = self.music_bot_client_name(bot);

        let mut row = BTreeMap::new();
        row.insert(String::from("playlist_id"), bot.playlist_id.to_string());
        row.insert(
            String::from("playlist_bot_id"),
            if bot.linked_client_id.is_some() {
                bot.client_database_id.to_string()
            } else {
                String::from("0")
            },
        );
        row.insert(String::from("playlist_title"), self.music_bot_playlist_title(bot));
        row.insert(String::from("playlist_type"), String::from("1"));
        row.insert(
            String::from("playlist_owner_dbid"),
            bot.client_database_id.to_string(),
        );
        row.insert(String::from("playlist_owner_name"), bot_name);
        row.insert(
            String::from("needed_power_modify"),
            Self::music_bot_playlist_power(bot, &["i_playlist_needed_modify_power"]).to_string(),
        );
        row.insert(
            String::from("needed_power_permission_modify"),
            Self::music_bot_playlist_power(bot, &["i_playlist_needed_permission_modify_power"])
                .to_string(),
        );
        row.insert(
            String::from("needed_power_delete"),
            Self::music_bot_playlist_power(bot, &["i_playlist_needed_delete_power"]).to_string(),
        );
        row.insert(
            String::from("needed_power_song_add"),
            Self::music_bot_playlist_power(bot, &["i_playlist_song_needed_add_power"]).to_string(),
        );
        row.insert(
            String::from("needed_power_song_move"),
            Self::music_bot_playlist_power(bot, &["i_playlist_song_needed_move_power"]).to_string(),
        );
        row.insert(
            String::from("needed_power_song_remove"),
            Self::music_bot_playlist_power(bot, &["i_playlist_song_needed_remove_power"])
                .to_string(),
        );
        row
    }

    pub(crate) fn music_bot_playlist_info_row(&self, bot: &MusicBot) -> BTreeMap<String, String> {
        let bot_name = self.music_bot_client_name(bot);

        let mut row = BTreeMap::new();
        row.insert(String::from("playlist_id"), bot.playlist_id.to_string());
        row.insert(String::from("playlist_title"), self.music_bot_playlist_title(bot));
        row.insert(
            String::from("playlist_description"),
            self.music_bot_playlist_description(bot),
        );
        row.insert(String::from("playlist_type"), String::from("1"));
        row.insert(
            String::from("playlist_owner_dbid"),
            bot.client_database_id.to_string(),
        );
        row.insert(String::from("playlist_owner_name"), bot_name);
        row.insert(
            String::from("playlist_flag_delete_played"),
            if bot.playlist_flag_delete_played {
                String::from("1")
            } else {
                String::from("0")
            },
        );
        row.insert(
            String::from("playlist_flag_finished"),
            if bot.playlist_flag_finished {
                String::from("1")
            } else {
                String::from("0")
            },
        );
        row.insert(
            String::from("playlist_replay_mode"),
            bot.playlist_replay_mode.to_string(),
        );
        row.insert(
            String::from("playlist_current_song_id"),
            bot.current_song_id.unwrap_or(0).to_string(),
        );
        row.insert(
            String::from("playlist_max_songs"),
            bot.playlist_max_songs.to_string(),
        );
        row
    }

    pub(crate) fn music_bot_id_by_playlist_id(&self, server_id: u32, playlist_id: u32) -> Option<u32> {
        self.store
            .music_bots
            .values()
            .find(|bot| bot.server_id == server_id && bot.playlist_id == playlist_id)
            .map(|bot| bot.id)
    }

    pub fn music_bot_notify_payload_by_identifier(
        &self,
        server_id: u32,
        bot_identifier: u64,
    ) -> Option<MusicBotNotifyPayload> {
        let bot = self.store.music_bots.values().find(|bot| {
            bot.server_id == server_id
                && (u64::from(bot.id) == bot_identifier
                    || bot.client_database_id == bot_identifier
                    || bot.linked_client_id == Some(bot_identifier))
        })?;

        let loaded_songs = bot
            .queue
            .iter()
            .filter(|entry| entry.loaded)
            .map(|entry| {
                let mut row = BTreeMap::new();
                row.insert(String::from("playlist_id"), bot.playlist_id.to_string());
                row.insert(String::from("song_id"), entry.id.to_string());
                row.insert(String::from("success"), String::from("1"));
                row.insert(
                    String::from("song_metadata"),
                    Self::format_song_metadata_json(entry),
                );
                row
            })
            .collect();

        Some(MusicBotNotifyPayload {
            client_id: bot.linked_client_id?,
            playlist_id: bot.playlist_id,
            client_updates: Self::music_bot_extra_properties(bot),
            song_change: Self::music_bot_song_change_row(bot),
            status_update: Self::music_bot_status_row(bot),
            player_info: Self::music_bot_playerinfo_row(bot),
            loaded_songs,
        })
    }

    pub fn music_bot_notify_payload_by_playlist_id(
        &self,
        server_id: u32,
        playlist_id: u32,
    ) -> Option<MusicBotNotifyPayload> {
        let bot_id = self.music_bot_id_by_playlist_id(server_id, playlist_id)?;
        self.music_bot_notify_payload_by_identifier(server_id, u64::from(bot_id))
    }

    pub fn playing_music_bots_status_rows(&self, server_id: u32) -> Vec<BTreeMap<String, String>> {
        self.store
            .music_bots
            .values()
            .filter(|bot| {
                bot.server_id == server_id
                    && bot.linked_client_id.is_some()
                    && bot.current_song_id.is_some()
                    && matches!(bot.state, MusicBotState::Playing | MusicBotState::Paused)
            })
            .map(Self::music_bot_status_row)
            .collect()
    }

    pub(crate) fn music_bot_id_by_client(&self, server_id: u32, client_id: u64) -> Option<u32> {
        self.store
            .music_bots
            .values()
            .find(|bot| bot.server_id == server_id && bot.linked_client_id == Some(client_id))
            .map(|bot| bot.id)
    }

}
