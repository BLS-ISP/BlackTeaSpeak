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

    let mut local_dirs = Vec::new();
    if let Ok(cwd) = env::current_dir() {
        local_dirs.push(cwd.join("data").join("tools"));
        local_dirs.push(cwd.join("BlackTeaSpeak-Server").join("data").join("tools"));
    }
    if let Ok(exe) = env::current_exe() {
        if let Some(parent) = exe.parent() {
            local_dirs.push(parent.join("data").join("tools"));
        }
    }
    for directory in local_dirs {
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

    pub(crate) fn server_data_dir(&self) -> PathBuf {
        let nested = self
            .specs
            .workspace_root
            .join("BlackTeaSpeak-Server")
            .join("data");
        let direct = self.specs.workspace_root.join("data");
        if direct.join("foundation").exists() || (!nested.exists() && direct.exists()) {
            direct
        } else {
            nested
        }
    }

    fn channel_repository_path(&self, channel_id: u32, relative_path: &str) -> PathBuf {
        let mut nested = self
            .specs
            .workspace_root
            .join("BlackTeaSpeak-Server")
            .join("data")
            .join("file-repositories")
            .join("channels")
            .join(channel_id.to_string());
        for segment in relative_path.split('/') {
            nested.push(segment);
        }
        if nested.exists() {
            return nested;
        }
        let mut direct = self
            .server_data_dir()
            .join("file-repositories")
            .join("channels")
            .join(channel_id.to_string());
        for segment in relative_path.split('/') {
            direct.push(segment);
        }
        if direct.exists() {
            direct
        } else {
            nested
        }
    }

    pub(crate) fn resolve_local_music_file_path(&self, url: &str) -> Option<PathBuf> {
        if let Some(stripped) = url
            .strip_prefix("/music/")
            .or_else(|| url.strip_prefix("local://"))
        {
            let clean = stripped.trim_start_matches('/');
            if clean.is_empty() || clean.contains("..") {
                return None;
            }
            if let Some(reg) = &self.file_transfer_registry
                && let Ok(p) = reg.music_download_path(clean)
                && p.is_file()
            {
                return Some(p);
            }
            for base in [
                self.server_data_dir().join("music"),
                self.server_data_dir()
                    .join("file-repositories")
                    .join("global")
                    .join("music"),
                self.specs
                    .workspace_root
                    .join("BlackTeaSpeak-Server")
                    .join("data")
                    .join("music"),
                self.specs
                    .workspace_root
                    .join("BlackTeaSpeak-Server")
                    .join("data")
                    .join("file-repositories")
                    .join("global")
                    .join("music"),
            ] {
                let candidate = base.join(clean);
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
            return None;
        }

        if let Some((channel_id, relative_path)) = Self::parse_channel_repository_reference(url) {
            let path = self.channel_repository_path(channel_id, &relative_path);
            if path.is_file() {
                return Some(path);
            }
        }
        None
    }

    pub(crate) fn discover_local_music_files(&self) -> Vec<(String, String, String, u64)> {
        const AUDIO_EXTS: &[&str] = &[
            "mp3", "ogg", "opus", "wav", "flac", "m4a", "aac", "webm", "mp4", "mkv", "mov", "avi",
        ];
        let data_dir = self.server_data_dir();
        let primary_music_dir = data_dir.join("music");
        let global_repo_music_dir = data_dir
            .join("file-repositories")
            .join("global")
            .join("music");
        let _ = fs::create_dir_all(&primary_music_dir);
        let _ = fs::create_dir_all(&global_repo_music_dir);

        let mut results: Vec<(String, String, String, u64)> = Vec::new();
        let mut seen_urls = std::collections::HashSet::new();

        for (dir, label) in [
            (primary_music_dir, "data/music"),
            (global_repo_music_dir, "Server Music"),
        ] {
            if let Ok(entries) = fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if !path.is_file() {
                        continue;
                    }
                    let ext = path
                        .extension()
                        .and_then(|e| e.to_str())
                        .map(|s| s.to_ascii_lowercase())
                        .unwrap_or_default();
                    if !AUDIO_EXTS.contains(&ext.as_str()) {
                        continue;
                    }
                    let Some(fname) = path.file_name().and_then(|n| n.to_str()) else {
                        continue;
                    };
                    let url = format!("/music/{fname}");
                    if seen_urls.insert(url.clone()) {
                        let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                        results.push((fname.to_string(), url, String::from(label), size));
                    }
                }
            }
        }

        let channels_root = data_dir.join("file-repositories").join("channels");
        if let Ok(ch_dirs) = fs::read_dir(&channels_root) {
            for ch_entry in ch_dirs.flatten() {
                let ch_path = ch_entry.path();
                if !ch_path.is_dir() {
                    continue;
                }
                let Some(cid_str) = ch_path.file_name().and_then(|n| n.to_str()) else {
                    continue;
                };
                if cid_str.parse::<u32>().is_err() {
                    continue;
                }
                if let Ok(files) = fs::read_dir(&ch_path) {
                    for file_entry in files.flatten() {
                        let path = file_entry.path();
                        if !path.is_file() {
                            continue;
                        }
                        let ext = path
                            .extension()
                            .and_then(|e| e.to_str())
                            .map(|s| s.to_ascii_lowercase())
                            .unwrap_or_default();
                        if !AUDIO_EXTS.contains(&ext.as_str()) {
                            continue;
                        }
                        let Some(fname) = path.file_name().and_then(|n| n.to_str()) else {
                            continue;
                        };
                        let url = format!("channel://{cid_str}/{fname}");
                        if seen_urls.insert(url.clone()) {
                            let size = file_entry.metadata().map(|m| m.len()).unwrap_or(0);
                            results.push((
                                fname.to_string(),
                                url,
                                format!("Channel #{cid_str}"),
                                size,
                            ));
                        }
                    }
                }
            }
        }

        results.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
        results
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
        if let Some((channel_id, relative_path)) = Self::parse_channel_repository_reference(url) {
            let path = self.channel_repository_path(channel_id, &relative_path);
            return self.probe_path_music_metadata(
                &path,
                url,
                format!("Server audio: channel {channel_id}/{}", relative_path),
            );
        }
        let path = self.resolve_local_music_file_path(url)?;
        self.probe_path_music_metadata(&path, url, format!("Local server audio: {url}"))
    }

    fn probe_path_music_metadata(
        &self,
        path: &Path,
        url: &str,
        mut description: String,
    ) -> Option<ResolvedMusicMetadata> {
        let file_metadata = fs::metadata(path).ok()?;
        if !file_metadata.is_file() {
            return None;
        }

        let file_stem = path
            .file_stem()
            .and_then(|value| value.to_str())
            .filter(|value| !value.trim().is_empty())
            .map(|value| value.replace(['_', '-'], " "))
            .unwrap_or_else(|| Self::music_entry_display_name(url, false));
        let mut length_seconds = 0;
        let mut seekable = true;

        if let Some(probe) = Self::probe_ffprobe_metadata(path) {
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
