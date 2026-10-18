use crate::teaspeak_transport::UdpSession;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::UdpSocket;

pub async fn handle_teaspeak_native(
    payload_str: &str,
    session: &mut UdpSession,
    addr: SocketAddr,
    socket: &Arc<UdpSocket>,
    iv_struct: &[u8],
) -> bool {
    if payload_str.starts_with("handshakebegin ") || payload_str == "handshakebegin" {
        println!("Received handshakebegin! Client using TeaSpeak handshake.");
        if let Some(pk) = super::wire_formatting::parse_teaspeak_argument(payload_str, "publicKey")
            .or_else(|| super::wire_formatting::parse_teaspeak_argument(payload_str, "public_key"))
            .or_else(|| super::wire_formatting::parse_teaspeak_argument(payload_str, "client_uid"))
            .filter(|s| !s.is_empty())
        {
            session.client_uid = crate::runtime::stable_web_client_unique_identifier(&pk);
        } else if let Some(nick) =
            super::wire_formatting::parse_teaspeak_argument(payload_str, "client_nickname")
                .filter(|s| !s.is_empty())
        {
            session.client_nickname = nick.clone();
            session.client_uid = crate::runtime::stable_web_client_unique_identifier(&nick);
        }

        let auth_method =
            super::wire_formatting::parse_teaspeak_argument_u32(payload_str, "authentication_method")
                .unwrap_or(1);
        let rc = super::wire_formatting::get_teaspeak_return_code(payload_str);

        // Send ACK FIRST
        let ack_cmd = if let Some(rc_val) = &rc {
            format!("error id=0 msg=ok return_code={}", rc_val)
        } else {
            "error id=0 msg=ok".to_string()
        };
        super::wire_formatting::send_teaspeak_command(&ack_cmd, session, addr, socket, iv_struct)
            .await;
        println!("Sent ack to {}: {}", addr, ack_cmd);

        // For TeamSpeak (1) and TeaForum (0) identities, send handshakeidentityproof challenge
        if auth_method != 2 {
            let message = "TeaSpeak,\\smade\\swith\\slove\\sand\\scoffee\\sby\\sWolverinDEV\\s(#QUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUE=)";
            let handshakeidentityproof =
                format!("handshakeidentityproof message={} digest=SHA-256", message);
            super::wire_formatting::send_teaspeak_command(
                &handshakeidentityproof,
                session,
                addr,
                socket,
                iv_struct,
            )
            .await;
            println!(
                "Sent handshakeidentityproof to {}: {}",
                addr, handshakeidentityproof
            );
        }
        return true;
    } else if payload_str.starts_with("handshakeidentityproof ")
        || payload_str.starts_with("handshakeindentityproof ")
        || payload_str == "handshakeidentityproof"
        || payload_str == "handshakeindentityproof"
    {
        println!("Received handshakeidentityproof! Client signed the proof message.");

        let rc = super::wire_formatting::get_teaspeak_return_code(payload_str);
        let ack_cmd = if let Some(rc_val) = &rc {
            format!("error id=0 msg=ok return_code={}", rc_val)
        } else {
            "error id=0 msg=ok".to_string()
        };
        super::wire_formatting::send_teaspeak_command(&ack_cmd, session, addr, socket, iv_struct)
            .await;
        println!("Sent ack to {}: {}", addr, ack_cmd);
        return true;
    }
    false
}
