use std::collections::BTreeMap;

use serde_json::json;

use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MusicBotNotifyPayload {
    pub client_id: u64,
    pub client_updates: BTreeMap<String, String>,
    pub song_change: BTreeMap<String, String>,
    pub status_update: BTreeMap<String, String>,
}

#[derive(Debug, Clone)]
pub(super) struct MusicQueueEntry {
    pub(super) id: u32,
    pub(super) previous_song_id: u32,
    pub(super) url: String,
    pub(super) url_loader: String,
    pub(super) invoker_database_id: u64,
    pub(super) loaded: bool,
    pub(super) metadata: String,
    pub(super) title: String,
    pub(super) description: String,
    pub(super) thumbnail: String,
    pub(super) length_seconds: u32,
    pub(super) seekable: bool,
    pub(super) live_stream: bool,
}

#[derive(Debug, Clone)]
pub(super) struct PlaylistClientPermissionTarget {
    pub(super) client_database_id: u64,
    pub(super) permissions: BTreeMap<String, PermissionAssignment>,
}

#[derive(Debug, Clone)]
pub(super) struct ResolvedMusicMetadata {
    pub(super) loaded: bool,
    pub(super) title: String,
    pub(super) description: String,
    pub(super) thumbnail: String,
    pub(super) length_seconds: u32,
    pub(super) seekable: bool,
    pub(super) live_stream: bool,
}

#[derive(Debug, Clone)]
pub(super) struct MusicBot {
    pub(super) id: u32,
    pub(super) server_id: u32,
    pub(super) client_database_id: u64,
    pub(super) linked_client_id: Option<u64>,
    pub(super) playlist_id: u32,
    pub(super) current_song_id: Option<u32>,
    pub(super) next_song_id: u32,
    pub(super) state: MusicBotState,
    pub(super) player_volume: String,
    pub(super) playlist_title: String,
    pub(super) playlist_description: String,
    pub(super) playlist_flag_delete_played: bool,
    pub(super) playlist_flag_finished: bool,
    pub(super) playlist_replay_mode: u32,
    pub(super) playlist_max_songs: u32,
    pub(super) permissions: BTreeMap<String, PermissionAssignment>,
    pub(super) client_permissions: Vec<PlaylistClientPermissionTarget>,
    pub(super) current_song_started_at_millis: Option<u64>,
    pub(super) current_song_progress_millis: u32,
    pub(super) queue: Vec<MusicQueueEntry>,
}

#[derive(Debug, Clone)]
pub(super) enum MusicBotState {
    Stopped,
    Playing,
    Paused,
}

impl MusicBotState {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Stopped => "stopped",
            Self::Playing => "playing",
            Self::Paused => "paused",
        }
    }

    fn as_player_state(&self) -> &'static str {
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
                        || bot.client_database_id == bot_identifier)
            })
            .and_then(|bot| bot.linked_client_id)?;

        self.online_client_snapshot(server_id, client_id)
    }

    fn music_bot_extra_properties(bot: &MusicBot) -> BTreeMap<String, String> {
        let mut row = BTreeMap::new();
        row.insert(String::from("client_type_exact"), String::from("4"));
        row.insert(
            String::from("player_state"),
            bot.state.as_player_state().to_string(),
        );
        row.insert(String::from("player_volume"), bot.player_volume.clone());
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
        row
    }

    fn music_bot_current_song(bot: &MusicBot) -> Option<&MusicQueueEntry> {
        bot.current_song_id
            .and_then(|song_id| bot.queue.iter().find(|entry| entry.id == song_id))
    }

    fn music_bot_queue_position(bot: &MusicBot) -> u32 {
        bot.current_song_id
            .and_then(|song_id| bot.queue.iter().position(|entry| entry.id == song_id))
            .map(|index| index as u32 + 1)
            .unwrap_or(0)
    }

    fn music_bot_client_name(&self, bot: &MusicBot) -> String {
        bot.linked_client_id
            .and_then(|client_id| self.store.online_clients.get(&client_id))
            .map(|client| client.nickname.clone())
            .unwrap_or_else(|| format!("MusicBot {}", bot.id))
    }

    fn music_bot_playlist_title(&self, bot: &MusicBot) -> String {
        let title = bot.playlist_title.trim();
        if title.is_empty() {
            format!("{} Queue", self.music_bot_client_name(bot))
        } else {
            bot.playlist_title.clone()
        }
    }

    fn music_bot_playlist_description(&self, bot: &MusicBot) -> String {
        let description = bot.playlist_description.trim();
        if description.is_empty() {
            self.music_bot_client_name(bot)
        } else {
            bot.playlist_description.clone()
        }
    }

    fn music_bot_playlist_power(bot: &MusicBot, names: &[&str]) -> i64 {
        permission_value_or_default(&bot.permissions, names)
    }

    fn music_bot_current_song_max_millis(bot: &MusicBot) -> u32 {
        Self::music_bot_current_song(bot)
            .map(|song| song.length_seconds.saturating_mul(1000))
            .unwrap_or(0)
    }

    fn music_bot_current_song_replay_millis(bot: &MusicBot) -> u32 {
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

    fn music_bot_current_song_buffered_millis(bot: &MusicBot) -> u32 {
        let replay_millis = Self::music_bot_current_song_replay_millis(bot);
        let max_millis = Self::music_bot_current_song_max_millis(bot);
        if max_millis == 0 {
            replay_millis
        } else {
            replay_millis.saturating_add(15_000).min(max_millis)
        }
    }

    fn reset_music_bot_playback_progress(bot: &mut MusicBot) {
        bot.current_song_progress_millis = 0;
        bot.current_song_started_at_millis = if matches!(bot.state, MusicBotState::Playing)
            && bot.current_song_id.is_some()
        {
            Some(current_unix_timestamp_millis())
        } else {
            None
        };
    }

    fn pause_music_bot_playback(bot: &mut MusicBot) {
        bot.current_song_progress_millis = Self::music_bot_current_song_replay_millis(bot);
        bot.current_song_started_at_millis = None;
    }

    fn resume_music_bot_playback(bot: &mut MusicBot) {
        bot.current_song_started_at_millis = if bot.current_song_id.is_some() {
            Some(current_unix_timestamp_millis())
        } else {
            None
        };
    }

    fn stop_music_bot_playback(bot: &mut MusicBot) {
        bot.current_song_progress_millis = 0;
        bot.current_song_started_at_millis = None;
    }

    pub(super) fn normalize_music_bot_queue(bot: &mut MusicBot) {
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

    fn append_music_song_fields(row: &mut BTreeMap<String, String>, song: Option<&MusicQueueEntry>) {
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
        let song_length = song.map(|entry| entry.length_seconds).unwrap_or(0);

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
        row.insert(String::from("song_length"), song_length.to_string());
    }

    fn music_queue_entry_row(playlist_id: u32, entry: &MusicQueueEntry) -> BTreeMap<String, String> {
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
        row.insert(String::from("song_metadata"), entry.metadata.clone());
        row
    }

    fn music_bot_playerinfo_row(bot: &MusicBot) -> BTreeMap<String, String> {
        let current_song = Self::music_bot_current_song(bot);
        let replay_millis = Self::music_bot_current_song_replay_millis(bot);
        let buffered_millis = Self::music_bot_current_song_buffered_millis(bot);
        let max_millis = Self::music_bot_current_song_max_millis(bot);
        let mut row = BTreeMap::new();
        row.insert(String::from("bot_id"), bot.client_database_id.to_string());
        row.insert(
            String::from("player_state"),
            bot.state.as_player_state().to_string(),
        );
        row.insert(String::from("player_volume"), bot.player_volume.clone());
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
        row
    }

    fn music_bot_song_change_row(bot: &MusicBot) -> BTreeMap<String, String> {
        let current_song = Self::music_bot_current_song(bot);
        let mut row = BTreeMap::new();
        row.insert(String::from("bot_id"), bot.client_database_id.to_string());
        Self::append_music_song_fields(&mut row, current_song);
        row
    }

    fn music_bot_status_row(bot: &MusicBot) -> BTreeMap<String, String> {
        let mut row = BTreeMap::new();
        row.insert(String::from("bot_id"), bot.client_database_id.to_string());
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

    fn music_bot_playlist_list_row(&self, bot: &MusicBot) -> BTreeMap<String, String> {
        let bot_name = self.music_bot_client_name(bot);

        let mut row = BTreeMap::new();
        row.insert(String::from("playlist_id"), bot.playlist_id.to_string());
        row.insert(
            String::from("playlist_bot_id"),
            bot.client_database_id.to_string(),
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

    fn music_bot_playlist_info_row(&self, bot: &MusicBot) -> BTreeMap<String, String> {
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
                && (u64::from(bot.id) == bot_identifier || bot.client_database_id == bot_identifier)
        })?;

        Some(MusicBotNotifyPayload {
            client_id: bot.linked_client_id?,
            client_updates: Self::music_bot_extra_properties(bot),
            song_change: Self::music_bot_song_change_row(bot),
            status_update: Self::music_bot_status_row(bot),
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

    pub(super) fn music_bot_id_by_client(&self, server_id: u32, client_id: u64) -> Option<u32> {
        self.store
            .music_bots
            .values()
            .find(|bot| bot.server_id == server_id && bot.linked_client_id == Some(client_id))
            .map(|bot| bot.id)
    }

    fn resolve_music_bot_id(&self, server_id: u32, request: &CommandRequest) -> Option<u32> {
        let requested_id = request
            .named_args
            .get("botid")
            .or_else(|| request.named_args.get("bot_id"))
            .and_then(|value| value.parse::<u64>().ok());

        match requested_id {
            Some(requested_id) => {
                if let Ok(internal_id) = u32::try_from(requested_id)
                    && self
                        .store
                        .music_bots
                        .get(&internal_id)
                        .is_some_and(|bot| bot.server_id == server_id)
                {
                    return Some(internal_id);
                }

                self.store
                    .music_bots
                    .values()
                    .find(|bot| {
                        bot.server_id == server_id && bot.client_database_id == requested_id
                    })
                    .map(|bot| bot.id)
            }
            None => self
                .store
                .music_bots
                .values()
                .find(|bot| bot.server_id == server_id)
                .map(|bot| bot.id),
        }
    }

    fn resolve_music_bot_id_by_playlist_request(
        &self,
        server_id: u32,
        request: &CommandRequest,
    ) -> Option<u32> {
        request
            .named_args
            .get("playlist_id")
            .and_then(|value| value.parse::<u32>().ok())
            .and_then(|playlist_id| self.music_bot_id_by_playlist_id(server_id, playlist_id))
    }

    fn infer_music_loader(raw_loader: Option<&str>, url: &str) -> String {
        let lower_url = url.to_ascii_lowercase();
        let loader = raw_loader
            .map(|value| value.trim().to_ascii_lowercase())
            .unwrap_or_default();

        if loader == "channel" || lower_url.starts_with("channel://") {
            return String::from("channel");
        }
        if lower_url.contains("youtube.com/") || lower_url.contains("youtu.be/") {
            return String::from("youtube");
        }
        if matches!(loader.as_str(), "ffmpeg" | "radio" | "stream") {
            return String::from("ffmpeg");
        }
        if lower_url.starts_with("http://") || lower_url.starts_with("https://") {
            return String::from("ffmpeg");
        }
        if matches!(loader.as_str(), "yt" | "youtube") {
            return String::from("youtube");
        }
        if loader == "any" || loader.is_empty() {
            return String::from("any");
        }

        loader
    }

    fn infer_music_live_stream(url_loader: &str, url: &str) -> bool {
        if url_loader == "channel" {
            return false;
        }

        let lower_url = url.to_ascii_lowercase();
        let path = lower_url
            .split(['?', '#'])
            .next()
            .unwrap_or(lower_url.as_str());
        let extension = path.rsplit('.').next().unwrap_or_default();

        if matches!(extension, "m3u" | "m3u8" | "pls" | "asx" | "xspf") {
            return true;
        }

        url_loader == "ffmpeg"
            && !matches!(extension, "mp3" | "aac" | "ogg" | "opus" | "wav" | "flac")
    }

    pub(super) fn music_entry_display_name(url: &str, live_stream: bool) -> String {
        let trimmed = url.trim();
        let without_scheme = trimmed
            .trim_start_matches("https://")
            .trim_start_matches("http://");
        let host = without_scheme.split('/').next().unwrap_or(without_scheme).trim();
        if live_stream && !host.is_empty() {
            return format!("Webradio {}", host);
        }

        let path = trimmed
            .split(['?', '#'])
            .next()
            .unwrap_or(trimmed)
            .trim_end_matches('/');
        let tail = path
            .rsplit('/')
            .next()
            .filter(|segment| !segment.is_empty() && *segment != path)
            .unwrap_or(path);
        let label = tail.rsplit_once('.').map(|(name, _)| name).unwrap_or(tail).trim();

        if !label.is_empty() && !label.contains("://") {
            return label.replace(['_', '-'], " ");
        }

        if !host.is_empty() {
            return host.to_string();
        }

        trimmed.to_string()
    }

    fn resolve_music_metadata(&self, url_loader: &str, url: &str) -> ResolvedMusicMetadata {
        let live_stream = Self::infer_music_live_stream(url_loader, url);
        let title = Self::music_entry_display_name(url, live_stream);
        let description = if live_stream {
            format!("Live stream: {}", url)
        } else if url_loader == "channel" {
            format!("Server audio: {}", url)
        } else {
            url.to_string()
        };

        let fallback = ResolvedMusicMetadata {
            loaded: true,
            title,
            description,
            thumbnail: String::new(),
            length_seconds: 0,
            seekable: !live_stream,
            live_stream,
        };

        match url_loader {
            "channel" => self.probe_channel_music_metadata(url).unwrap_or(fallback),
            "youtube" => Self::probe_youtube_music_metadata(url).unwrap_or(fallback),
            _ => fallback,
        }
    }

    fn build_music_queue_entry(
        &self,
        song_id: u32,
        url: &str,
        requested_loader: Option<&str>,
        invoker_database_id: u64,
    ) -> MusicQueueEntry {
        let url_loader = Self::infer_music_loader(requested_loader, url);
        let resolved = self.resolve_music_metadata(&url_loader, url);
        let metadata_title = resolved.title.clone();
        let metadata_description = resolved.description.clone();
        let metadata_thumbnail = if resolved.thumbnail.trim().is_empty() {
            String::from("none")
        } else {
            resolved.thumbnail.clone()
        };
        let metadata = json!({
            "title": metadata_title,
            "description": metadata_description,
            "length": resolved.length_seconds,
            "thumbnail": metadata_thumbnail,
            "is_live": resolved.live_stream,
            "seekable": resolved.seekable,
        })
        .to_string();

        MusicQueueEntry {
            id: song_id,
            previous_song_id: 0,
            url: url.to_string(),
            url_loader,
            invoker_database_id,
            loaded: resolved.loaded,
            metadata,
            title: resolved.title,
            description: resolved.description,
            thumbnail: resolved.thumbnail,
            length_seconds: resolved.length_seconds,
            seekable: resolved.seekable,
            live_stream: resolved.live_stream,
        }
    }

    fn insert_music_queue_entry(
        bot: &mut MusicBot,
        entry: MusicQueueEntry,
        previous_song_id: u32,
    ) -> std::result::Result<(), QueryResponse> {
        let insert_index = if previous_song_id == 0 {
            0
        } else {
            bot.queue
                .iter()
                .position(|song| song.id == previous_song_id)
                .map(|index| index + 1)
                .ok_or_else(|| QueryResponse::error(768, "song not found"))?
        };

        let queue_was_empty = bot.queue.is_empty();
        bot.queue.insert(insert_index, entry);
        Self::normalize_music_bot_queue(bot);
        if queue_was_empty {
            bot.current_song_id = bot.queue.first().map(|song| song.id);
            if bot.current_song_id.is_some() {
                bot.state = MusicBotState::Playing;
                Self::reset_music_bot_playback_progress(bot);
            }
        }

        Ok(())
    }

    fn remove_music_queue_entry(
        bot: &mut MusicBot,
        song_id: u32,
    ) -> std::result::Result<MusicQueueEntry, QueryResponse> {
        let index = bot
            .queue
            .iter()
            .position(|song| song.id == song_id)
            .ok_or_else(|| QueryResponse::error(768, "song not found"))?;

        let removed = bot.queue.remove(index);
        if bot.current_song_id == Some(song_id) {
            bot.current_song_id = bot
                .queue
                .get(index)
                .or_else(|| index.checked_sub(1).and_then(|previous| bot.queue.get(previous)))
                .map(|song| song.id);
            if bot.current_song_id.is_none() {
                bot.state = MusicBotState::Stopped;
            }
        }

        Self::normalize_music_bot_queue(bot);
        Ok(removed)
    }

    fn reorder_music_queue_entry(
        bot: &mut MusicBot,
        song_id: u32,
        previous_song_id: u32,
    ) -> std::result::Result<(), QueryResponse> {
        let current_index = bot
            .queue
            .iter()
            .position(|song| song.id == song_id)
            .ok_or_else(|| QueryResponse::error(768, "song not found"))?;
        let entry = bot.queue.remove(current_index);

        let mut insert_index = if previous_song_id == 0 {
            0
        } else {
            bot.queue
                .iter()
                .position(|song| song.id == previous_song_id)
                .map(|index| index + 1)
                .ok_or_else(|| QueryResponse::error(768, "song not found"))?
        };
        if insert_index > bot.queue.len() {
            insert_index = bot.queue.len();
        }

        bot.queue.insert(insert_index, entry);
        Self::normalize_music_bot_queue(bot);
        Ok(())
    }

    fn set_music_queue_current_song(
        bot: &mut MusicBot,
        song_id: u32,
    ) -> std::result::Result<(), QueryResponse> {
        if !bot.queue.iter().any(|song| song.id == song_id) {
            return Err(QueryResponse::error(768, "song not found"));
        }

        bot.current_song_id = Some(song_id);
        bot.state = MusicBotState::Playing;
        Self::normalize_music_bot_queue(bot);
        Self::reset_music_bot_playback_progress(bot);
        Ok(())
    }

    pub fn update_downloaded_music_track(&mut self, bot_id: u32, song_id: u32, url_loader: &str, url: &str) {
        let mut state_changed = false;
        if let Some(bot) = self.store.music_bots.get_mut(&bot_id) {
            if let Some(song) = bot.queue.iter_mut().find(|s| s.id == song_id) {
                song.url_loader = url_loader.to_string();
                song.url = url.to_string();
                song.loaded = true;
                state_changed = true;
            }
        }
        if state_changed {
            self.sync_music_bot_client_state(bot_id);
        }
    }

    pub(super) fn sync_music_bot_client_state(&mut self, bot_id: u32) {
        let Some((server_id, linked_client_id, extra_properties)) = self
            .store
            .music_bots
            .get(&bot_id)
            .map(|bot| {
                (
                    bot.server_id,
                    bot.linked_client_id,
                    Self::music_bot_extra_properties(bot),
                )
            })
        else {
            return;
        };

        let Some(client_id) = linked_client_id else {
            return;
        };
        let Some(client) = self.store.online_clients.get_mut(&client_id) else {
            return;
        };
        if client.server_id != server_id {
            return;
        }

        client.extra_properties.extend(extra_properties);
    }

    pub(super) fn handle_musicbotcreate(
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
        let target_channel_id = request
            .named_args
            .get("cid")
            .and_then(|value| value.parse::<u32>().ok())
            .or(session.current_channel_id)
            .or_else(|| self.default_channel_id_for_server(server_id));
        let Some(target_channel_id) = target_channel_id else {
            return QueryResponse::error(768, "target channel not found");
        };
        if !self.channel_exists(server_id, target_channel_id) {
            return QueryResponse::error(768, "target channel not found");
        }

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
        let music_bot = MusicBot {
            id: bot_id,
            server_id,
            client_database_id,
            linked_client_id: Some(client_id),
            playlist_id: bot_id,
            current_song_id: None,
            next_song_id: 1,
            state: MusicBotState::Stopped,
            player_volume: String::from("1"),
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
        let nickname = request
            .named_args
            .get("botname")
            .filter(|value| !value.trim().is_empty())
            .cloned()
            .unwrap_or_else(|| format!("MusicBot {bot_id}"));

        self.store.online_clients.insert(
            client_id,
            OnlineClient {
                id: client_id,
                database_id: client_database_id,
                unique_identifier: format!("compat-musicbot-{client_database_id}"),
                nickname,
                away: false,
                away_message: String::new(),
                input_muted: false,
                output_muted: false,
                server_id,
                channel_id: target_channel_id,
                client_type: 0,
                version: String::from("BlackTeaSpeak 1.5.6 compat-musicbot"),
                platform: String::from("compat-rust"),
                country: String::from("ZZ"),
                connection_ip: String::new(),
                server_groups: vec![8],
                connected_at: current_unix_timestamp(),
                last_seen_at: current_unix_timestamp(),
                extra_properties: Self::music_bot_extra_properties(&music_bot),
            },
        );
        self.store.music_bots.insert(bot_id, music_bot);

        let mut row = BTreeMap::new();
        row.insert(String::from("botid"), bot_id.to_string());
        row.insert(String::from("clid"), client_id.to_string());
        row.insert(
            String::from("client_database_id"),
            client_database_id.to_string(),
        );
        QueryResponse::ok_row(row)
    }

    pub(super) fn handle_musicbotdelete(
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
        let Some(bot_id) = self.resolve_music_bot_id(server_id, request) else {
            return QueryResponse::error(768, "music bot not found");
        };
        let linked_client_id = self
            .store
            .music_bots
            .get(&bot_id)
            .and_then(|bot| bot.linked_client_id);

        self.store.music_bots.remove(&bot_id);
        if let Some(client_id) = linked_client_id {
            self.store.online_clients.remove(&client_id);
        }

        QueryResponse::ok()
    }

    pub(super) fn handle_musicbotqueueadd(
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
        let Some(bot_id) = self.resolve_music_bot_id(server_id, request) else {
            return QueryResponse::error(768, "music bot not found");
        };
        let Some(url) = request
            .named_args
            .get("url")
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
        else {
            return QueryResponse::error(512, "url is required");
        };
        let (invoker_database_id, actor_permissions) =
            match self.query_actor_effective_permissions(session) {
                Ok((actor, actor_permissions)) => (actor.client_database_id, actor_permissions),
                Err(response) => return response,
            };
        let requested_loader = request
            .named_args
            .get("type")
            .or_else(|| request.named_args.get("invoker"))
            .map(String::as_str);

        let (previous_song_id, song_id, target_permissions) = {
            let Some(bot) = self.store.music_bots.get(&bot_id) else {
                return QueryResponse::error(768, "music bot not found");
            };
            (
                request
                    .named_args
                    .get("previous")
                    .or_else(|| request.named_args.get("song_previous_song_id"))
                    .and_then(|value| value.parse::<u32>().ok())
                    .unwrap_or_else(|| bot.queue.last().map(|song| song.id).unwrap_or(0)),
                bot.next_song_id.max(1),
                bot.permissions.clone(),
            )
        };
        if let Err(response) =
            self.ensure_playlist_song_add_allowed(&actor_permissions, &target_permissions)
        {
            return response;
        }
        let entry = self.build_music_queue_entry(song_id, url, requested_loader, invoker_database_id);

        let url_loader = entry.url_loader.clone();
        let response_row = {
            let Some(bot) = self.store.music_bots.get_mut(&bot_id) else {
                return QueryResponse::error(768, "music bot not found");
            };
            bot.next_song_id = song_id.saturating_add(1);
            if let Err(response) = Self::insert_music_queue_entry(bot, entry, previous_song_id) {
                return response;
            }
            let Some(entry) = bot.queue.iter().find(|song| song.id == song_id) else {
                return QueryResponse::error(768, "song not found");
            };
            let mut row = Self::music_queue_entry_row(bot.playlist_id, entry);
            row.insert(String::from("bot_id"), bot.client_database_id.to_string());
            row.insert(String::from("botid"), bot.id.to_string());
            row
        };

        if url_loader == "youtube" {
            if let Some(tx) = self.music_download_tx.as_ref() {
                let _ = tx.send((bot_id, song_id, url.to_string()));
            }
        }

        self.sync_music_bot_client_state(bot_id);
        QueryResponse::ok_row(response_row)
    }

    pub(super) fn handle_musicbotqueuelist(
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
        let Some(bot_id) = self.resolve_music_bot_id(server_id, request) else {
            return QueryResponse::error(768, "music bot not found");
        };
        let (_actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        let Some(bot) = self.store.music_bots.get(&bot_id) else {
            return QueryResponse::error(768, "music bot not found");
        };
        if let Err(response) = self.ensure_playlist_view_allowed(&actor_permissions, &bot.permissions)
        {
            return response;
        }
        if bot.queue.is_empty() {
            return QueryResponse::error(ERROR_DATABASE_EMPTY_RESULT, "database empty result set");
        }

        QueryResponse::ok_rows(
            bot.queue
                .iter()
                .map(|entry| {
                    let mut row = Self::music_queue_entry_row(bot.playlist_id, entry);
                    row.insert(String::from("bot_id"), bot.client_database_id.to_string());
                    row.insert(String::from("botid"), bot.id.to_string());
                    row
                })
                .collect(),
        )
    }

    pub(super) fn handle_musicbotqueueremove(
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
        let Some(bot_id) = self.resolve_music_bot_id(server_id, request) else {
            return QueryResponse::error(768, "music bot not found");
        };
        let Some(song_id) = request
            .named_args
            .get("song_id")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return QueryResponse::error(512, "song_id is required");
        };

        let (_actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        let Some(target_permissions) = self
            .store
            .music_bots
            .get(&bot_id)
            .map(|bot| bot.permissions.clone())
        else {
            return QueryResponse::error(768, "music bot not found");
        };
        if let Err(response) =
            self.ensure_playlist_song_remove_allowed(&actor_permissions, &target_permissions)
        {
            return response;
        }

        let response_row = {
            let Some(bot) = self.store.music_bots.get_mut(&bot_id) else {
                return QueryResponse::error(768, "music bot not found");
            };
            let removed = match Self::remove_music_queue_entry(bot, song_id) {
                Ok(removed) => removed,
                Err(response) => return response,
            };
            let mut row = Self::music_queue_entry_row(bot.playlist_id, &removed);
            row.insert(String::from("bot_id"), bot.client_database_id.to_string());
            row.insert(String::from("botid"), bot.id.to_string());
            row
        };

        self.sync_music_bot_client_state(bot_id);
        QueryResponse::ok_row(response_row)
    }

    pub(super) fn handle_musicbotqueuereorder(
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
        let Some(bot_id) = self.resolve_music_bot_id(server_id, request) else {
            return QueryResponse::error(768, "music bot not found");
        };
        let Some(song_id) = request
            .named_args
            .get("song_id")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return QueryResponse::error(512, "song_id is required");
        };
        let Some(previous_song_id) = request
            .named_args
            .get("song_previous_song_id")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return QueryResponse::error(512, "song_previous_song_id is required");
        };

        let (_actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        let Some(target_permissions) = self
            .store
            .music_bots
            .get(&bot_id)
            .map(|bot| bot.permissions.clone())
        else {
            return QueryResponse::error(768, "music bot not found");
        };
        if let Err(response) =
            self.ensure_playlist_song_move_allowed(&actor_permissions, &target_permissions)
        {
            return response;
        }

        let response_row = {
            let Some(bot) = self.store.music_bots.get_mut(&bot_id) else {
                return QueryResponse::error(768, "music bot not found");
            };
            if let Err(response) = Self::reorder_music_queue_entry(bot, song_id, previous_song_id)
            {
                return response;
            }
            let Some(entry) = bot.queue.iter().find(|song| song.id == song_id) else {
                return QueryResponse::error(768, "song not found");
            };
            let mut row = Self::music_queue_entry_row(bot.playlist_id, entry);
            row.insert(String::from("bot_id"), bot.client_database_id.to_string());
            row.insert(String::from("botid"), bot.id.to_string());
            row
        };

        self.sync_music_bot_client_state(bot_id);
        QueryResponse::ok_row(response_row)
    }

    pub(super) fn handle_musicbotplayeraction(
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
        let Some(bot_id) = self.resolve_music_bot_id(server_id, request) else {
            return QueryResponse::error(768, "music bot not found");
        };
        let action = request
            .named_args
            .get("action")
            .and_then(|value| value.parse::<u32>().ok())
            .unwrap_or(0);

        let (response_bot_id, response_state, response_queue_position) = {
            let Some(bot) = self.store.music_bots.get_mut(&bot_id) else {
                return QueryResponse::error(768, "music bot not found");
            };

            let previous_song_id = bot.current_song_id;

            match action {
                0 => {
                    bot.state = MusicBotState::Stopped;
                    Self::stop_music_bot_playback(bot);
                }
                1 => {
                    if bot.current_song_id.is_none() {
                        bot.current_song_id = bot.queue.first().map(|song| song.id);
                    }
                    if bot.current_song_id.is_some() {
                        bot.state = MusicBotState::Playing;
                        if previous_song_id != bot.current_song_id {
                            Self::reset_music_bot_playback_progress(bot);
                        } else {
                            Self::resume_music_bot_playback(bot);
                        }
                    }
                }
                2 => {
                    bot.state = if bot.current_song_id.is_some() {
                        MusicBotState::Paused
                    } else {
                        MusicBotState::Stopped
                    };
                    if matches!(bot.state, MusicBotState::Paused) {
                        Self::pause_music_bot_playback(bot);
                    } else {
                        Self::stop_music_bot_playback(bot);
                    }
                }
                3 => {
                    let next_song_id = bot.current_song_id.and_then(|current_song_id| {
                        bot.queue
                            .iter()
                            .position(|song| song.id == current_song_id)
                            .and_then(|index| bot.queue.get(index + 1))
                            .map(|song| song.id)
                    });
                    bot.current_song_id = next_song_id;
                    bot.state = if bot.current_song_id.is_some() {
                        MusicBotState::Playing
                    } else {
                        MusicBotState::Stopped
                    };
                    if bot.current_song_id.is_some() {
                        Self::reset_music_bot_playback_progress(bot);
                    } else {
                        Self::stop_music_bot_playback(bot);
                    }
                }
                4 => {
                    bot.current_song_id = bot
                        .current_song_id
                        .and_then(|current_song_id| {
                            bot.queue
                                .iter()
                                .position(|song| song.id == current_song_id)
                                .and_then(|index| index.checked_sub(1))
                                .and_then(|index| bot.queue.get(index))
                                .map(|song| song.id)
                        })
                        .or_else(|| bot.queue.first().map(|song| song.id));
                    bot.state = if bot.current_song_id.is_some() {
                        MusicBotState::Playing
                    } else {
                        MusicBotState::Stopped
                    };
                    if bot.current_song_id.is_some() {
                        Self::reset_music_bot_playback_progress(bot);
                    } else {
                        Self::stop_music_bot_playback(bot);
                    }
                }
                _ => return QueryResponse::error(512, "unsupported music bot action"),
            }

            Self::normalize_music_bot_queue(bot);

            (
                bot.id,
                bot.state.as_str().to_string(),
                Self::music_bot_queue_position(bot),
            )
        };

        self.sync_music_bot_client_state(bot_id);

        let mut row = BTreeMap::new();
        row.insert(String::from("botid"), response_bot_id.to_string());
        row.insert(String::from("state"), response_state);
        row.insert(
            String::from("queue_position"),
            response_queue_position.to_string(),
        );
        QueryResponse::ok_row(row)
    }

    pub(super) fn handle_musicbotplayerinfo(
        &self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        let Some(server_id) = session.selected_virtual_server_id else {
            return QueryResponse::error(522, "virtual server selection required");
        };
        let Some(bot_id) = self.resolve_music_bot_id(server_id, request) else {
            return QueryResponse::error(768, "music bot not found");
        };
        let Some(bot) = self.store.music_bots.get(&bot_id) else {
            return QueryResponse::error(768, "music bot not found");
        };

        QueryResponse::ok_row(Self::music_bot_playerinfo_row(bot))
    }

    pub(super) fn handle_musicbotsetsubscription(
        &self,
        request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        let Some(server_id) = session.selected_virtual_server_id else {
            return QueryResponse::error(522, "virtual server selection required");
        };
        let Some(bot_id) = self.resolve_music_bot_id(server_id, request) else {
            return QueryResponse::error(768, "music bot not found");
        };
        let Some(bot) = self.store.music_bots.get(&bot_id) else {
            return QueryResponse::error(768, "music bot not found");
        };

        let mut row = BTreeMap::new();
        row.insert(String::from("bot_id"), bot.client_database_id.to_string());
        QueryResponse::ok_row(row)
    }

    pub(super) fn handle_playlistlist(
        &self,
        _request: &CommandRequest,
        session: &QuerySessionState,
    ) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }

        let Some(server_id) = session.selected_virtual_server_id else {
            return QueryResponse::error(522, "virtual server selection required");
        };
        let (_actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };

        let rows = self
            .store
            .music_bots
            .values()
            .filter(|bot| bot.server_id == server_id)
            .filter(|bot| {
                self.ensure_playlist_view_allowed(&actor_permissions, &bot.permissions)
                    .is_ok()
            })
            .map(|bot| self.music_bot_playlist_list_row(bot))
            .collect::<Vec<_>>();
        if rows.is_empty() {
            return QueryResponse::error(ERROR_DATABASE_EMPTY_RESULT, "database empty result set");
        }

        QueryResponse::ok_rows(rows)
    }

    pub(super) fn handle_playlistinfo(
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
        let Some(bot_id) = self.resolve_music_bot_id_by_playlist_request(server_id, request) else {
            return QueryResponse::error(768, "playlist not found");
        };
        let (_actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        let Some(bot) = self.store.music_bots.get(&bot_id) else {
            return QueryResponse::error(768, "playlist not found");
        };
        if let Err(response) = self.ensure_playlist_view_allowed(&actor_permissions, &bot.permissions)
        {
            return response;
        }

        QueryResponse::ok_row(self.music_bot_playlist_info_row(bot))
    }

    pub(super) fn handle_playlistedit(
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
        let Some(bot_id) = self.resolve_music_bot_id_by_playlist_request(server_id, request) else {
            return QueryResponse::error(768, "playlist not found");
        };
        let (_actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        let Some(target_permissions) = self
            .store
            .music_bots
            .get(&bot_id)
            .map(|bot| bot.permissions.clone())
        else {
            return QueryResponse::error(768, "playlist not found");
        };
        if let Err(response) =
            self.ensure_playlist_modify_allowed(&actor_permissions, &target_permissions)
        {
            return response;
        }

        let Some(bot) = self.store.music_bots.get_mut(&bot_id) else {
            return QueryResponse::error(768, "playlist not found");
        };

        if let Some(title) = request.named_args.get("playlist_title") {
            bot.playlist_title = title.clone();
        }
        if let Some(description) = request.named_args.get("playlist_description") {
            bot.playlist_description = description.clone();
        }
        if let Some(delete_played) = request
            .named_args
            .get("playlist_flag_delete_played")
            .and_then(|value| parse_query_bool(value))
        {
            bot.playlist_flag_delete_played = delete_played;
        }
        if let Some(finished) = request
            .named_args
            .get("playlist_flag_finished")
            .and_then(|value| parse_query_bool(value))
        {
            bot.playlist_flag_finished = finished;
        }
        if let Some(replay_mode) = request
            .named_args
            .get("playlist_replay_mode")
            .and_then(|value| value.parse::<u32>().ok())
        {
            bot.playlist_replay_mode = replay_mode;
        }
        if let Some(max_songs) = request
            .named_args
            .get("playlist_max_songs")
            .and_then(|value| value.parse::<u32>().ok())
        {
            bot.playlist_max_songs = max_songs;
        }

        self.sync_music_bot_client_state(bot_id);
        QueryResponse::ok()
    }

    pub(super) fn handle_playlistsetsubscription(
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
        let Some(bot_id) = self.resolve_music_bot_id_by_playlist_request(server_id, request) else {
            return QueryResponse::error(768, "playlist not found");
        };
        let (_actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        let Some(bot) = self.store.music_bots.get(&bot_id) else {
            return QueryResponse::error(768, "playlist not found");
        };
        if let Err(response) = self.ensure_playlist_view_allowed(&actor_permissions, &bot.permissions)
        {
            return response;
        }

        QueryResponse::ok()
    }

    pub(super) fn handle_playlistsonglist(
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
        let Some(bot_id) = self.resolve_music_bot_id_by_playlist_request(server_id, request) else {
            return QueryResponse::error(768, "playlist not found");
        };
        let (_actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        let Some(bot) = self.store.music_bots.get(&bot_id) else {
            return QueryResponse::error(768, "playlist not found");
        };
        if let Err(response) = self.ensure_playlist_view_allowed(&actor_permissions, &bot.permissions)
        {
            return response;
        }
        if bot.queue.is_empty() {
            return QueryResponse::error(ERROR_DATABASE_EMPTY_RESULT, "database empty result set");
        }

        let extract_metadata = request.flags.contains("extract-metadata");
        QueryResponse::ok_rows(
            bot.queue
                .iter()
                .map(|entry| {
                    let mut row = Self::music_queue_entry_row(bot.playlist_id, entry);
                    if extract_metadata && entry.loaded {
                        row.insert(String::from("song_metadata_title"), entry.title.clone());
                        row.insert(
                            String::from("song_metadata_description"),
                            entry.description.clone(),
                        );
                        row.insert(
                            String::from("song_metadata_thumbnail_url"),
                            if entry.thumbnail.is_empty() {
                                String::from("none")
                            } else {
                                entry.thumbnail.clone()
                            },
                        );
                        row.insert(
                            String::from("song_metadata_length"),
                            entry.length_seconds.to_string(),
                        );
                    }
                    row
                })
                .collect(),
        )
    }

    pub(super) fn handle_playlistsongadd(
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
        let Some(bot_id) = self.resolve_music_bot_id_by_playlist_request(server_id, request) else {
            return QueryResponse::error(768, "playlist not found");
        };
        let Some(url) = request
            .named_args
            .get("url")
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
        else {
            return QueryResponse::error(512, "url is required");
        };
        let (invoker_database_id, actor_permissions) =
            match self.query_actor_effective_permissions(session) {
                Ok((actor, actor_permissions)) => (actor.client_database_id, actor_permissions),
                Err(response) => return response,
            };

        let requested_loader = request
            .named_args
            .get("type")
            .or_else(|| request.named_args.get("invoker"))
            .map(String::as_str);
        let (previous_song_id, song_id, target_permissions) = {
            let Some(bot) = self.store.music_bots.get(&bot_id) else {
                return QueryResponse::error(768, "playlist not found");
            };
            (
                request
                    .named_args
                    .get("previous")
                    .or_else(|| request.named_args.get("song_previous_song_id"))
                    .and_then(|value| value.parse::<u32>().ok())
                    .unwrap_or_else(|| bot.queue.last().map(|song| song.id).unwrap_or(0)),
                bot.next_song_id.max(1),
                bot.permissions.clone(),
            )
        };
        if let Err(response) =
            self.ensure_playlist_song_add_allowed(&actor_permissions, &target_permissions)
        {
            return response;
        }
        let entry = self.build_music_queue_entry(song_id, url, requested_loader, invoker_database_id);

        let url_loader = entry.url_loader.clone();
        let response_row = {
            let Some(bot) = self.store.music_bots.get_mut(&bot_id) else {
                return QueryResponse::error(768, "playlist not found");
            };
            bot.next_song_id = song_id.saturating_add(1);
            if let Err(response) = Self::insert_music_queue_entry(bot, entry, previous_song_id) {
                return response;
            }
            let Some(entry) = bot.queue.iter().find(|song| song.id == song_id) else {
                return QueryResponse::error(768, "song not found");
            };
            Self::music_queue_entry_row(bot.playlist_id, entry)
        };

        if url_loader == "youtube" {
            if let Some(tx) = self.music_download_tx.as_ref() {
                let _ = tx.send((bot_id, song_id, url.to_string()));
            }
        }

        self.sync_music_bot_client_state(bot_id);
        QueryResponse::ok_row(response_row)
    }

    pub(super) fn handle_playlistsongremove(
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
        let Some(bot_id) = self.resolve_music_bot_id_by_playlist_request(server_id, request) else {
            return QueryResponse::error(768, "playlist not found");
        };
        let Some(song_id) = request
            .named_args
            .get("song_id")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return QueryResponse::error(512, "song_id is required");
        };

        let (_actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        let Some(target_permissions) = self
            .store
            .music_bots
            .get(&bot_id)
            .map(|bot| bot.permissions.clone())
        else {
            return QueryResponse::error(768, "playlist not found");
        };
        if let Err(response) =
            self.ensure_playlist_song_remove_allowed(&actor_permissions, &target_permissions)
        {
            return response;
        }

        let response_row = {
            let Some(bot) = self.store.music_bots.get_mut(&bot_id) else {
                return QueryResponse::error(768, "playlist not found");
            };
            let removed = match Self::remove_music_queue_entry(bot, song_id) {
                Ok(removed) => removed,
                Err(response) => return response,
            };
            Self::music_queue_entry_row(bot.playlist_id, &removed)
        };

        self.sync_music_bot_client_state(bot_id);
        QueryResponse::ok_row(response_row)
    }

    pub(super) fn handle_playlistsongreorder(
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
        let Some(bot_id) = self.resolve_music_bot_id_by_playlist_request(server_id, request) else {
            return QueryResponse::error(768, "playlist not found");
        };
        let Some(song_id) = request
            .named_args
            .get("song_id")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return QueryResponse::error(512, "song_id is required");
        };
        let Some(previous_song_id) = request
            .named_args
            .get("song_previous_song_id")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return QueryResponse::error(512, "song_previous_song_id is required");
        };

        let (_actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        let Some(target_permissions) = self
            .store
            .music_bots
            .get(&bot_id)
            .map(|bot| bot.permissions.clone())
        else {
            return QueryResponse::error(768, "playlist not found");
        };
        if let Err(response) =
            self.ensure_playlist_song_move_allowed(&actor_permissions, &target_permissions)
        {
            return response;
        }

        let response_row = {
            let Some(bot) = self.store.music_bots.get_mut(&bot_id) else {
                return QueryResponse::error(768, "playlist not found");
            };
            if let Err(response) = Self::reorder_music_queue_entry(bot, song_id, previous_song_id)
            {
                return response;
            }
            let Some(entry) = bot.queue.iter().find(|song| song.id == song_id) else {
                return QueryResponse::error(768, "song not found");
            };
            Self::music_queue_entry_row(bot.playlist_id, entry)
        };

        self.sync_music_bot_client_state(bot_id);
        QueryResponse::ok_row(response_row)
    }

    pub(super) fn handle_playlistsongsetcurrent(
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
        let Some(bot_id) = self.resolve_music_bot_id_by_playlist_request(server_id, request) else {
            return QueryResponse::error(768, "playlist not found");
        };
        let Some(song_id) = request
            .named_args
            .get("song_id")
            .and_then(|value| value.parse::<u32>().ok())
        else {
            return QueryResponse::error(512, "song_id is required");
        };

        let (_actor, actor_permissions) = match self.query_actor_effective_permissions(session) {
            Ok(actor) => actor,
            Err(response) => return response,
        };
        let Some(target_permissions) = self
            .store
            .music_bots
            .get(&bot_id)
            .map(|bot| bot.permissions.clone())
        else {
            return QueryResponse::error(768, "playlist not found");
        };
        if let Err(response) =
            self.ensure_playlist_song_move_allowed(&actor_permissions, &target_permissions)
        {
            return response;
        }

        let Some(bot) = self.store.music_bots.get_mut(&bot_id) else {
            return QueryResponse::error(768, "playlist not found");
        };
        if let Err(response) = Self::set_music_queue_current_song(bot, song_id) {
            return response;
        }

        self.sync_music_bot_client_state(bot_id);
        QueryResponse::ok()
    }

    pub(crate) fn handle_musicbotlist(&self, session: &QuerySessionState) -> QueryResponse {
        if !session_has_permission_actor(session) {
            return QueryResponse::error(521, "login required");
        }

        let Some(server_id) = session.selected_virtual_server_id else {
            return QueryResponse::error(522, "virtual server selection required");
        };

        let rows = self.store.music_bots.values()
            .filter(|bot| bot.server_id == server_id)
            .map(|bot| {
                let mut row = Self::music_bot_playerinfo_row(bot);
                row.insert(String::from("name"), self.music_bot_client_name(bot));
                row
            })
            .collect::<Vec<_>>();

        QueryResponse::ok_rows(rows)
    }
}
