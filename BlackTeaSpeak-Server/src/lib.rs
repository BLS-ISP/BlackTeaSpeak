pub mod database;
pub mod file_transfer;
pub mod models;
pub mod protocols;
pub mod runtime;
pub mod specs;
pub mod state;
pub mod transport;
pub mod web;

// Backward-compatible module aliases
pub use protocols::btea::capture_proxy as desktop_capture;
pub use protocols::btea::crypto as desktop_crypto;
pub use protocols::btea::handshake as btea;
pub use protocols::btea::packet_codec as desktop_proto;
pub use protocols::btea::session as desktop_session;
pub use protocols::btea::tcp_transport as desktop_tcp_transport;
pub use protocols::btea::udp_transport as desktop_transport;

pub use protocols::teaspeak::command_handler as teaspeak_cmd;
pub use protocols::teaspeak::native_framing as teaspeak_native;
pub use protocols::teaspeak::rtc_manager;
pub use protocols::teaspeak::webrtc_bridge as teaspeak_web_old;

pub use protocols::ts3::command_handler as teaspeak_ts3;
pub use protocols::ts3::tcp_transport as teaspeak_tcp_transport;
pub use protocols::ts3::udp_transport as teaspeak_transport;
pub use protocols::ts6::command_handler as teaspeak_ts6;
pub use protocols::ts6::stream_manager as ts6_stream_manager;

pub use protocols::serverquery::parser as query;
pub use protocols::serverquery::ssh_and_telnet as ssh_query;
pub use protocols::tsdns;
pub use protocols::webtransport::http_static_server as web_client;
