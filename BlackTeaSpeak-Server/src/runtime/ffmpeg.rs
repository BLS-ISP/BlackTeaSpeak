use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::BaselineRuntime;
use super::musicbot::ResolvedMusicMetadata;

pub(super) fn resolve_music_command(env_key: &str, candidates: &[&str]) -> Option<PathBuf> {
    if let Ok(value) = env::var(env_key) {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return Some(PathBuf::from(trimmed));
        }
    }

    let path = env::var_os("PATH")?;
    for directory in env::split_paths(&path) {
        for candidate in candidates {
            let direct_candidate = directory.join(candidate);
            if direct_candidate.is_file() {
                return Some(direct_candidate);
            }

            #[cfg(windows)]
            for extension in ["exe", "cmd", "bat"] {
                let windows_candidate = directory.join(format!("{candidate}.{extension}"));
                if windows_candidate.is_file() {
                    return Some(windows_candidate);
                }
            }
        }
    }

    None
}

impl BaselineRuntime {
    fn parse_channel_repository_reference(url: &str) -> Option<(u32, String)> {
        let raw_reference = url.strip_prefix("channel://")?.replace('\\', "/");
        let trimmed_reference = raw_reference.trim_start_matches('/');
        let (channel_id, relative_path) = trimmed_reference.split_once('/')?;
        let channel_id = channel_id.parse::<u32>().ok()?;

        let sanitized_segments = relative_path
            .split('/')
            .filter(|segment| !segment.is_empty() && *segment != ".")
            .map(str::trim)
            .map(str::to_string)
            .collect::<Vec<_>>();
        if sanitized_segments.is_empty() || sanitized_segments.iter().any(|segment| segment == "..") {
            return None;
        }

        Some((channel_id, sanitized_segments.join("/")))
    }

    fn channel_repository_path(&self, channel_id: u32, relative_path: &str) -> PathBuf {
        let mut path = self
            .specs
            .workspace_root
            .join("BlackTeaSpeak-Server")
            .join("data")
            .join("file-repositories")
            .join("channels")
            .join(channel_id.to_string());
        for segment in relative_path.split('/') {
            path.push(segment);
        }
        path
    }

    fn probe_ffprobe_metadata(path: &Path) -> Option<serde_json::Value> {
        let ffprobe = resolve_music_command("TEASPEAK_COMPAT_FFPROBE", &["ffprobe"])?;
        let output = Command::new(ffprobe)
            .args([
                "-v",
                "quiet",
                "-print_format",
                "json",
                "-show_format",
                "-show_streams",
            ])
            .arg(path)
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }

        serde_json::from_slice::<serde_json::Value>(&output.stdout).ok()
    }

    pub(super) fn probe_channel_music_metadata(&self, url: &str) -> Option<ResolvedMusicMetadata> {
        let (channel_id, relative_path) = Self::parse_channel_repository_reference(url)?;
        let path = self.channel_repository_path(channel_id, &relative_path);
        let file_metadata = fs::metadata(&path).ok()?;
        if !file_metadata.is_file() {
            return None;
        }

        let file_stem = path
            .file_stem()
            .and_then(|value| value.to_str())
            .filter(|value| !value.trim().is_empty())
            .map(|value| value.replace(['_', '-'], " "))
            .unwrap_or_else(|| Self::music_entry_display_name(url, false));
        let mut description = format!("Server audio: channel {channel_id}/{}", relative_path);
        let mut length_seconds = 0;
        let mut seekable = true;

        if let Some(probe) = Self::probe_ffprobe_metadata(&path) {
            if let Some(title) = probe
                .get("format")
                .and_then(|format| format.get("tags"))
                .and_then(|tags| tags.get("title"))
                .and_then(|title| title.as_str())
                .filter(|title| !title.trim().is_empty())
            {
                description = probe
                    .get("format")
                    .and_then(|format| format.get("tags"))
                    .and_then(|tags| tags.get("comment"))
                    .and_then(|comment| comment.as_str())
                    .filter(|comment| !comment.trim().is_empty())
                    .unwrap_or(description.as_str())
                    .to_string();
                return Some(ResolvedMusicMetadata {
                    loaded: true,
                    title: title.to_string(),
                    description,
                    thumbnail: String::new(),
                    length_seconds: probe
                        .get("format")
                        .and_then(|format| format.get("duration"))
                        .and_then(|duration| duration.as_str())
                        .and_then(|duration| duration.parse::<f64>().ok())
                        .map(|duration| duration.max(0.0).round() as u32)
                        .unwrap_or_default(),
                    seekable,
                    live_stream: false,
                });
            }

            length_seconds = probe
                .get("format")
                .and_then(|format| format.get("duration"))
                .and_then(|duration| duration.as_str())
                .and_then(|duration| duration.parse::<f64>().ok())
                .map(|duration| duration.max(0.0).round() as u32)
                .unwrap_or_default();
            seekable = probe
                .get("format")
                .and_then(|format| format.get("format_name"))
                .and_then(|format_name| format_name.as_str())
                .map(|format_name| !format_name.contains("hls"))
                .unwrap_or(true);
        }

        Some(ResolvedMusicMetadata {
            loaded: true,
            title: file_stem,
            description,
            thumbnail: String::new(),
            length_seconds,
            seekable,
            live_stream: false,
        })
    }
}
