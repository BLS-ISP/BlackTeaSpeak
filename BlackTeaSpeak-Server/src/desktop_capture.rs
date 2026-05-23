use std::net::SocketAddr;
use anyhow::{Result, Context, anyhow};

pub const DEFAULT_DESKTOP_CAPTURE_BIND: &str = "0.0.0.0:10000";
pub const DEFAULT_DESKTOP_COMPAT_MAX_BYTES: usize = 256;

#[derive(Clone, Default)]
pub struct DesktopResponder {}

impl DesktopResponder {
    pub fn describe(&self) -> String {
        "Disabled".to_string()
    }
    
    pub fn native_compat_default() -> Self {
        Self::default()
    }
    
    pub fn with_ts3init1_action(self, _action: DesktopResponseAction) -> Self { self }
    pub fn with_bootstrap185_action(self, _action: DesktopResponseAction) -> Self { self }
    pub fn with_get_cookie_action(self, _action: Option<DesktopResponseAction>) -> Self { self }
    pub fn with_get_puzzle_action(self, _action: Option<DesktopResponseAction>) -> Self { self }
    pub fn with_followup39_action(self, _action: Option<DesktopResponseAction>) -> Self { self }
    pub fn with_post_command4_action(self, _action: Option<DesktopResponseAction>) -> Self { self }
}

pub enum DesktopResponseAction {
    None,
}

impl DesktopResponseAction {
    pub fn from_cli_value(value: &str) -> Result<Self> {
        Ok(Self::None)
    }
}

pub struct DesktopCaptureServer {
    bind_addr: String,
}

impl DesktopCaptureServer {
    pub fn bind_with_responder(bind_addr: &str, _max_bytes: Option<usize>, _responder: DesktopResponder) -> Result<Self> {
        Ok(Self {
            bind_addr: bind_addr.to_string(),
        })
    }
}

impl DesktopUdpServer for DesktopCaptureServer {
    fn local_addr(&self) -> Result<SocketAddr> {
        self.bind_addr.parse().context("invalid bind address")
    }

    fn run(self) -> Result<()> {
        Ok(())
    }
}

pub trait DesktopUdpServer {
    fn local_addr(&self) -> Result<SocketAddr>;
    fn run(self) -> Result<()>;
}
