use std::sync::{Arc, Mutex};
use std::net::SocketAddr;
use std::io;
use russh::{Channel, ChannelId};
use russh::server::{Auth, Session, Msg, Handler, Server};
use crate::runtime::{BaselineRuntime, QuerySessionState};
use crate::transport::{execute_request_with_notifications, render_notification};
use crate::query::{parse_request_line, render_response, QueryResponse};

#[derive(Clone)]
pub struct SshQueryServer {
    pub runtime: Arc<Mutex<BaselineRuntime>>,
}

impl Server for SshQueryServer {
    type Handler = SshQueryHandler;
    
    fn new_client(&mut self, peer_addr: Option<SocketAddr>) -> Self::Handler {
        SshQueryHandler {
            runtime: Arc::clone(&self.runtime),
            session_state: QuerySessionState {
                connection_ip: peer_addr.map(|a| a.ip().to_string()).unwrap_or_default(),
                ..QuerySessionState::default()
            },
            buffer: String::new(),
        }
    }
}

pub struct SshQueryHandler {
    runtime: Arc<Mutex<BaselineRuntime>>,
    session_state: QuerySessionState,
    buffer: String,
}

impl Handler for SshQueryHandler {
    type Error = anyhow::Error;

    async fn auth_password(
        &mut self,
        user: &str,
        password: &str,
    ) -> Result<Auth, Self::Error> {
        let mut runtime = self.runtime.lock().unwrap();
        let request = crate::query::CommandRequest {
            command: "login".to_string(),
            named_args: std::collections::BTreeMap::new(),
            positional_args: vec![user.to_string(), password.to_string()],
            option_groups: vec![], flags: std::collections::BTreeSet::new(),
        };
        let before_session = self.session_state.clone();
        let (response, _) = execute_request_with_notifications(
            &mut runtime,
            &request,
            &before_session,
            &mut self.session_state,
        );
        if response.error_id == 0 {
            Ok(Auth::Accept)
        } else {
            Ok(Auth::Reject { proceed_with_methods: None, partial_success: false })
        }
    }

    async fn channel_open_session(
        &mut self,
        _channel: Channel<Msg>,
        _session: &mut Session,
    ) -> Result<bool, Self::Error> {
        Ok(true)
    }

    async fn data(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        let text = String::from_utf8_lossy(data);
        self.buffer.push_str(&text);
        
        while let Some(idx) = self.buffer.find('\n') {
            let line = self.buffer[..idx].trim().to_string();
            self.buffer = self.buffer[idx + 1..].to_string();

            if line.is_empty() { continue; }
            if line.eq_ignore_ascii_case("quit") {
                // We just stop processing and close
                session.close(channel);
                break;
            }

            let parsed = parse_request_line(&line);
            let mut runtime = self.runtime.lock().unwrap();
            let before_session = self.session_state.clone();
            
            let (response, notifs) = match parsed {
                Ok(req) => execute_request_with_notifications(&mut runtime, &req, &before_session, &mut self.session_state),
                Err(err) => (QueryResponse::error(1536, err.to_string()), vec![]),
            };
            
            let rendered_resp = render_response(&response);
            session.data(channel, bytes::Bytes::copy_from_slice(format!("{rendered_resp}\n\r").as_bytes()));
            
            for notif in notifs {
                let rendered = render_notification(&notif);
                session.data(channel, bytes::Bytes::copy_from_slice(format!("{rendered}\n\r").as_bytes()));
            }
        }
        Ok(())
    }
}
