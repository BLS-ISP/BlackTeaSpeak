use serde_json::json;
use crate::runtime::*;
use super::*;

impl BaselineRuntime {
    pub(crate) fn resolve_music_bot_id(&self, server_id: u32, request: &CommandRequest) -> Option<u32> {
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
                        bot.server_id == server_id
                            && (bot.client_database_id == requested_id
                                || bot.linked_client_id == Some(requested_id))
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

    pub(crate) fn resolve_music_bot_id_by_playlist_request(
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

    pub(crate) fn infer_music_loader(raw_loader: Option<&str>, url: &str) -> String {
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

    pub(crate) fn infer_music_live_stream(url_loader: &str, url: &str) -> bool {
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

    pub(crate) fn music_entry_display_name(url: &str, live_stream: bool) -> String {
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

    pub(crate) fn resolve_music_metadata(&self, url_loader: &str, url: &str) -> ResolvedMusicMetadata {
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
            "youtube" => {
                if std::env::var_os("TEASPEAK_COMPAT_YTDLP").is_some() {
                    Self::probe_youtube_music_metadata(url).unwrap_or(fallback)
                } else {
                    fallback
                }
            }
            _ => {
                if url.starts_with("/music/") || url.starts_with("local://") {
                    self.probe_channel_music_metadata(url).unwrap_or(fallback)
                } else {
                    fallback
                }
            }
        }
    }

    pub(crate) fn build_music_queue_entry(
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
            "type": if resolved.live_stream { 1u32 } else { 0u32 },
            "url": url,
            "title": metadata_title,
            "description": metadata_description,
            "length": resolved.length_seconds.saturating_mul(1000),
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

    pub(crate) fn insert_music_queue_entry(
        bot: &mut MusicBot,
        entry: MusicQueueEntry,
        previous_song_id: u32,
    ) -> std::result::Result<(), QueryResponse> {
        let insert_index = if previous_song_id == 0 {
            bot.queue.len()
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

    pub(crate) fn remove_music_queue_entry(
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

    pub(crate) fn reorder_music_queue_entry(
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

    pub(crate) fn set_music_queue_current_song(
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
        crate::runtime::musicbot::audio_engine::request_bot_seek(bot.id, 0);
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

    pub(crate) fn sync_music_bot_client_properties(&mut self, bot_id: u32) -> Option<(u32, u64)> {
        let (server_id, linked_client_id, extra_properties) = self
            .store
            .music_bots
            .get(&bot_id)
            .map(|bot| {
                (
                    bot.server_id,
                    bot.linked_client_id,
                    Self::music_bot_extra_properties(bot),
                )
            })?;

        let client_id = linked_client_id?;
        let (client_nickname, client_channel_id) = self
            .store
            .online_clients
            .get_mut(&client_id)
            .filter(|client| client.server_id == server_id)
            .map(|client| {
                for (k, v) in extra_properties {
                    if matches!(
                        k.as_str(),
                        "client_type_exact"
                            | "player_state"
                            | "player_volume"
                            | "client_playlist_id"
                            | "_music_song_state"
                            | "_music_sync_state"
                            | "_music_loaded_songs"
                    ) || !client.extra_properties.contains_key(&k)
                    {
                        client.extra_properties.insert(k, v);
                    }
                }
                (client.nickname.clone(), client.channel_id)
            })?;

        if let Some(bot) = self.store.music_bots.get_mut(&bot_id) {
            if !client_nickname.trim().is_empty() {
                bot.nickname = client_nickname;
            }
            if client_channel_id != 0 {
                bot.channel_id = client_channel_id;
            }
        }

        Some((server_id, client_id))
    }

    pub(crate) fn sync_music_bot_client_state(&mut self, bot_id: u32) {
        let Some((server_id, Some(client_id))) = self
            .store
            .music_bots
            .get(&bot_id)
            .map(|bot| (bot.server_id, bot.linked_client_id))
        else {
            return;
        };

        let before_snap = self.online_client_snapshot(server_id, client_id);
        let _ = self.sync_music_bot_client_properties(bot_id);
        let after_snap = self.online_client_snapshot(server_id, client_id);

        if let (Some(before), Some(after)) = (before_snap, after_snap)
            && before != after
        {
            let notif = crate::transport::TransportNotification::ClientUpdated {
                server_id,
                before,
                after,
            };
            self.broadcast_event(server_id, &notif);
        }
    }

}
