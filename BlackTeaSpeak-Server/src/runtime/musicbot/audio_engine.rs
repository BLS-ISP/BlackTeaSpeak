use std::collections::HashMap;
use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::sync::{
    Arc, Mutex, OnceLock,
    atomic::{AtomicBool, Ordering},
};
use std::thread;
use std::time::{Duration, Instant};

use crate::runtime::ffmpeg::resolve_music_command;
use crate::runtime::musicbot::{MusicBotState, ResolvedMusicMetadata};
use crate::runtime::{ActiveMediaBroadcast, BaselineRuntime, TextMessageTarget};
use crate::transport::TransportNotification;

static BOT_SEEK_REQUESTS: OnceLock<Mutex<HashMap<u32, u32>>> = OnceLock::new();
static YOUTUBE_DIRECT_URL_CACHE: OnceLock<Mutex<HashMap<String, (String, Instant)>>> =
    OnceLock::new();
static YOUTUBE_VIDEO_DIRECT_URL_CACHE: OnceLock<Mutex<HashMap<String, (String, Instant)>>> =
    OnceLock::new();

pub fn cache_youtube_direct_url(song_url: &str, direct_url: String) {
    let trimmed = direct_url.trim().to_string();
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        let map = YOUTUBE_DIRECT_URL_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
        if let Ok(mut guard) = map.lock() {
            guard.insert(song_url.to_string(), (trimmed, Instant::now()));
        }
    }
}

pub fn get_cached_youtube_direct_url(song_url: &str) -> Option<String> {
    let map = YOUTUBE_DIRECT_URL_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = map.lock().ok()?;
    if let Some((direct_url, cached_at)) = guard.get(song_url) {
        if cached_at.elapsed() < Duration::from_secs(7200) {
            return Some(direct_url.clone());
        }
    }
    guard.remove(song_url);
    None
}

pub fn invalidate_youtube_direct_url(song_url: &str) {
    let map = YOUTUBE_DIRECT_URL_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(mut guard) = map.lock() {
        guard.remove(song_url);
    }
}

pub fn resolve_and_cache_youtube_direct_url(song_url: &str) -> Option<String> {
    let ytdlp_bin = resolve_music_command("TEASPEAK_COMPAT_YTDLP", &["yt-dlp", "youtube-dl"])?;
    let out = Command::new(&ytdlp_bin)
        .args([
            "--js-runtimes",
            "node",
            "--no-playlist",
            "--no-warnings",
            "-q",
            "-f",
            "140/bestaudio[ext=m4a]/bestaudio/best",
            "-g",
            song_url,
        ])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let resolved = String::from_utf8_lossy(&out.stdout)
        .lines()
        .next()
        .unwrap_or("")
        .trim()
        .to_string();
    if !resolved.is_empty() {
        cache_youtube_direct_url(song_url, resolved.clone());
        Some(resolved)
    } else {
        None
    }
}

pub fn cache_youtube_video_direct_url(song_url: &str, direct_url: String) {
    let trimmed = direct_url.trim().to_string();
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        let map = YOUTUBE_VIDEO_DIRECT_URL_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
        if let Ok(mut guard) = map.lock() {
            guard.insert(song_url.to_string(), (trimmed, Instant::now()));
        }
    }
}

pub fn get_cached_youtube_video_direct_url(song_url: &str) -> Option<String> {
    let map = YOUTUBE_VIDEO_DIRECT_URL_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = map.lock().ok()?;
    if let Some((direct_url, cached_at)) = guard.get(song_url) {
        if cached_at.elapsed() < Duration::from_secs(7200) {
            return Some(direct_url.clone());
        }
    }
    guard.remove(song_url);
    None
}

pub fn resolve_and_cache_youtube_video_direct_url(song_url: &str) -> Option<String> {
    let ytdlp_bin = resolve_music_command("TEASPEAK_COMPAT_YTDLP", &["yt-dlp", "youtube-dl"])?;
    let out = Command::new(&ytdlp_bin)
        .args([
            "--js-runtimes",
            "node",
            "--no-playlist",
            "--no-warnings",
            "-q",
            "-f",
            "bestvideo[height<=720][ext=mp4]/bestvideo[height<=720]/18/best[height<=720]/best",
            "-g",
            song_url,
        ])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let resolved = String::from_utf8_lossy(&out.stdout)
        .lines()
        .next()
        .unwrap_or("")
        .trim()
        .to_string();
    if !resolved.is_empty() {
        cache_youtube_video_direct_url(song_url, resolved.clone());
        Some(resolved)
    } else {
        None
    }
}

pub fn request_bot_seek(bot_id: u32, seek_secs: u32) {
    let map = BOT_SEEK_REQUESTS.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(mut guard) = map.lock() {
        guard.insert(bot_id, seek_secs);
    }
}

fn take_bot_seek_request(bot_id: u32) -> Option<u32> {
    let map = BOT_SEEK_REQUESTS.get_or_init(|| Mutex::new(HashMap::new()));
    map.lock().ok().and_then(|mut g| g.remove(&bot_id))
}

struct ActiveBotStream {
    song_id: u32,
    song_url: String,
    stop_flag: Arc<AtomicBool>,
}

pub fn spawn_music_bot_audio_engine(runtime: Arc<Mutex<BaselineRuntime>>) {
    thread::spawn(move || {
        let mut active_streams: HashMap<u32, ActiveBotStream> = HashMap::new();
        let mut probed_queued_songs: std::collections::HashSet<(u32, u32)> =
            std::collections::HashSet::new();

        loop {
            thread::sleep(Duration::from_millis(80));

            let (desired_bots, queued_youtube_to_probe): (
                Vec<(u32, u32, u64, u32, String, String, bool)>,
                Vec<(u32, u32, u64, u32, String)>,
            ) = {
                let Ok(rt) = runtime.lock() else {
                    continue;
                };
                let mut desired = Vec::new();
                let mut to_probe = Vec::new();
                for bot in rt.store.music_bots.values() {
                    let Some(client_id) = bot.linked_client_id else {
                        continue;
                    };
                    for song in &bot.queue {
                        if bot.current_song_id != Some(song.id)
                            && (song.url_loader == "youtube"
                                || song.url.contains("youtube.com/")
                                || song.url.contains("youtu.be/"))
                            && song.length_seconds == 0
                            && !probed_queued_songs.contains(&(bot.id, song.id))
                        {
                            to_probe.push((
                                bot.id,
                                bot.server_id,
                                client_id,
                                song.id,
                                song.url.clone(),
                            ));
                        }
                    }
                    if !matches!(bot.state, MusicBotState::Playing | MusicBotState::Paused) {
                        continue;
                    }
                    let Some(song_id) = bot.current_song_id else {
                        continue;
                    };
                    let Some(song) = bot.queue.iter().find(|s| s.id == song_id) else {
                        continue;
                    };
                    if song.url.trim().is_empty() {
                        continue;
                    }
                    desired.push((
                        bot.id,
                        bot.server_id,
                        client_id,
                        song_id,
                        song.url.clone(),
                        song.url_loader.clone(),
                        song.title.starts_with("http")
                            || song.title.starts_with("youtube")
                            || song.title.starts_with("watch")
                            || song.title.starts_with("⏳")
                            || song.title.starts_with("🔍")
                            || song.title.starts_with("📥")
                            || song.length_seconds == 0,
                    ));
                }
                (desired, to_probe)
            };

            for (bot_id, server_id, bot_clid, song_id, song_url) in queued_youtube_to_probe {
                probed_queued_songs.insert((bot_id, song_id));
                spawn_background_metadata_probe(
                    Arc::clone(&runtime),
                    bot_id,
                    server_id,
                    bot_clid,
                    song_id,
                    song_url,
                );
            }

            let desired_map: HashMap<u32, (u32, u64, u32, String, String, bool)> = desired_bots
                .into_iter()
                .map(|(bot_id, sid, clid, song_id, url, loader, needs_meta)| {
                    (bot_id, (sid, clid, song_id, url, loader, needs_meta))
                })
                .collect();

            let mut pending_seeks: HashMap<u32, u32> = HashMap::new();
            for &bot_id in desired_map.keys() {
                if let Some(seek_secs) = take_bot_seek_request(bot_id) {
                    pending_seeks.insert(bot_id, seek_secs);
                }
            }

            // Stop any stream whose bot stopped, was deleted, changed song_id/url, or requested a seek
            active_streams.retain(|bot_id, active| {
                if pending_seeks.contains_key(bot_id) {
                    active.stop_flag.store(true, Ordering::SeqCst);
                    return false;
                }
                if let Some((_, _, song_id, url, _, _)) = desired_map.get(bot_id) {
                    if *song_id == active.song_id && *url == active.song_url && !active.stop_flag.load(Ordering::Relaxed) {
                        return true;
                    }
                }
                active.stop_flag.store(true, Ordering::SeqCst);
                false
            });

            // Spawn new stream workers for playing bots that don't have an active worker
            for (bot_id, (server_id, client_id, song_id, song_url, url_loader, needs_meta)) in desired_map {
                if active_streams.contains_key(&bot_id) {
                    continue;
                }
                // Only start a new stream if the bot is in Playing state
                let Some(bot_resume_seek_secs) = ({
                    let Ok(rt) = runtime.lock() else {
                        continue;
                    };
                    rt.store.music_bots.get(&bot_id).and_then(|b| {
                        matches!(b.state, MusicBotState::Playing).then_some(
                            BaselineRuntime::music_bot_current_song_replay_millis(b) / 1000,
                        )
                    })
                }) else {
                    continue;
                };

                let seek_secs = pending_seeks
                    .remove(&bot_id)
                    .unwrap_or(bot_resume_seek_secs);
                let stop_flag = Arc::new(AtomicBool::new(false));
                active_streams.insert(
                    bot_id,
                    ActiveBotStream {
                        song_id,
                        song_url: song_url.clone(),
                        stop_flag: Arc::clone(&stop_flag),
                    },
                );

                let worker_rt = Arc::clone(&runtime);
                thread::spawn(move || {
                    run_bot_song_stream(
                        worker_rt,
                        bot_id,
                        server_id,
                        client_id,
                        song_id,
                        song_url,
                        url_loader,
                        needs_meta,
                        seek_secs,
                        stop_flag,
                    );
                });
            }
        }
    });
}

pub fn format_duration_mm_ss(total_secs: u32) -> String {
    if total_secs == 0 {
        return String::from("LIVE");
    }
    let hours = total_secs / 3600;
    let mins = (total_secs % 3600) / 60;
    let secs = total_secs % 60;
    if hours > 0 {
        format!("{hours}:{mins:02}:{secs:02}")
    } else {
        format!("{mins:02}:{secs:02}")
    }
}

pub fn spawn_background_metadata_probe(
    runtime: Arc<Mutex<BaselineRuntime>>,
    bot_id: u32,
    server_id: u32,
    bot_clid: u64,
    song_id: u32,
    song_url: String,
) {
    thread::spawn(move || {
        match BaselineRuntime::probe_youtube_music_metadata_detailed(&song_url) {
            Ok(meta) => {
                if let Ok(mut rt) = runtime.lock() {
                    let mut is_queued_not_playing = false;
                    if let Some(bot) = rt.store.music_bots.get_mut(&bot_id) {
                        is_queued_not_playing = bot.current_song_id != Some(song_id);
                        if let Some(song) = bot.queue.iter_mut().find(|s| s.id == song_id) {
                            song.title = meta.title.clone();
                            song.description = meta.description.clone();
                            song.thumbnail = meta.thumbnail.clone();
                            song.length_seconds = meta.length_seconds;
                            song.seekable = meta.seekable;
                            song.live_stream = meta.live_stream;
                            song.loaded = true;
                            song.metadata = serde_json::json!({
                                "type": if meta.live_stream { 1u32 } else { 0u32 },
                                "url": song.url,
                                "title": meta.title,
                                "description": meta.description,
                                "length": meta.length_seconds.saturating_mul(1000),
                                "thumbnail": if meta.thumbnail.trim().is_empty() { "none" } else { &meta.thumbnail },
                                "is_live": meta.live_stream,
                                "seekable": meta.seekable,
                            })
                            .to_string();
                        }
                    }
                    rt.sync_music_bot_client_state(bot_id);
                    if is_queued_not_playing {
                        let dur = format_duration_mm_ss(meta.length_seconds);
                        send_bot_channel_message_locked(
                            &mut rt,
                            server_id,
                            bot_clid,
                            &format!(
                                "✅ Warteschlangen-Info geladen: \"[b]{}[/b]\" [Dauer: {}] ist bereit.",
                                meta.title, dur
                            ),
                        );
                    }
                }
            }
            Err(err_reason) => {
                if let Ok(mut rt) = runtime.lock() {
                    send_bot_channel_message_locked(
                        &mut rt,
                        server_id,
                        bot_clid,
                        &format!(
                            "⚠️ Hinweis zur Warteschlange: YouTube-Link konnte nicht geprüft werden ({})",
                            err_reason
                        ),
                    );
                }
            }
        }
    });
}

#[allow(clippy::too_many_arguments)]
fn run_bot_song_stream(
    runtime: Arc<Mutex<BaselineRuntime>>,
    bot_id: u32,
    server_id: u32,
    client_id: u64,
    song_id: u32,
    song_url: String,
    url_loader: String,
    needs_meta: bool,
    seek_secs: u32,
    stop_flag: Arc<AtomicBool>,
) {
    let load_started = Instant::now();
    let first_packet_received = Arc::new(AtomicBool::new(false));
    let is_youtube = url_loader == "youtube"
        || song_url.contains("youtube.com/")
        || song_url.contains("youtu.be/");

    // Stage 1 (t = 0.0s): Immediate feedback in GUI & Channel Chat (only on initial start, not on seek)
    if seek_secs == 0 {
        if let Ok(mut rt) = runtime.lock() {
            let mut initial_title = song_url.clone();
            if let Some(bot) = rt.store.music_bots.get_mut(&bot_id) {
                bot.current_song_started_at_millis = None;
                bot.current_song_progress_millis = 0;
                if let Some(song) = bot.queue.iter_mut().find(|s| s.id == song_id) {
                    if is_youtube && needs_meta {
                        song.title = String::from("⏳ [1/3] Prüfe YouTube-Link & lade Infos... (ca. 2–4s)");
                        song.description = format!("Lade YouTube-Metadaten für {}...", song_url);
                    } else if !song.title.starts_with("⏳") {
                        initial_title = song.title.clone();
                    }
                }
            }
            rt.sync_music_bot_client_state(bot_id);
            if is_youtube && needs_meta {
                send_bot_channel_message_locked(
                    &mut rt,
                    server_id,
                    client_id,
                    "🔍 [1/3] Prüfe YouTube-Link & lade Video-Infos... (geschätzte Dauer: ca. 2–4 Sek.)",
                );
            } else {
                send_bot_channel_message_locked(
                    &mut rt,
                    server_id,
                    client_id,
                    &format!(
                        "🔍 [1/2] Verbinde & puffere Audio-Stream für \"[b]{}[/b]\"... (ca. 1–2 Sek.)",
                        initial_title
                    ),
                );
            }
        }
    }

    // Stage 2 (parallel): Resolve YouTube metadata concurrently while yt-dlp | ffmpeg warms up
    if is_youtube && needs_meta && seek_secs == 0 && !stop_flag.load(Ordering::Relaxed) {
        let meta_rt = Arc::clone(&runtime);
        let meta_stop = Arc::clone(&stop_flag);
        let meta_first_pkt = Arc::clone(&first_packet_received);
        let meta_url = song_url.clone();
        thread::spawn(move || {
            match BaselineRuntime::probe_youtube_music_metadata_detailed(&meta_url) {
                Ok(meta) => {
                    if meta_stop.load(Ordering::Relaxed) {
                        return;
                    }
                    let already_playing = meta_first_pkt.load(Ordering::Relaxed);
                    apply_resolved_metadata(
                        &meta_rt,
                        bot_id,
                        server_id,
                        client_id,
                        song_id,
                        &meta,
                        already_playing,
                    );
                }
                Err(err_reason) => {
                    if meta_stop.swap(true, Ordering::SeqCst) {
                        return;
                    }
                    if let Ok(mut rt) = meta_rt.lock() {
                        if let Some(bot) = rt.store.music_bots.get_mut(&bot_id) {
                            if bot.current_song_id == Some(song_id) {
                                bot.state = MusicBotState::Stopped;
                                BaselineRuntime::stop_music_bot_playback(bot);
                            }
                            if let Some(song) = bot.queue.iter_mut().find(|s| s.id == song_id) {
                                song.title = format!("❌ Fehler: {err_reason}");
                                song.description = format!("Fehler beim Laden: {err_reason}");
                            }
                        }
                        rt.sync_music_bot_client_state(bot_id);
                        send_bot_channel_message_locked(
                            &mut rt,
                            server_id,
                            client_id,
                            &format!(
                                "❌ YouTube-Link konnte nicht geladen werden: [b]{}[/b]",
                                err_reason
                            ),
                        );
                    }
                }
            }
        });
    }

    if stop_flag.load(Ordering::Relaxed) {
        return;
    }

    if is_youtube && !needs_meta && seek_secs == 0 && get_cached_youtube_direct_url(&song_url).is_none() {
        let warm_url = song_url.clone();
        thread::spawn(move || {
            let _ = resolve_and_cache_youtube_direct_url(&warm_url);
        });
    }

    let had_cached_direct_url = is_youtube && get_cached_youtube_direct_url(&song_url).is_some();

    // Spawn ffmpeg (and yt-dlp if YouTube) to produce 20ms Ogg Opus pages on stdout
    let Some((mut ytdlp_child, mut ffmpeg_child)) =
        spawn_opus_pipeline(&runtime, bot_id, &song_url, is_youtube, seek_secs)
    else {
        notify_bot_channel_chat(
            &runtime,
            server_id,
            client_id,
            &format!("❌ Konnte Audio-Pipeline (yt-dlp/ffmpeg) nicht starten für: {}", song_url),
        );
        stop_flag.store(true, Ordering::SeqCst);
        return;
    };

    let Some(mut stdout) = ffmpeg_child.stdout.take() else {
        kill_children(&mut ytdlp_child, &mut ffmpeg_child);
        stop_flag.store(true, Ordering::SeqCst);
        return;
    };

    if is_video_candidate(&song_url, is_youtube) {
        spawn_bot_video_stream(
            Arc::clone(&runtime),
            bot_id,
            server_id,
            client_id,
            song_id,
            song_url.clone(),
            is_youtube,
            seek_secs,
            Arc::clone(&stop_flag),
            Arc::clone(&first_packet_received),
        );
    }

    let mut ogg_reader = OggOpusPacketReader::new();
    let mut read_buf = [0u8; 4096];
    let mut next_frame_deadline = Instant::now();
    let mut finished_naturally = false;

    'stream_loop: loop {
        if stop_flag.load(Ordering::Relaxed) {
            break;
        }

        // Check if bot is paused or stopped
        let bot_state = {
            let Ok(rt) = runtime.lock() else {
                break;
            };
            rt.store.music_bots.get(&bot_id).and_then(|b| {
                if b.current_song_id != Some(song_id) {
                    None
                } else {
                    Some(b.state.clone())
                }
            })
        };

        match bot_state {
            None | Some(MusicBotState::Stopped) => break 'stream_loop,
            Some(MusicBotState::Paused) => {
                set_bot_talking(&runtime, server_id, client_id, false);
                while !stop_flag.load(Ordering::Relaxed) {
                    thread::sleep(Duration::from_millis(60));
                    let resumed = {
                        let Ok(rt) = runtime.lock() else {
                            break 'stream_loop;
                        };
                        rt.store.music_bots.get(&bot_id).and_then(|b| {
                            if b.current_song_id != Some(song_id) {
                                None
                            } else {
                                Some(b.state.clone())
                            }
                        })
                    };
                    match resumed {
                        Some(MusicBotState::Playing) => {
                            if first_packet_received.load(Ordering::Relaxed) {
                                set_bot_talking(&runtime, server_id, client_id, true);
                            }
                            next_frame_deadline = Instant::now();
                            break;
                        }
                        Some(MusicBotState::Paused) => continue,
                        _ => break 'stream_loop,
                    }
                }
            }
            Some(MusicBotState::Playing) => {}
        }

        while let Some(opus_packet) = ogg_reader.next_packet() {
            if stop_flag.load(Ordering::Relaxed) {
                break 'stream_loop;
            }

            // Stage 3: Announce when the very first Opus audio frame is ready!
            if !first_packet_received.swap(true, Ordering::SeqCst) {
                let load_secs = load_started.elapsed().as_secs_f32();
                next_frame_deadline = Instant::now();
                set_bot_talking(&runtime, server_id, client_id, true);
                if let Ok(mut rt) = runtime.lock() {
                    let mut clean_title = song_url.clone();
                    let mut duration_secs = 0u32;
                    let mut vol_str = String::from("100");
                    if let Some(bot) = rt.store.music_bots.get_mut(&bot_id) {
                        bot.current_song_started_at_millis =
                            Some(crate::runtime::current_unix_timestamp_millis());
                        bot.current_song_progress_millis = seek_secs.saturating_mul(1000);
                        vol_str = BaselineRuntime::format_music_bot_volume_percent(&bot.player_volume).to_string();
                        if let Some(song) = bot.queue.iter_mut().find(|s| s.id == song_id) {
                            if let Ok(parsed) =
                                serde_json::from_str::<serde_json::Value>(&song.metadata)
                                && let Some(meta_title) =
                                    parsed.get("title").and_then(|v| v.as_str())
                                && !meta_title.is_empty()
                                && !meta_title.starts_with("⏳")
                            {
                                song.title = meta_title.to_string();
                            }
                            clean_title = song.title.clone();
                            duration_secs = song.length_seconds;
                            song.description = format!(
                                "▶️ Spielt: {} [{}]",
                                clean_title,
                                format_duration_mm_ss(duration_secs)
                            );
                        }
                    }
                    rt.sync_music_bot_client_state(bot_id);
                    if seek_secs == 0 {
                        let dur_label = format_duration_mm_ss(duration_secs);
                        let step_prefix = if is_youtube { "[3/3]" } else { "[2/2]" };
                        send_bot_channel_message_locked(
                            &mut rt,
                            server_id,
                            client_id,
                            &format!(
                                "▶️ {} Spielt jetzt: \"[b]{}[/b]\" [00:00 / {}] — Lautstärke: {}% (geladen in {:.1} Sek.)",
                                step_prefix, clean_title, dur_label, vol_str, load_secs
                            ),
                        );
                    }
                }
            }

            // Pace at 20ms per Opus frame
            let now = Instant::now();
            if next_frame_deadline > now {
                thread::sleep(next_frame_deadline - now);
            }
            next_frame_deadline = next_frame_deadline
                .checked_add(Duration::from_millis(20))
                .unwrap_or_else(Instant::now);
            // Prevent burst catch-up if paused or delayed by more than 100ms
            if Instant::now().saturating_duration_since(next_frame_deadline) > Duration::from_millis(100) {
                next_frame_deadline = Instant::now() + Duration::from_millis(20);
            }

            if let Ok(rt) = runtime.lock() {
                if let Some(bot) = rt.store.music_bots.get(&bot_id) {
                    if !matches!(bot.state, MusicBotState::Playing) || bot.current_song_id != Some(song_id) {
                        continue 'stream_loop;
                    }
                    rt.route_btea_media_to_webtransport(server_id, client_id, 1, &opus_packet);
                    rt.route_btea_media_to_desktop(server_id, client_id, 0x0A, &opus_packet);
                } else {
                    break 'stream_loop;
                }
            }
        }

        match stdout.read(&mut read_buf) {
            Ok(0) => {
                finished_naturally = true;
                break 'stream_loop;
            }
            Ok(n) => {
                ogg_reader.push_bytes(&read_buf[..n]);
            }
            Err(_) => {
                break 'stream_loop;
            }
        }
    }

    let had_audio = first_packet_received.load(Ordering::Relaxed);
    let was_aborted = stop_flag.swap(true, Ordering::SeqCst);
    kill_children(&mut ytdlp_child, &mut ffmpeg_child);
    set_bot_talking(&runtime, server_id, client_id, false);
    set_bot_video_broadcast(&runtime, server_id, client_id, false);

    if finished_naturally && had_audio {
        advance_bot_queue_on_eof(&runtime, bot_id, song_id);
    } else if !had_audio && !was_aborted {
        if had_cached_direct_url {
            invalidate_youtube_direct_url(&song_url);
            request_bot_seek(bot_id, seek_secs);
            return;
        }
        if let Ok(mut rt) = runtime.lock() {
            if let Some(bot) = rt.store.music_bots.get_mut(&bot_id) {
                if bot.current_song_id == Some(song_id) {
                    bot.state = MusicBotState::Stopped;
                    BaselineRuntime::stop_music_bot_playback(bot);
                }
                if let Some(song) = bot.queue.iter_mut().find(|s| s.id == song_id) {
                    song.title = String::from("❌ Fehler: Stream konnte nicht gestartet werden");
                }
            }
            rt.sync_music_bot_client_state(bot_id);
            send_bot_channel_message_locked(
                &mut rt,
                server_id,
                client_id,
                "❌ Audio-Stream konnte nicht dekodiert werden. Bitte prüfe, ob der Link gültig und nicht gesperrt ist.",
            );
        }
    }
}

fn spawn_opus_pipeline(
    runtime: &Arc<Mutex<BaselineRuntime>>,
    bot_id: u32,
    song_url: &str,
    is_youtube: bool,
    seek_secs: u32,
) -> Option<(Option<Child>, Child)> {
    let ffmpeg_bin = resolve_music_command("TEASPEAK_COMPAT_FFMPEG", &["ffmpeg"])?;
    let vol_factor: f32 = runtime
        .lock()
        .ok()
        .and_then(|rt| {
            rt.store
                .music_bots
                .get(&bot_id)
                .map(|b| BaselineRuntime::parse_music_bot_volume_factor(&b.player_volume))
        })
        .unwrap_or(1.0);
    let vol_filter = format!("volume={vol_factor:.2}");
    let seek_arg = seek_secs.to_string();

    if is_youtube {
        let ytdlp_bin = resolve_music_command("TEASPEAK_COMPAT_YTDLP", &["yt-dlp", "youtube-dl"])?;

        // Check cached direct M4A stream URL first; if seeking and not cached yet, resolve via `yt-dlp -g`
        // so ffmpeg can perform an instant HTTP Range seek (`-ss`) via the MP4 sidx index.
        let mut direct_url_opt = get_cached_youtube_direct_url(song_url);
        if direct_url_opt.is_none() && seek_secs > 0 {
            direct_url_opt = resolve_and_cache_youtube_direct_url(song_url);
        }

        if let Some(direct_url) = direct_url_opt {
            let mut ff_args = vec![
                String::from("-hide_banner"),
                String::from("-loglevel"),
                String::from("quiet"),
                String::from("-reconnect"),
                String::from("1"),
                String::from("-reconnect_streamed"),
                String::from("1"),
                String::from("-reconnect_delay_max"),
                String::from("2"),
            ];
            if seek_secs > 0 {
                ff_args.push(String::from("-ss"));
                ff_args.push(seek_arg.clone());
            }
            ff_args.extend([
                String::from("-i"),
                direct_url,
                String::from("-vn"),
                String::from("-af"),
                vol_filter.clone(),
                String::from("-ac"),
                String::from("2"),
                String::from("-ar"),
                String::from("48000"),
                String::from("-c:a"),
                String::from("libopus"),
                String::from("-b:a"),
                String::from("64k"),
                String::from("-vbr"),
                String::from("on"),
                String::from("-application"),
                String::from("audio"),
                String::from("-frame_duration"),
                String::from("20"),
                String::from("-page_duration"),
                String::from("20000"),
                String::from("-f"),
                String::from("ogg"),
                String::from("pipe:1"),
            ]);
            if let Ok(ffmpeg_child) = Command::new(&ffmpeg_bin)
                .args(&ff_args)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
            {
                return Some((None, ffmpeg_child));
            }
        }

        let mut ytdlp_child = Command::new(ytdlp_bin)
            .args([
                "--js-runtimes",
                "node",
                "--no-playlist",
                "--no-warnings",
                "-q",
                "-f",
                "140/bestaudio[ext=m4a]/bestaudio/best",
                "-o",
                "-",
                song_url,
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;

        let ytdlp_stdout = ytdlp_child.stdout.take()?;
        let mut ff_args = vec![
            String::from("-hide_banner"),
            String::from("-loglevel"),
            String::from("quiet"),
        ];
        if seek_secs > 0 {
            ff_args.push(String::from("-ss"));
            ff_args.push(seek_arg);
        }
        ff_args.extend([
            String::from("-i"),
            String::from("pipe:0"),
            String::from("-vn"),
            String::from("-af"),
            vol_filter,
            String::from("-ac"),
            String::from("2"),
            String::from("-ar"),
            String::from("48000"),
            String::from("-c:a"),
            String::from("libopus"),
            String::from("-b:a"),
            String::from("64k"),
            String::from("-vbr"),
            String::from("on"),
            String::from("-application"),
            String::from("audio"),
            String::from("-frame_duration"),
            String::from("20"),
            String::from("-page_duration"),
            String::from("20000"),
            String::from("-f"),
            String::from("ogg"),
            String::from("pipe:1"),
        ]);
        let ffmpeg_child = Command::new(ffmpeg_bin)
            .args(&ff_args)
            .stdin(Stdio::from(ytdlp_stdout))
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;

        Some((Some(ytdlp_child), ffmpeg_child))
    } else {
        let resolved_input = resolve_local_or_remote_input(runtime, song_url);
        let mut ff_args = vec![
            String::from("-hide_banner"),
            String::from("-loglevel"),
            String::from("quiet"),
        ];
        if seek_secs > 0 {
            ff_args.push(String::from("-ss"));
            ff_args.push(seek_arg);
        }
        ff_args.extend([
            String::from("-i"),
            resolved_input,
            String::from("-vn"),
            String::from("-af"),
            vol_filter,
            String::from("-ac"),
            String::from("2"),
            String::from("-ar"),
            String::from("48000"),
            String::from("-c:a"),
            String::from("libopus"),
            String::from("-b:a"),
            String::from("64k"),
            String::from("-vbr"),
            String::from("on"),
            String::from("-application"),
            String::from("audio"),
            String::from("-frame_duration"),
            String::from("20"),
            String::from("-page_duration"),
            String::from("20000"),
            String::from("-f"),
            String::from("ogg"),
            String::from("pipe:1"),
        ]);
        let ffmpeg_child = Command::new(ffmpeg_bin)
            .args(&ff_args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;

        Some((None, ffmpeg_child))
    }
}

fn resolve_local_or_remote_input(runtime: &Arc<Mutex<BaselineRuntime>>, song_url: &str) -> String {
    if let Ok(rt) = runtime.lock()
        && let Some(path) = rt.resolve_local_music_file_path(song_url)
    {
        return path.to_string_lossy().to_string();
    }
    song_url.to_string()
}

fn kill_children(ytdlp: &mut Option<Child>, ffmpeg: &mut Child) {
    let _ = ffmpeg.kill();
    let _ = ffmpeg.wait();
    if let Some(yt) = ytdlp.as_mut() {
        let _ = yt.kill();
        let _ = yt.wait();
    }
}

fn set_bot_talking(
    runtime: &Arc<Mutex<BaselineRuntime>>,
    server_id: u32,
    client_id: u64,
    is_talking: bool,
) {
    if let Ok(rt) = runtime.lock() {
        let Some(c) = rt.online_client_snapshot(server_id, client_id) else {
            return;
        };
        if !is_talking {
            rt.route_btea_media_to_webtransport(server_id, client_id, 1, &[]);
        }
        rt.broadcast_event(
            server_id,
            &TransportNotification::TalkStatus {
                server_id,
                channel_id: c.channel_id,
                client_id,
                is_talking,
                is_whisper: false,
                whisper_targets: None,
            },
        );
    }
}

fn apply_resolved_metadata(
    runtime: &Arc<Mutex<BaselineRuntime>>,
    bot_id: u32,
    server_id: u32,
    client_id: u64,
    song_id: u32,
    meta: &ResolvedMusicMetadata,
    already_playing: bool,
) {
    let Ok(mut rt) = runtime.lock() else {
        return;
    };
    let dur_str = format_duration_mm_ss(meta.length_seconds);
    let mut updated_title = None;
    if let Some(bot) = rt.store.music_bots.get_mut(&bot_id) {
        if let Some(song) = bot.queue.iter_mut().find(|s| s.id == song_id) {
            song.title = meta.title.clone();
            song.description = meta.description.clone();
            song.thumbnail = meta.thumbnail.clone();
            song.length_seconds = meta.length_seconds;
            song.seekable = meta.seekable;
            song.live_stream = meta.live_stream;
            song.loaded = true;
            song.metadata = serde_json::json!({
                "type": if meta.live_stream { 1u32 } else { 0u32 },
                "url": song.url,
                "title": meta.title,
                "description": meta.description,
                "length": meta.length_seconds.saturating_mul(1000),
                "thumbnail": if meta.thumbnail.trim().is_empty() { "none" } else { &meta.thumbnail },
                "is_live": meta.live_stream,
                "seekable": meta.seekable,
            })
            .to_string();
            updated_title = Some(meta.title.clone());
        }
    }
    if let Some(title) = updated_title {
        rt.sync_music_bot_client_state(bot_id);
        let msg = if already_playing {
            format!(
                "ℹ️ Video-Metadaten geladen: \"[b]{}[/b]\" [Dauer: {}]",
                title, dur_str
            )
        } else {
            format!(
                "📥 [2/3] Video gefunden: \"[b]{}[/b]\" [Dauer: {}] — Extrahiere & puffere Audio-Stream (Start in ca. 1 Sek.)...",
                title, dur_str
            )
        };
        send_bot_channel_message_locked(&mut rt, server_id, client_id, &msg);
    }
}

fn advance_bot_queue_on_eof(runtime: &Arc<Mutex<BaselineRuntime>>, bot_id: u32, finished_song_id: u32) {
    let Ok(mut rt) = runtime.lock() else {
        return;
    };
    let mut state_changed = false;
    if let Some(bot) = rt.store.music_bots.get_mut(&bot_id) {
        if bot.current_song_id == Some(finished_song_id) && matches!(bot.state, MusicBotState::Playing) {
            let next_song_id = match bot.playlist_replay_mode {
                // 2 = Repeat One (single track loop)
                2 => Some(finished_song_id),
                // 1 = Repeat All (queue loop)
                1 => bot
                    .queue
                    .iter()
                    .position(|s| s.id == finished_song_id)
                    .and_then(|idx| bot.queue.get(idx + 1).or_else(|| bot.queue.first()))
                    .map(|s| s.id),
                // 0 = Off (stop after last song)
                _ => bot
                    .queue
                    .iter()
                    .position(|s| s.id == finished_song_id)
                    .and_then(|idx| bot.queue.get(idx + 1))
                    .map(|s| s.id),
            };
            if let Some(next_id) = next_song_id {
                bot.current_song_id = Some(next_id);
                bot.state = MusicBotState::Playing;
                BaselineRuntime::reset_music_bot_playback_progress(bot);
            } else {
                bot.state = MusicBotState::Stopped;
                BaselineRuntime::stop_music_bot_playback(bot);
            }
            state_changed = true;
        }
    }
    if state_changed {
        rt.sync_music_bot_client_state(bot_id);
    }
}

fn notify_bot_channel_chat(
    runtime: &Arc<Mutex<BaselineRuntime>>,
    server_id: u32,
    bot_client_id: u64,
    text: &str,
) {
    if let Ok(mut rt) = runtime.lock() {
        send_bot_channel_message_locked(&mut rt, server_id, bot_client_id, text);
    }
}

pub(crate) fn send_bot_channel_message_locked(
    rt: &mut BaselineRuntime,
    server_id: u32,
    bot_client_id: u64,
    text: &str,
) {
    let Some(bot_client) = rt.store.online_clients.get(&bot_client_id) else {
        return;
    };
    let bot_cid = bot_client.channel_id;
    let bot_name = bot_client.nickname.clone();
    let bot_uid = bot_client.unique_identifier.clone();
    let bot_dbid = bot_client.database_id;

    let target = TextMessageTarget {
        target_mode: 2,
        server_id,
        channel_id: Some(bot_cid),
        target_client_id: None,
        message: text.to_string(),
    };
    rt.record_text_message(&target, bot_dbid, bot_uid.clone(), bot_name.clone());
    rt.broadcast_event(
        server_id,
        &TransportNotification::TextMessage {
            target,
            invoker_id: bot_client_id,
            invoker_name: bot_name,
            invoker_uid: bot_uid,
        },
    );
}

/// Minimal, zero-dependency Ogg bitstream reader that extracts raw Opus packets.
struct OggOpusPacketReader {
    buf: Vec<u8>,
    packets: std::collections::VecDeque<Vec<u8>>,
    partial_packet: Vec<u8>,
}

impl OggOpusPacketReader {
    fn new() -> Self {
        Self {
            buf: Vec::with_capacity(16384),
            packets: std::collections::VecDeque::new(),
            partial_packet: Vec::new(),
        }
    }

    fn push_bytes(&mut self, data: &[u8]) {
        self.buf.extend_from_slice(data);
        self.parse_pages();
    }

    fn next_packet(&mut self) -> Option<Vec<u8>> {
        self.packets.pop_front()
    }

    fn parse_pages(&mut self) {
        loop {
            if self.buf.len() < 27 {
                break;
            }
            // Locate "OggS" sync pattern
            if &self.buf[0..4] != b"OggS" {
                if let Some(pos) = self.buf.windows(4).position(|w| w == b"OggS") {
                    self.buf.drain(0..pos);
                    if self.buf.len() < 27 {
                        break;
                    }
                } else {
                    let keep = self.buf.len().min(3);
                    let drain_len = self.buf.len() - keep;
                    self.buf.drain(0..drain_len);
                    break;
                }
            }

            let page_segments = self.buf[26] as usize;
            let header_len = 27 + page_segments;
            if self.buf.len() < header_len {
                break;
            }

            let body_len: usize = self.buf[27..header_len].iter().map(|&b| b as usize).sum();
            let total_page_len = header_len + body_len;
            if self.buf.len() < total_page_len {
                break;
            }

            let mut offset = header_len;
            for i in 0..page_segments {
                let seg_len = self.buf[27 + i] as usize;
                self.partial_packet
                    .extend_from_slice(&self.buf[offset..offset + seg_len]);
                offset += seg_len;

                if seg_len < 255 {
                    let pkt = std::mem::take(&mut self.partial_packet);
                    if !pkt.is_empty()
                        && !pkt.starts_with(b"OpusHead")
                        && !pkt.starts_with(b"OpusTags")
                    {
                        self.packets.push_back(pkt);
                    }
                }
            }

            self.buf.drain(0..total_page_len);
        }
    }
}

pub(crate) fn is_video_candidate(song_url: &str, is_youtube: bool) -> bool {
    if is_youtube {
        return true;
    }
    let clean = song_url
        .split('?')
        .next()
        .unwrap_or(song_url)
        .split('#')
        .next()
        .unwrap_or(song_url)
        .trim();
    let ext = std::path::Path::new(clean)
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase());
    match ext.as_deref() {
        Some("mp3" | "ogg" | "opus" | "wav" | "flac" | "m4a" | "aac") => false,
        _ => true,
    }
}

fn spawn_bot_video_stream(
    runtime: Arc<Mutex<BaselineRuntime>>,
    bot_id: u32,
    server_id: u32,
    client_id: u64,
    song_id: u32,
    song_url: String,
    is_youtube: bool,
    seek_secs: u32,
    stop_flag: Arc<AtomicBool>,
    audio_started_flag: Arc<AtomicBool>,
) {
    thread::spawn(move || {
        let Some((mut ytdlp_child, mut ffmpeg_child)) =
            spawn_vp8_ivf_pipeline(&runtime, &song_url, is_youtube, seek_secs)
        else {
            return;
        };

        let Some(mut stdout) = ffmpeg_child.stdout.take() else {
            kill_children(&mut ytdlp_child, &mut ffmpeg_child);
            return;
        };

        let mut ivf_reader = IvfVp8FrameReader::new();
        let mut read_buf = [0u8; 16384];
        let mut broadcast_registered = false;
        let mut frame_index: u64 = 0;
        let mut next_frame_deadline = Instant::now();

        'video_loop: loop {
            if stop_flag.load(Ordering::Relaxed) {
                break;
            }

            let bot_state = {
                let Ok(rt) = runtime.lock() else {
                    break;
                };
                rt.store.music_bots.get(&bot_id).and_then(|b| {
                    if b.current_song_id != Some(song_id) {
                        None
                    } else {
                        Some(b.state.clone())
                    }
                })
            };

            match bot_state {
                None | Some(MusicBotState::Stopped) => break 'video_loop,
                Some(MusicBotState::Paused) => {
                    while !stop_flag.load(Ordering::Relaxed) {
                        thread::sleep(Duration::from_millis(60));
                        let resumed = {
                            let Ok(rt) = runtime.lock() else {
                                break 'video_loop;
                            };
                            rt.store.music_bots.get(&bot_id).and_then(|b| {
                                if b.current_song_id != Some(song_id) {
                                    None
                                } else {
                                    Some(b.state.clone())
                                }
                            })
                        };
                        match resumed {
                            Some(MusicBotState::Playing) => {
                                next_frame_deadline = Instant::now();
                                break;
                            }
                            Some(MusicBotState::Paused) => continue,
                            _ => break 'video_loop,
                        }
                    }
                }
                Some(MusicBotState::Playing) => {}
            }

            while let Some((width, height, vp8_frame)) = ivf_reader.next_frame() {
                if stop_flag.load(Ordering::Relaxed) {
                    break 'video_loop;
                }
                if vp8_frame.is_empty() {
                    continue;
                }

                if !broadcast_registered {
                    // Wait briefly for the audio pipeline's first packet so A/V start in sync
                    let sync_wait_start = Instant::now();
                    while !audio_started_flag.load(Ordering::Relaxed)
                        && !stop_flag.load(Ordering::Relaxed)
                        && sync_wait_start.elapsed() < Duration::from_secs(4)
                    {
                        thread::sleep(Duration::from_millis(15));
                    }
                    if stop_flag.load(Ordering::Relaxed) {
                        break 'video_loop;
                    }
                    broadcast_registered = true;
                    set_bot_video_broadcast(&runtime, server_id, client_id, true);
                    next_frame_deadline = Instant::now();
                }

                // Pace at 25 fps (40ms per video frame)
                let now = Instant::now();
                if next_frame_deadline > now {
                    thread::sleep(next_frame_deadline - now);
                }
                next_frame_deadline = next_frame_deadline
                    .checked_add(Duration::from_millis(40))
                    .unwrap_or_else(Instant::now);
                if Instant::now().saturating_duration_since(next_frame_deadline)
                    > Duration::from_millis(160)
                {
                    next_frame_deadline = Instant::now() + Duration::from_millis(40);
                }

                frame_index = frame_index.wrapping_add(1);
                // In RFC 6386 VP8 frame tag (byte 0), bit 0 == 0 indicates a keyframe
                let is_keyframe = (vp8_frame[0] & 0x01) == 0;
                let flags: u8 = if is_keyframe { 0x01 } else { 0x00 };
                let timestamp_us: f64 = (frame_index as f64) * 40_000.0;

                let mut btea_frame = Vec::with_capacity(13 + vp8_frame.len());
                btea_frame.push(flags);
                btea_frame.extend_from_slice(&width.to_le_bytes());
                btea_frame.extend_from_slice(&height.to_le_bytes());
                btea_frame.extend_from_slice(&timestamp_us.to_le_bytes());
                btea_frame.extend_from_slice(&vp8_frame);

                let rtc_mgr_opt = if let Ok(rt) = runtime.lock() {
                    if let Some(bot) = rt.store.music_bots.get(&bot_id) {
                        if !matches!(bot.state, MusicBotState::Playing)
                            || bot.current_song_id != Some(song_id)
                        {
                            continue 'video_loop;
                        }
                        // Route BTEA screen/video frame (packet_type = 4) to BlackTeaWeb clients
                        rt.route_btea_media_to_webtransport(server_id, client_id, 4, &btea_frame);
                        Some(Arc::clone(&rt.rtc_manager))
                    } else {
                        break 'video_loop;
                    }
                } else {
                    None
                };

                // Forward VP8 RTP packets to any subscribed TeaSpeak Desktop Clients
                if let Some(rtc_mgr) = rtc_mgr_opt {
                    rtc_mgr.forward_web_vp8_frame_blocking(client_id, 1, &btea_frame);
                }
            }

            match stdout.read(&mut read_buf) {
                Ok(0) => break 'video_loop,
                Ok(n) => ivf_reader.push_bytes(&read_buf[..n]),
                Err(_) => break 'video_loop,
            }
        }

        kill_children(&mut ytdlp_child, &mut ffmpeg_child);
        if broadcast_registered {
            set_bot_video_broadcast(&runtime, server_id, client_id, false);
        }
    });
}

fn spawn_vp8_ivf_pipeline(
    runtime: &Arc<Mutex<BaselineRuntime>>,
    song_url: &str,
    is_youtube: bool,
    seek_secs: u32,
) -> Option<(Option<Child>, Child)> {
    let ffmpeg_bin = resolve_music_command("TEASPEAK_COMPAT_FFMPEG", &["ffmpeg"])?;
    let seek_arg = seek_secs.to_string();
    let vf_arg = String::from(
        "scale=854:480:force_original_aspect_ratio=decrease,pad=854:480:(ow-iw)/2:(oh-ih)/2,fps=25",
    );

    if is_youtube {
        let ytdlp_bin = resolve_music_command("TEASPEAK_COMPAT_YTDLP", &["yt-dlp", "youtube-dl"])?;

        let mut direct_url_opt = get_cached_youtube_video_direct_url(song_url);
        if direct_url_opt.is_none() {
            direct_url_opt = resolve_and_cache_youtube_video_direct_url(song_url);
        }

        if let Some(direct_url) = direct_url_opt {
            let mut ff_args = vec![
                String::from("-hide_banner"),
                String::from("-loglevel"),
                String::from("quiet"),
                String::from("-reconnect"),
                String::from("1"),
                String::from("-reconnect_streamed"),
                String::from("1"),
                String::from("-reconnect_delay_max"),
                String::from("2"),
            ];
            if seek_secs > 0 {
                ff_args.push(String::from("-ss"));
                ff_args.push(seek_arg.clone());
            }
            ff_args.extend([
                String::from("-i"),
                direct_url,
                String::from("-an"),
                String::from("-vf"),
                vf_arg.clone(),
                String::from("-c:v"),
                String::from("libvpx"),
                String::from("-b:v"),
                String::from("750k"),
                String::from("-maxrate"),
                String::from("900k"),
                String::from("-bufsize"),
                String::from("1500k"),
                String::from("-deadline"),
                String::from("realtime"),
                String::from("-cpu-used"),
                String::from("6"),
                String::from("-threads"),
                String::from("2"),
                String::from("-g"),
                String::from("50"),
                String::from("-keyint_min"),
                String::from("25"),
                String::from("-auto-alt-ref"),
                String::from("0"),
                String::from("-pix_fmt"),
                String::from("yuv420p"),
                String::from("-f"),
                String::from("ivf"),
                String::from("pipe:1"),
            ]);
            if let Ok(ffmpeg_child) = Command::new(&ffmpeg_bin)
                .args(&ff_args)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
            {
                return Some((None, ffmpeg_child));
            }
        }

        let mut ytdlp_child = Command::new(ytdlp_bin)
            .args([
                "--js-runtimes",
                "node",
                "--no-playlist",
                "--no-warnings",
                "-q",
                "-f",
                "bestvideo[height<=720][ext=mp4]/bestvideo[height<=720]/18/best[height<=720]/best",
                "-o",
                "-",
                song_url,
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;

        let ytdlp_stdout = ytdlp_child.stdout.take()?;
        let mut ff_args = vec![
            String::from("-hide_banner"),
            String::from("-loglevel"),
            String::from("quiet"),
        ];
        if seek_secs > 0 {
            ff_args.push(String::from("-ss"));
            ff_args.push(seek_arg);
        }
        ff_args.extend([
            String::from("-i"),
            String::from("pipe:0"),
            String::from("-an"),
            String::from("-vf"),
            vf_arg,
            String::from("-c:v"),
            String::from("libvpx"),
            String::from("-b:v"),
            String::from("750k"),
            String::from("-maxrate"),
            String::from("900k"),
            String::from("-bufsize"),
            String::from("1500k"),
            String::from("-deadline"),
            String::from("realtime"),
            String::from("-cpu-used"),
            String::from("6"),
            String::from("-threads"),
            String::from("2"),
            String::from("-g"),
            String::from("50"),
            String::from("-keyint_min"),
            String::from("25"),
            String::from("-auto-alt-ref"),
            String::from("0"),
            String::from("-pix_fmt"),
            String::from("yuv420p"),
            String::from("-f"),
            String::from("ivf"),
            String::from("pipe:1"),
        ]);
        let ffmpeg_child = Command::new(ffmpeg_bin)
            .args(&ff_args)
            .stdin(Stdio::from(ytdlp_stdout))
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;

        Some((Some(ytdlp_child), ffmpeg_child))
    } else {
        let resolved_input = resolve_local_or_remote_input(runtime, song_url);
        let mut ff_args = vec![
            String::from("-hide_banner"),
            String::from("-loglevel"),
            String::from("quiet"),
        ];
        if seek_secs > 0 {
            ff_args.push(String::from("-ss"));
            ff_args.push(seek_arg);
        }
        ff_args.extend([
            String::from("-i"),
            resolved_input,
            String::from("-an"),
            String::from("-vf"),
            vf_arg,
            String::from("-c:v"),
            String::from("libvpx"),
            String::from("-b:v"),
            String::from("750k"),
            String::from("-maxrate"),
            String::from("900k"),
            String::from("-bufsize"),
            String::from("1500k"),
            String::from("-deadline"),
            String::from("realtime"),
            String::from("-cpu-used"),
            String::from("6"),
            String::from("-threads"),
            String::from("2"),
            String::from("-g"),
            String::from("50"),
            String::from("-keyint_min"),
            String::from("25"),
            String::from("-auto-alt-ref"),
            String::from("0"),
            String::from("-pix_fmt"),
            String::from("yuv420p"),
            String::from("-f"),
            String::from("ivf"),
            String::from("pipe:1"),
        ]);
        let ffmpeg_child = Command::new(ffmpeg_bin)
            .args(&ff_args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;

        Some((None, ffmpeg_child))
    }
}

fn set_bot_video_broadcast(
    runtime: &Arc<Mutex<BaselineRuntime>>,
    server_id: u32,
    client_id: u64,
    is_active: bool,
) {
    let Ok(mut rt) = runtime.lock() else {
        return;
    };
    let Some(channel_id) = rt
        .store
        .online_clients
        .get(&client_id)
        .filter(|c| c.server_id == server_id)
        .map(|c| c.channel_id)
    else {
        return;
    };

    let broadcast_type = 1_u32; // Screen/Video broadcast (`bt=1`)
    let key = (server_id, client_id, broadcast_type);

    if is_active {
        let ssrc = 0x7000_0000_u32
            .wrapping_add((client_id as u32).wrapping_mul(4))
            .wrapping_add(broadcast_type);
        let existing_joined = rt
            .store
            .active_broadcasts
            .get(&key)
            .map(|b| b.joined_client_ids.clone())
            .unwrap_or_default();
        rt.store.active_broadcasts.insert(
            key,
            ActiveMediaBroadcast {
                server_id,
                channel_id,
                broadcaster_client_id: client_id,
                broadcast_type,
                ssrc,
                joined_client_ids: existing_joined,
            },
        );
        if let Some(client) = rt.store.online_clients.get_mut(&client_id) {
            client.extra_properties.insert(
                String::from("client_flag_screen_broadcasting"),
                String::from("1"),
            );
        }
        rt.broadcast_event(
            server_id,
            &TransportNotification::VideoBroadcastChanged {
                server_id,
                channel_id,
            },
        );
    } else if rt.store.active_broadcasts.remove(&key).is_some() {
        if let Ok(mut lock) = rt.rtc_manager.viewer_slots.write() {
            for slots in lock.values_mut() {
                for slot in slots.iter_mut() {
                    if slot.assigned_broadcast == Some((client_id, broadcast_type)) {
                        slot.assigned_broadcast = None;
                    }
                }
            }
        }
        if let Some(client) = rt.store.online_clients.get_mut(&client_id) {
            client.extra_properties.insert(
                String::from("client_flag_screen_broadcasting"),
                String::from("0"),
            );
        }
        rt.broadcast_event(
            server_id,
            &TransportNotification::VideoBroadcastChanged {
                server_id,
                channel_id,
            },
        );
    }
}

/// Minimal, zero-dependency IVF bitstream reader (`-f ivf -c:v libvpx`) that extracts raw VP8 frames.
struct IvfVp8FrameReader {
    buf: Vec<u8>,
    header_parsed: bool,
    width: u16,
    height: u16,
    frames: std::collections::VecDeque<(u16, u16, Vec<u8>)>,
}

impl IvfVp8FrameReader {
    fn new() -> Self {
        Self {
            buf: Vec::with_capacity(65536),
            header_parsed: false,
            width: 854,
            height: 480,
            frames: std::collections::VecDeque::new(),
        }
    }

    fn push_bytes(&mut self, data: &[u8]) {
        self.buf.extend_from_slice(data);
        self.parse_frames();
    }

    fn next_frame(&mut self) -> Option<(u16, u16, Vec<u8>)> {
        self.frames.pop_front()
    }

    fn parse_frames(&mut self) {
        if !self.header_parsed {
            if self.buf.len() < 32 {
                return;
            }
            if &self.buf[0..4] != b"DKIF" {
                if let Some(pos) = self.buf.windows(4).position(|w| w == b"DKIF") {
                    self.buf.drain(0..pos);
                    if self.buf.len() < 32 {
                        return;
                    }
                } else {
                    let keep = self.buf.len().min(3);
                    let drain_len = self.buf.len() - keep;
                    self.buf.drain(0..drain_len);
                    return;
                }
            }
            let header_len = u16::from_le_bytes([self.buf[6], self.buf[7]]) as usize;
            let actual_header_len = header_len.max(32);
            if self.buf.len() < actual_header_len {
                return;
            }
            let w = u16::from_le_bytes([self.buf[12], self.buf[13]]);
            let h = u16::from_le_bytes([self.buf[14], self.buf[15]]);
            if w > 0 && h > 0 {
                self.width = w;
                self.height = h;
            }
            self.buf.drain(0..actual_header_len);
            self.header_parsed = true;
        }

        loop {
            if self.buf.len() < 12 {
                break;
            }
            let frame_size = u32::from_le_bytes([
                self.buf[0],
                self.buf[1],
                self.buf[2],
                self.buf[3],
            ]) as usize;
            if frame_size > 4 * 1024 * 1024 {
                // Corrupt IVF stream guard
                self.buf.clear();
                break;
            }
            let total_len = 12 + frame_size;
            if self.buf.len() < total_len {
                break;
            }
            let frame_data = self.buf[12..total_len].to_vec();
            self.buf.drain(0..total_len);
            if !frame_data.is_empty() {
                self.frames.push_back((self.width, self.height, frame_data));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_video_candidate_identifies_video_files_and_youtube() {
        assert!(is_video_candidate("https://www.youtube.com/watch?v=dQw4w9WgXcQ", true));
        assert!(is_video_candidate("local:clip.mp4", false));
        assert!(is_video_candidate("local:movie.mkv", false));
        assert!(is_video_candidate("local:capture.webm", false));
        assert!(!is_video_candidate("local:song.mp3", false));
        assert!(!is_video_candidate("local:voice.opus", false));
        assert!(!is_video_candidate("local:track.flac", false));
    }

    #[test]
    fn ivf_vp8_frame_reader_parses_header_and_frames() {
        let mut ivf_bytes = Vec::new();
        ivf_bytes.extend_from_slice(b"DKIF");
        ivf_bytes.extend_from_slice(&0u16.to_le_bytes()); // version
        ivf_bytes.extend_from_slice(&32u16.to_le_bytes()); // header len
        ivf_bytes.extend_from_slice(b"VP80");
        ivf_bytes.extend_from_slice(&854u16.to_le_bytes()); // width
        ivf_bytes.extend_from_slice(&480u16.to_le_bytes()); // height
        ivf_bytes.extend_from_slice(&25u32.to_le_bytes()); // fps num
        ivf_bytes.extend_from_slice(&1u32.to_le_bytes()); // fps den
        ivf_bytes.extend_from_slice(&2u32.to_le_bytes()); // num frames
        ivf_bytes.extend_from_slice(&0u32.to_le_bytes()); // unused

        // Frame 1 (keyframe: byte 0 bit 0 == 0)
        let payload1 = vec![0x10, 0x00, 0x9d, 0x01, 0x2a];
        ivf_bytes.extend_from_slice(&(payload1.len() as u32).to_le_bytes());
        ivf_bytes.extend_from_slice(&0u64.to_le_bytes());
        ivf_bytes.extend_from_slice(&payload1);

        // Frame 2 (interframe: byte 0 bit 0 == 1)
        let payload2 = vec![0x11, 0x00, 0xaa, 0xbb];
        ivf_bytes.extend_from_slice(&(payload2.len() as u32).to_le_bytes());
        ivf_bytes.extend_from_slice(&1u64.to_le_bytes());
        ivf_bytes.extend_from_slice(&payload2);

        let mut reader = IvfVp8FrameReader::new();
        reader.push_bytes(&ivf_bytes[..20]);
        assert!(reader.next_frame().is_none());
        reader.push_bytes(&ivf_bytes[20..]);

        let (w1, h1, f1) = reader.next_frame().expect("first frame");
        assert_eq!((w1, h1), (854, 480));
        assert_eq!(f1, payload1);
        assert_eq!(f1[0] & 0x01, 0);

        let (w2, h2, f2) = reader.next_frame().expect("second frame");
        assert_eq!((w2, h2), (854, 480));
        assert_eq!(f2, payload2);
        assert_eq!(f2[0] & 0x01, 1);

        assert!(reader.next_frame().is_none());
    }
}

