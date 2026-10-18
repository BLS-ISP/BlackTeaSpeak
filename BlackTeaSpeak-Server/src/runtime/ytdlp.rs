use std::process::Command;

use super::BaselineRuntime;
use super::ffmpeg::resolve_music_command;
use super::musicbot::ResolvedMusicMetadata;

impl BaselineRuntime {
    pub(super) fn probe_youtube_music_metadata(url: &str) -> Option<ResolvedMusicMetadata> {
        Self::probe_youtube_music_metadata_detailed(url).ok()
    }

    pub(crate) fn probe_youtube_music_metadata_detailed(
        url: &str,
    ) -> Result<ResolvedMusicMetadata, String> {
        let ytdlp = resolve_music_command("TEASPEAK_COMPAT_YTDLP", &["yt-dlp", "youtube-dl"])
            .ok_or_else(|| {
                String::from("yt-dlp wurde auf dem Server nicht gefunden (data/tools/yt-dlp.exe)")
            })?;
        let output = Command::new(ytdlp)
            .args([
                "--js-runtimes",
                "node",
                "-f",
                "140/bestaudio[ext=m4a]/bestaudio/best",
                "--dump-single-json",
                "--no-warnings",
                "--no-playlist",
                "--skip-download",
                url,
            ])
            .output()
            .map_err(|e| format!("yt-dlp Prozessfehler: {e}"))?;
        if !output.status.success() {
            let stderr_text = String::from_utf8_lossy(&output.stderr);
            let reason = stderr_text
                .lines()
                .rev()
                .map(str::trim)
                .find(|l| !l.is_empty())
                .unwrap_or("Ungültiger oder nicht verfügbarer YouTube-Link")
                .trim_start_matches("ERROR:")
                .trim();
            return Err(reason.to_string());
        }

        let payload = serde_json::from_slice::<serde_json::Value>(&output.stdout)
            .map_err(|_| String::from("Ungültige Metadaten-Antwort von YouTube"))?;
        if let Some(direct_url) = payload.get("url").and_then(|value| value.as_str()) {
            crate::runtime::musicbot::audio_engine::cache_youtube_direct_url(
                url,
                direct_url.to_string(),
            );
        }
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
            .or_else(|| {
                payload
                    .get("duration")
                    .and_then(|value| value.as_f64())
                    .map(|f| f.round() as u64)
            })
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

        Ok(ResolvedMusicMetadata {
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
                "--js-runtimes",
                "node",
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
