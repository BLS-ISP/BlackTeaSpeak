use std::ffi::OsString;
use std::fs;
use std::ops::{Deref, DerefMut};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use blackteaspeak_server::runtime::{
    BaselineRuntime, QuerySessionState, create_baseline_runtime_with_state_path,
};

struct TestRuntime {
    runtime: BaselineRuntime,
    state_path: PathBuf,
}

impl Deref for TestRuntime {
    type Target = BaselineRuntime;

    fn deref(&self) -> &Self::Target {
        &self.runtime
    }
}

impl DerefMut for TestRuntime {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.runtime
    }
}

impl Drop for TestRuntime {
    fn drop(&mut self) {
        cleanup_state_path(&self.state_path);
    }
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate should live inside workspace root")
        .to_path_buf()
}

fn create_test_runtime(label: &str) -> TestRuntime {
    let state_path = temp_state_path(label);
    cleanup_state_path(&state_path);
    let runtime = create_baseline_runtime_with_state_path(workspace_root(), &state_path)
        .expect("runtime should build");
    TestRuntime {
        runtime,
        state_path,
    }
}

fn temp_state_path(label: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("BlackTeaSpeak-Server-{label}-{unique}.json"))
}

fn unique_temp_dir(label: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("BlackTeaSpeak-Server-{label}-{unique}"))
}

fn cleanup_state_path(path: &PathBuf) {
    let _ = fs::remove_file(path);
    let _ = fs::remove_file(path.with_extension("db"));
    let _ = fs::remove_dir_all(path.with_extension("ft-workspace"));
}

struct TempDirectory {
    path: PathBuf,
}

impl TempDirectory {
    fn new(path: PathBuf) -> Self {
        Self { path }
    }

    fn path(&self) -> &PathBuf {
        &self.path
    }
}

impl Drop for TempDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

struct LockedEnvVar {
    key: &'static str,
    original: Option<OsString>,
    _guard: std::sync::MutexGuard<'static, ()>,
}

impl LockedEnvVar {
    fn set(key: &'static str, value: OsString) -> Self {
        static ENV_MUTEX: OnceLock<Mutex<()>> = OnceLock::new();

        let guard = ENV_MUTEX
            .get_or_init(|| Mutex::new(()))
            .lock()
            .expect("environment lock should not be poisoned");
        let original = std::env::var_os(key);
        unsafe {
            std::env::set_var(key, value);
        }

        Self {
            key,
            original,
            _guard: guard,
        }
    }
}

impl Drop for LockedEnvVar {
    fn drop(&mut self) {
        match self.original.as_ref() {
            Some(value) => unsafe {
                std::env::set_var(self.key, value);
            },
            None => unsafe {
                std::env::remove_var(self.key);
            },
        }
    }
}

fn write_fake_ytdlp_command(directory: &PathBuf) -> PathBuf {
    #[cfg(windows)]
    let path = directory.join("fake-ytdlp.cmd");
    #[cfg(not(windows))]
    let path = directory.join("fake-ytdlp.sh");

    #[cfg(windows)]
    let content = concat!(
        "@echo off\r\n",
        "echo {\"title\":\"Fake YouTube Title\",\"description\":\"Synthetic regression description\",\"thumbnail\":\"https://img.example.test/thumb.jpg\",\"duration\":321,\"live_status\":\"post_live\"}\r\n",
    );
    #[cfg(not(windows))]
    let content = concat!(
        "#!/bin/sh\n",
        "printf '%s\\n' '{\"title\":\"Fake YouTube Title\",\"description\":\"Synthetic regression description\",\"thumbnail\":\"https://img.example.test/thumb.jpg\",\"duration\":321,\"live_status\":\"post_live\"}'\n",
    );

    fs::write(&path, content).expect("fake ytdlp command should be writable");

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mut permissions = fs::metadata(&path)
            .expect("fake ytdlp command metadata should exist")
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&path, permissions)
            .expect("fake ytdlp command should be executable");
    }

    path
}

fn extract_field(line: &str, field_name: &str) -> Option<String> {
    line.split(|character: char| character.is_whitespace() || character == '|')
        .find_map(|part| {
            part.split_once('=')
                .and_then(|(key, value)| (key == field_name).then(|| value.to_string()))
        })
}

fn extract_rows(response: &str) -> Vec<std::collections::BTreeMap<String, String>> {
    response
        .split("\nerror ")
        .next()
        .unwrap_or(response)
        .split('|')
        .filter_map(|segment| {
            let row = segment
                .split_whitespace()
                .filter_map(|part| part.split_once('='))
                .map(|(key, value)| (key.to_string(), value.to_string()))
                .collect::<std::collections::BTreeMap<_, _>>();

            (!row.is_empty()).then_some(row)
        })
        .collect()
}


#[path = "baseline_suite/core_and_musicbot.rs"]
mod core_and_musicbot;
#[path = "baseline_suite/permissions_and_groups.rs"]
mod permissions_and_groups;
#[path = "baseline_suite/persistence_and_messaging.rs"]
mod persistence_and_messaging;
#[path = "baseline_suite/security_and_antiflood.rs"]
mod security_and_antiflood;
#[path = "baseline_suite/live_wire_and_protocols.rs"]
mod live_wire_and_protocols;
#[path = "baseline_suite/ts3_feature_parity_packets.rs"]
mod ts3_feature_parity_packets;
