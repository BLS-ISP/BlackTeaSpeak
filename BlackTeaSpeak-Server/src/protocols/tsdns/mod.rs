pub mod resolver;
pub mod tcp_server;

pub use resolver::{TsdnsRule, resolve_tsdns_hostname};
pub use tcp_server::{DEFAULT_TSDNS_BIND, TsdnsTcpServer};
