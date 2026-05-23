use std::process::Command;

use super::BaselineRuntime;
use super::ffmpeg::resolve_music_command;
use super::musicbot::ResolvedMusicMetadata;

impl BaselineRuntime {
    pub(super) fn probe_youtube_music_metadata(url: &str) -> Option<ResolvedMusicMetadata> {
        let ytdlp = resolve_music_command(
            "TEASPEAK_COMPAT_YTDLP",
            &["yt-dlp", "youtube-dl"],
        )?;
        let output = Command::new(ytdlp)
            .args([
                "--dump-single-json",
                "--no-warnings",
                "--no-playlist",
                "--skip-download",
                url,
            ])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }

        let payload = serde_json::from_slice::<serde_json::Value>(&output.stdout).ok()?;
        let title = payload
            .get("title")
            .and_then(|value| value.as_str())
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(url)
            .to_string();
        let description = payload
            .get("description")
            .and_then(|value| value.as_str())
            .unwrap_or(url)
            .to_string();
        let thumbnail = payload
            .get("thumbnail")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .to_string();
        let length_seconds = payload
            .get("duration")
            .and_then(|value| value.as_u64())
            .and_then(|value| u32::try_from(value).ok())
            .unwrap_or_default();
        let live_stream = payload
            .get("is_live")
            .and_then(|value| value.as_bool())
            .or_else(|| {
                payload
                    .get("live_status")
                    .and_then(|value| value.as_str())
                    .map(|value| value == "is_live" || value == "post_live")
            })
            .unwrap_or(false);

        Some(ResolvedMusicMetadata {
            loaded: true,
            title,
            description,
            thumbnail,
            length_seconds,
            seekable: !live_stream,
            live_stream,
        })
    }

    pub fn download_youtube_track(url: &str, output_path: &std::path::Path) -> bool {
        let ytdlp = match resolve_music_command(
            "TEASPEAK_COMPAT_YTDLP",
            &["yt-dlp", "youtube-dl"],
        ) {
            Some(path) => path,
            None => return false,
        };
        let output = Command::new(ytdlp)
            .args([
                "-x",
                "--audio-format",
                "mp3",
                "--no-playlist",
                "-o",
                output_path.to_string_lossy().as_ref(),
                url,
            ])
            .output()
            .ok();
        
        output.map(|o| o.status.success()).unwrap_or(false)
    }
}
