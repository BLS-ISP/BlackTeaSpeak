use std::fs;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;
use std::thread;
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

use blackteaspeak_server::transport::QueryTransportServer;

struct TestClient {
    reader: BufReader<TcpStream>,
    writer: BufWriter<TcpStream>,
}

impl TestClient {
    fn connect(addr: SocketAddr) -> Self {
        let stream = TcpStream::connect(addr).expect("client should connect");
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .expect("read timeout should apply");
        let writer_stream = stream.try_clone().expect("writer clone should succeed");
        writer_stream
            .set_write_timeout(Some(Duration::from_secs(2)))
            .expect("write timeout should apply");

        Self {
            reader: BufReader::new(stream),
            writer: BufWriter::new(writer_stream),
        }
    }

    fn read_banner(&mut self) -> Vec<String> {
        read_banner(&mut self.reader)
    }

    fn read_response(&mut self, label: &str) -> Vec<String> {
        read_response(&mut self.reader, label)
    }

    fn read_notification(&mut self, label: &str) -> String {
        read_notification(&mut self.reader, label)
    }

    fn expect_no_message(&mut self, label: &str) {
        self.reader
            .get_mut()
            .set_read_timeout(Some(Duration::from_millis(250)))
            .expect("short read timeout should apply");

        let mut line = String::new();
        let result = self.reader.read_line(&mut line);
        assert!(result.is_err(), "unexpected message for {label}: {line}");

        self.reader
            .get_mut()
            .set_read_timeout(Some(Duration::from_secs(2)))
            .expect("default read timeout should restore");
    }

    fn write_line(&mut self, line: &str) {
        self.writer
            .write_all(line.as_bytes())
            .expect("request line should write");
        self.writer
            .write_all(b"\r\n")
            .expect("request terminator should write");
        self.writer.flush().expect("request should flush");
    }
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate should live inside workspace root")
        .to_path_buf()
}

fn bind_test_server(label: &str) -> (QueryTransportServer, PathBuf) {
    let state_path = temp_state_path(label);
    cleanup_state_path(&state_path);
    (
        QueryTransportServer::bind_with_state_path(workspace_root(), &state_path, "127.0.0.1:0")
            .expect("server should bind"),
        state_path,
    )
}

#[test]
fn tcp_transport_handles_banner_and_core_commands() {
    let (server, state_path) = bind_test_server("tcp-core");
    let shutdown = server.shutdown_handle();
    let addr = server
        .local_addr()
        .expect("server should expose local addr");
    let join = thread::spawn(move || server.run().expect("server should run"));

    let result = (|| {
        let mut client = TestClient::connect(addr);

        let banner = client.read_banner();
        assert!(
            banner
                .iter()
                .any(|line| line.contains("BlackTeaSpeak Compat ServerQuery"))
        );

        client.write_line("login serveradmin serveradmin");
        assert_eq!(
            client.read_response("login").last().map(String::as_str),
            Some("error id=0 msg=ok")
        );

        client.write_line("use sid=1");
        assert_eq!(
            client.read_response("use").last().map(String::as_str),
            Some("error id=0 msg=ok")
        );

        client.write_line("serverinfo");
        let response = client.read_response("serverinfo");
        assert!(
            response
                .first()
                .is_some_and(|line| line.contains("virtualserver_name=BlackTeaSpeak\\sCompat"))
        );

        client.write_line("quit");
        assert_eq!(
            client.read_response("quit").last().map(String::as_str),
            Some("error id=0 msg=ok")
        );
    })();

    shutdown.shutdown();
    join.join().expect("server thread should stop");
    cleanup_state_path(&state_path);
    result
}

#[test]
fn tcp_transport_shares_runtime_state_between_clients() {
    let (server, state_path) = bind_test_server("tcp-shared-runtime");
    let shutdown = server.shutdown_handle();
    let addr = server
        .local_addr()
        .expect("server should expose local addr");
    let join = thread::spawn(move || server.run().expect("server should run"));

    let result = (|| {
        let mut admin_client = TestClient::connect(addr);
        let _ = admin_client.read_banner();

        admin_client.write_line("login serveradmin serveradmin");
        let _ = admin_client.read_response("admin login");
        admin_client.write_line("querycreate client_login_name=server_bot");
        let created = admin_client.read_response("querycreate");
        assert!(
            created
                .first()
                .is_some_and(|line| line.contains("client_login_name=server_bot"))
        );

        let mut bot_client = TestClient::connect(addr);
        let _ = bot_client.read_banner();

        bot_client.write_line("login server_bot generated-server_bot");
        assert_eq!(
            bot_client
                .read_response("bot login")
                .last()
                .map(String::as_str),
            Some("error id=0 msg=ok")
        );

        bot_client.write_line("whoami");
        let whoami = bot_client.read_response("whoami");
        assert!(
            whoami
                .first()
                .is_some_and(|line| line.contains("client_login_name=server_bot"))
        );

        admin_client.write_line("quit");
        let _ = admin_client.read_response("admin quit");
        bot_client.write_line("quit");
        let _ = bot_client.read_response("bot quit");
    })();

    shutdown.shutdown();
    join.join().expect("server thread should stop");
    cleanup_state_path(&state_path);
    result
}

#[test]
fn tcp_transport_shares_antiflood_ip_blocks_between_clients() {
    let (server, state_path) = bind_test_server("tcp-shared-antiflood-ip");
    let shutdown = server.shutdown_handle();
    let addr = server
        .local_addr()
        .expect("server should expose local addr");
    let join = thread::spawn(move || server.run().expect("server should run"));

    let result = (|| {
        let mut admin = TestClient::connect(addr);
        let _ = admin.read_banner();

        admin.write_line("login serveradmin serveradmin");
        assert_eq!(
            admin
                .read_response("admin login")
                .last()
                .map(String::as_str),
            Some("error id=0 msg=ok")
        );

        admin.write_line(
            "querycreate client_login_name=ipblock_a client_login_password=ipblock_secret_a server_id=1",
        );
        assert_eq!(
            admin
                .read_response("querycreate ipblock_a")
                .last()
                .map(String::as_str),
            Some("error id=0 msg=ok")
        );

        admin.write_line(
            "querycreate client_login_name=ipblock_b client_login_password=ipblock_secret_b server_id=1",
        );
        assert_eq!(
            admin
                .read_response("querycreate ipblock_b")
                .last()
                .map(String::as_str),
            Some("error id=0 msg=ok")
        );

        admin.write_line("use sid=1");
        assert_eq!(
            admin.read_response("admin use").last().map(String::as_str),
            Some("error id=0 msg=ok")
        );

        admin.write_line(
            "serveredit virtualserver_antiflood_points_tick_reduce=0 virtualserver_antiflood_points_needed_command_block=100 virtualserver_antiflood_points_needed_ip_block=8 virtualserver_antiflood_ban_time=60",
        );
        assert_eq!(
            admin
                .read_response("serveredit antiflood")
                .last()
                .map(String::as_str),
            Some("error id=0 msg=ok")
        );

        let mut same_a = TestClient::connect(addr);
        let _ = same_a.read_banner();
        same_a.write_line("login ipblock_a ipblock_secret_a");
        assert_eq!(
            same_a
                .read_response("ipblock_a login")
                .last()
                .map(String::as_str),
            Some("error id=0 msg=ok")
        );
        same_a.write_line("use sid=1");
        assert_eq!(
            same_a
                .read_response("ipblock_a use")
                .last()
                .map(String::as_str),
            Some("error id=0 msg=ok")
        );

        let mut same_b = TestClient::connect(addr);
        let _ = same_b.read_banner();
        same_b.write_line("login ipblock_b ipblock_secret_b");
        assert_eq!(
            same_b
                .read_response("ipblock_b login")
                .last()
                .map(String::as_str),
            Some("error id=0 msg=ok")
        );
        same_b.write_line("use sid=1");
        assert_eq!(
            same_b
                .read_response("ipblock_b use")
                .last()
                .map(String::as_str),
            Some("error id=0 msg=ok")
        );

        let mut same_a_blocked = false;
        let mut same_b_blocked = false;
        for _ in 0..10 {
            same_a.write_line("serverinfo");
            let shared_a_response = same_a.read_response("ipblock_a serverinfo");
            if shared_a_response
                .last()
                .is_some_and(|line| line.contains("error id=524"))
            {
                same_a_blocked = true;
                break;
            }
            assert_eq!(
                shared_a_response.last().map(String::as_str),
                Some("error id=0 msg=ok")
            );

            same_b.write_line("serverinfo");
            let shared_b_response = same_b.read_response("ipblock_b serverinfo");
            if shared_b_response
                .last()
                .is_some_and(|line| line.contains("error id=524"))
            {
                same_b_blocked = true;
                break;
            }
            assert_eq!(
                shared_b_response.last().map(String::as_str),
                Some("error id=0 msg=ok")
            );
        }

        assert!(
            same_a_blocked || same_b_blocked,
            "same-IP TCP clients should eventually trigger a shared flood block"
        );

        if same_a_blocked {
            same_b.write_line("serverinfo");
            assert!(
                same_b
                    .read_response("ipblock_b shared block")
                    .last()
                    .is_some_and(|line| line.contains("error id=524"))
            );
        } else {
            same_a.write_line("serverinfo");
            assert!(
                same_a
                    .read_response("ipblock_a shared block")
                    .last()
                    .is_some_and(|line| line.contains("error id=524"))
            );
        }

        admin.write_line("quit");
        let _ = admin.read_response("admin quit");
        same_a.write_line("quit");
        let _ = same_a.read_response("ipblock_a quit");
        same_b.write_line("quit");
        let _ = same_b.read_response("ipblock_b quit");
    })();

    shutdown.shutdown();
    join.join().expect("server thread should stop");
    cleanup_state_path(&state_path);
    result
}

#[test]
fn tcp_transport_delivers_and_stops_server_notifications() {
    let (server, state_path) = bind_test_server("tcp-server-notify");
    let shutdown = server.shutdown_handle();
    let addr = server
        .local_addr()
        .expect("server should expose local addr");
    let join = thread::spawn(move || server.run().expect("server should run"));

    let result = (|| {
        let mut watcher = TestClient::connect(addr);
        let _ = watcher.read_banner();
        watcher.write_line("login serveradmin serveradmin");
        let _ = watcher.read_response("watcher login");
        watcher.write_line("servernotifyregister event=server");
        assert_eq!(
            watcher
                .read_response("notify register")
                .last()
                .map(String::as_str),
            Some("error id=0 msg=ok")
        );

        let mut actor = TestClient::connect(addr);
        let _ = actor.read_banner();
        actor.write_line("login serveradmin serveradmin");
        let _ = actor.read_response("actor login");

        let enter = watcher.read_notification("client enter notification");
        assert!(enter.starts_with("notifycliententerview "));
        assert!(enter.contains("client_nickname=serveradmin"));

        watcher.write_line("servernotifyunregister");
        assert_eq!(
            watcher
                .read_response("notify unregister")
                .last()
                .map(String::as_str),
            Some("error id=0 msg=ok")
        );

        actor.write_line("quit");
        let _ = actor.read_response("actor quit");
        watcher.expect_no_message("server notifications after unregister");

        watcher.write_line("quit");
        let _ = watcher.read_response("watcher quit");
    })();

    shutdown.shutdown();
    join.join().expect("server thread should stop");
    cleanup_state_path(&state_path);
    result
}

#[test]
fn tcp_transport_delivers_clientupdate_notifications() {
    let (server, state_path) = bind_test_server("tcp-clientupdate-notify");
    let shutdown = server.shutdown_handle();
    let addr = server
        .local_addr()
        .expect("server should expose local addr");
    let join = thread::spawn(move || server.run().expect("server should run"));

    let result = (|| {
        let mut watcher = TestClient::connect(addr);
        let _ = watcher.read_banner();
        watcher.write_line("login serveradmin serveradmin");
        let _ = watcher.read_response("watcher login");
        watcher.write_line("servernotifyregister event=server");
        let _ = watcher.read_response("watcher server notify register");

        let mut actor = TestClient::connect(addr);
        let _ = actor.read_banner();
        actor.write_line("login serveradmin serveradmin");
        let _ = actor.read_response("actor login");

        let enter = watcher.read_notification("client enter notification before update");
        assert!(enter.starts_with("notifycliententerview "));

        actor.write_line("whoami");
        let actor_id = extract_field(&actor.read_response("actor whoami")[0], "clid")
            .expect("whoami should expose clid");

        actor.write_line(
            "clientupdate client_nickname=Query\\sRenamed client_away=1 client_away_message=Heads\\sdown client_input_muted=1 client_output_muted=1",
        );
        assert_eq!(
            actor
                .read_response("actor clientupdate")
                .last()
                .map(String::as_str),
            Some("error id=0 msg=ok")
        );

        let updated = watcher.read_notification("client updated notification");
        assert!(updated.starts_with("notifyclientupdated "));
        assert!(updated.contains(&format!("clid={}", actor_id)));
        assert!(updated.contains("client_nickname=Query\\sRenamed"));
        assert!(updated.contains("client_away=1"));
        assert!(updated.contains("client_away_message=Heads\\sdown"));
        assert!(updated.contains("client_input_muted=1"));
        assert!(updated.contains("client_output_muted=1"));

        actor.write_line(&format!("clientinfo clid={}", actor_id));
        let clientinfo = actor.read_response("actor clientinfo after update");
        assert!(clientinfo[0].contains("client_nickname=Query\\sRenamed"));
        assert!(clientinfo[0].contains("client_away=1"));
        assert!(clientinfo[0].contains("client_away_message=Heads\\sdown"));
        assert!(clientinfo[0].contains("client_input_muted=1"));
        assert!(clientinfo[0].contains("client_output_muted=1"));

        actor.write_line("clientfind pattern=Renamed");
        let clientfind = actor.read_response("actor clientfind after update");
        assert!(
            clientfind
                .iter()
                .any(|line| line.contains(&format!("clid={}", actor_id)))
        );
        assert!(
            clientfind
                .iter()
                .any(|line| line.contains("client_nickname=Query\\sRenamed"))
        );

        watcher.write_line("servernotifyunregister");
        let _ = watcher.read_response("watcher server notify unregister");

        actor.write_line("clientupdate client_nickname=Query\\sSilent");
        let _ = actor.read_response("actor second clientupdate");
        watcher.expect_no_message("clientupdate notifications after unregister");

        actor.write_line("quit");
        let _ = actor.read_response("actor quit");
        watcher.write_line("quit");
        let _ = watcher.read_response("watcher quit");
    })();

    shutdown.shutdown();
    join.join().expect("server thread should stop");
    cleanup_state_path(&state_path);
    result
}

#[test]
fn tcp_transport_delivers_serveredit_notifications() {
    let (server, state_path) = bind_test_server("tcp-serveredit-notify");
    let shutdown = server.shutdown_handle();
    let addr = server
        .local_addr()
        .expect("server should expose local addr");
    let join = thread::spawn(move || server.run().expect("server should run"));

    let result = (|| {
        let mut watcher = TestClient::connect(addr);
        let _ = watcher.read_banner();
        watcher.write_line("login serveradmin serveradmin");
        let _ = watcher.read_response("watcher login");
        watcher.write_line("servernotifyregister event=server");
        let _ = watcher.read_response("watcher server notify register");

        let mut actor = TestClient::connect(addr);
        let _ = actor.read_banner();
        actor.write_line("login serveradmin serveradmin");
        let _ = actor.read_response("actor login");

        let enter = watcher.read_notification("client enter notification before serveredit");
        assert!(enter.starts_with("notifycliententerview "));

        actor.write_line(
            "serveredit virtualserver_name=Query\\sServer virtualserver_welcomemessage=Hello\\sWatchers virtualserver_hostmessage=Heads\\sUp virtualserver_hostmessage_mode=2 virtualserver_ask_for_privilegekey=1 virtualserver_maxclients=64",
        );
        assert_eq!(
            actor
                .read_response("actor serveredit")
                .last()
                .map(String::as_str),
            Some("error id=0 msg=ok")
        );

        let edited = watcher.read_notification("server edited notification");
        assert!(edited.starts_with("notifyserveredited "));
        assert!(edited.contains("virtualserver_id=1"));
        assert!(edited.contains("virtualserver_name=Query\\sServer"));
        assert!(edited.contains("virtualserver_welcomemessage=Hello\\sWatchers"));
        assert!(edited.contains("virtualserver_hostmessage=Heads\\sUp"));
        assert!(edited.contains("virtualserver_hostmessage_mode=2"));
        assert!(edited.contains("virtualserver_ask_for_privilegekey=1"));
        assert!(edited.contains("virtualserver_maxclients=64"));
        assert!(edited.contains("invokername=serveradmin"));

        actor.write_line("serverinfo");
        let serverinfo = actor.read_response("actor serverinfo after serveredit");
        assert!(serverinfo[0].contains("virtualserver_name=Query\\sServer"));
        assert!(serverinfo[0].contains("virtualserver_welcomemessage=Hello\\sWatchers"));
        assert!(serverinfo[0].contains("virtualserver_hostmessage=Heads\\sUp"));
        assert!(serverinfo[0].contains("virtualserver_maxclients=64"));

        watcher.write_line("servernotifyunregister");
        let _ = watcher.read_response("watcher server notify unregister");

        actor.write_line("serveredit virtualserver_name=Query\\sSilent");
        let _ = actor.read_response("actor second serveredit");
        watcher.expect_no_message("serveredit notifications after unregister");

        actor.write_line("quit");
        let _ = actor.read_response("actor quit");
        watcher.write_line("quit");
        let _ = watcher.read_response("watcher quit");
    })();

    shutdown.shutdown();
    join.join().expect("server thread should stop");
    cleanup_state_path(&state_path);
    result
}

#[test]
fn tcp_transport_delivers_channel_join_leave_and_edit_notifications() {
    let (server, state_path) = bind_test_server("tcp-channel-notify");
    let shutdown = server.shutdown_handle();
    let addr = server
        .local_addr()
        .expect("server should expose local addr");
    let join = thread::spawn(move || server.run().expect("server should run"));

    let result = (|| {
        let mut watcher = TestClient::connect(addr);
        let _ = watcher.read_banner();
        watcher.write_line("login serveradmin serveradmin");
        let _ = watcher.read_response("watcher login");
        watcher.write_line("servernotifyregister event=channel id=2");
        let _ = watcher.read_response("watcher channel notify register");

        let mut actor = TestClient::connect(addr);
        let _ = actor.read_banner();
        actor.write_line("login serveradmin serveradmin");
        let _ = actor.read_response("actor login");
        actor.write_line("whoami");
        let actor_id = extract_field(&actor.read_response("actor whoami")[0], "clid")
            .expect("whoami should expose clid");

        actor.write_line(&format!("clientmove clid={} cid=2", actor_id));
        let _ = actor.read_response("actor clientmove to channel 2");
        let enter = watcher.read_notification("channel enter notification");
        assert!(enter.starts_with("notifycliententerview "));
        assert!(enter.contains(&format!("clid={}", actor_id)));
        assert!(enter.contains("ctid=2"));

        actor.write_line(
            "channeledit cid=2 channel_name=Music\\sSuite channel_topic=Late\\sSession",
        );
        let _ = actor.read_response("actor channeledit");
        let edited = watcher.read_notification("channel edited notification");
        assert!(edited.starts_with("notifychanneledited "));
        assert!(edited.contains("cid=2"));
        assert!(edited.contains("channel_name=Music\\sSuite"));
        assert!(edited.contains("channel_topic=Late\\sSession"));

        actor.write_line(&format!("clientmove clid={} cid=1", actor_id));
        let _ = actor.read_response("actor clientmove to channel 1");
        let left = watcher.read_notification("channel leave notification");
        assert!(left.starts_with("notifyclientleftview "));
        assert!(left.contains(&format!("clid={}", actor_id)));
        assert!(left.contains("cfid=2"));
        assert!(left.contains("ctid=1"));

        watcher.write_line("servernotifyunregister");
        let _ = watcher.read_response("watcher channel notify unregister");

        actor.write_line("channeledit cid=2 channel_name=Quiet\\sRoom");
        let _ = actor.read_response("actor second channeledit");
        watcher.expect_no_message("channel notifications after unregister");

        actor.write_line("quit");
        let _ = actor.read_response("actor quit");
        watcher.write_line("quit");
        let _ = watcher.read_response("watcher quit");
    })();

    shutdown.shutdown();
    join.join().expect("server thread should stop");
    cleanup_state_path(&state_path);
    result
}

#[test]
fn tcp_transport_delivers_channel_structure_notifications() {
    let (server, state_path) = bind_test_server("tcp-channel-structure");
    let shutdown = server.shutdown_handle();
    let addr = server
        .local_addr()
        .expect("server should expose local addr");
    let join = thread::spawn(move || server.run().expect("server should run"));

    let result = (|| {
        let mut watcher = TestClient::connect(addr);
        let _ = watcher.read_banner();
        watcher.write_line("login serveradmin serveradmin");
        let _ = watcher.read_response("watcher login");

        let mut actor = TestClient::connect(addr);
        let _ = actor.read_banner();
        actor.write_line("login serveradmin serveradmin");
        let _ = actor.read_response("actor login");

        watcher.write_line("servernotifyregister event=channel id=1");
        let _ = watcher.read_response("watcher root channel notify register");

        actor.write_line(
            "channelcreate channel_name=Ops\\sRoom cpid=1 order=0 channel_topic=Build\\sQueue",
        );
        let created_response = actor.read_response("actor channelcreate");
        let created_id =
            extract_field(&created_response[0], "cid").expect("channelcreate should expose cid");
        let created = watcher.read_notification("channel created notification");
        assert!(created.starts_with("notifychannelcreated "));
        assert!(created.contains(&format!("cid={}", created_id)));
        assert!(created.contains("cpid=1"));
        assert!(created.contains("channel_name=Ops\\sRoom"));

        watcher.write_line(&format!(
            "servernotifyregister event=channel id={}",
            created_id
        ));
        let _ = watcher.read_response("watcher created channel notify register");

        actor.write_line(&format!("channelmove cid={} cpid=0 order=0", created_id));
        let _ = actor.read_response("actor channelmove");
        let moved = watcher.read_notification("channel moved notification");
        assert!(moved.starts_with("notifychannelmoved "));
        assert!(moved.contains(&format!("cid={}", created_id)));
        assert!(moved.contains("cpid=0"));
        assert!(moved.contains("order=0"));

        actor.write_line(&format!("channeldelete cid={} force=1", created_id));
        let _ = actor.read_response("actor channeldelete");
        let deleted = watcher.read_notification("channel deleted notification");
        assert!(deleted.starts_with("notifychanneldeleted "));
        assert!(deleted.contains(&format!("cid={}", created_id)));

        watcher.write_line("servernotifyunregister");
        let _ = watcher.read_response("watcher channel notify unregister");

        actor.write_line("channelcreate channel_name=No\\sNotify cpid=1");
        let _ = actor.read_response("actor second channelcreate");
        watcher.expect_no_message("channel structure notifications after unregister");

        actor.write_line("quit");
        let _ = actor.read_response("actor quit");
        watcher.write_line("quit");
        let _ = watcher.read_response("watcher quit");
    })();

    shutdown.shutdown();
    join.join().expect("server thread should stop");
    cleanup_state_path(&state_path);
    result
}

#[test]
fn tcp_transport_exposes_serverlist_clientlist_and_clientinfo() {
    let (server, state_path) = bind_test_server("tcp-online-views");
    let shutdown = server.shutdown_handle();
    let addr = server
        .local_addr()
        .expect("server should expose local addr");
    let join = thread::spawn(move || server.run().expect("server should run"));

    let result = (|| {
        let mut watcher = TestClient::connect(addr);
        let _ = watcher.read_banner();
        watcher.write_line("login serveradmin serveradmin");
        let _ = watcher.read_response("watcher login");

        let mut actor = TestClient::connect(addr);
        let _ = actor.read_banner();
        actor.write_line("login serveradmin serveradmin");
        let _ = actor.read_response("actor login");
        actor.write_line("whoami");
        let actor_id = extract_field(&actor.read_response("actor whoami")[0], "clid")
            .expect("actor whoami should expose clid");

        watcher.write_line("serverlist -uid");
        let serverlist = watcher.read_response("watcher serverlist");
        assert!(serverlist[0].contains("virtualserver_id=1"));
        assert!(serverlist[0].contains("virtualserver_clientsonline=5"));
        assert!(serverlist[0].contains("virtualserver_unique_identifier=compat-baseline-uid"));

        watcher.write_line("clientlist -uid -groups -country");
        let clientlist = watcher.read_response("watcher clientlist");
        assert!(clientlist[0].contains("client_nickname=ScP"));
        assert!(clientlist[0].contains("client_nickname=Rabe85"));
        assert!(clientlist[0].matches("client_nickname=serveradmin").count() >= 2);

        watcher.write_line(&format!("clientinfo clid={}", actor_id));
        let clientinfo = watcher.read_response("watcher clientinfo");
        assert!(clientinfo[0].contains(&format!("clid={}", actor_id)));
        assert!(clientinfo[0].contains("client_type=1"));
        assert!(clientinfo[0].contains("client_platform=compat-rust"));

        actor.write_line("quit");
        let _ = actor.read_response("actor quit");

        watcher.write_line("serverlist");
        let serverlist_after_quit = watcher.read_response("watcher serverlist after actor quit");
        assert!(serverlist_after_quit[0].contains("virtualserver_clientsonline=4"));

        watcher.write_line("quit");
        let _ = watcher.read_response("watcher quit");
    })();

    shutdown.shutdown();
    join.join().expect("server thread should stop");
    cleanup_state_path(&state_path);
    result
}

#[test]
fn tcp_transport_delivers_textserver_textchannel_and_textprivate_notifications() {
    let (server, state_path) = bind_test_server("tcp-text-notify");
    let shutdown = server.shutdown_handle();
    let addr = server
        .local_addr()
        .expect("server should expose local addr");
    let join = thread::spawn(move || server.run().expect("server should run"));

    let result = (|| {
        let mut watcher = TestClient::connect(addr);
        let _ = watcher.read_banner();
        watcher.write_line("login serveradmin serveradmin");
        let _ = watcher.read_response("watcher login");
        watcher.write_line("whoami");
        let watcher_id = extract_field(&watcher.read_response("watcher whoami")[0], "clid")
            .expect("watcher whoami should expose clid");
        watcher.write_line(&format!("clientmove clid={} cid=2", watcher_id));
        let _ = watcher.read_response("watcher move to channel 2");
        watcher.write_line("servernotifyregister event=textserver");
        let _ = watcher.read_response("watcher textserver register");
        watcher.write_line("servernotifyregister event=textchannel id=2");
        let _ = watcher.read_response("watcher textchannel register");
        watcher.write_line("servernotifyregister event=textprivate");
        let _ = watcher.read_response("watcher textprivate register");

        let mut actor = TestClient::connect(addr);
        let _ = actor.read_banner();
        actor.write_line("login serveradmin serveradmin");
        let _ = actor.read_response("actor login");
        actor.write_line("whoami");
        let actor_id = extract_field(&actor.read_response("actor whoami")[0], "clid")
            .expect("actor whoami should expose clid");
        actor.write_line(&format!("clientmove clid={} cid=2", actor_id));
        let _ = actor.read_response("actor move to channel 2");

        actor.write_line("sendtextmessage targetmode=3 target=0 msg=Hello\\sServer");
        let _ = actor.read_response("actor server text");
        let server_message = watcher.read_notification("textserver notification");
        assert!(server_message.starts_with("notifytextmessage "));
        assert!(server_message.contains("targetmode=3"));
        assert!(server_message.contains("msg=Hello\\sServer"));

        actor.write_line("sendtextmessage targetmode=2 target=0 msg=Hello\\sChannel");
        let _ = actor.read_response("actor channel text");
        let channel_message = watcher.read_notification("textchannel notification");
        assert!(channel_message.starts_with("notifytextmessage "));
        assert!(channel_message.contains("targetmode=2"));
        assert!(channel_message.contains("msg=Hello\\sChannel"));

        actor.write_line(&format!(
            "sendtextmessage targetmode=1 target={} msg=Hello\\sPrivate",
            watcher_id
        ));
        let _ = actor.read_response("actor private text");
        let private_message = watcher.read_notification("textprivate notification");
        assert!(private_message.starts_with("notifytextmessage "));
        assert!(private_message.contains("targetmode=1"));
        assert!(private_message.contains("msg=Hello\\sPrivate"));

        watcher.write_line("servernotifyunregister");
        let _ = watcher.read_response("watcher text unregister");

        actor.write_line("sendtextmessage targetmode=3 target=0 msg=After\\sUnregister");
        let _ = actor.read_response("actor server text after unregister");
        watcher.expect_no_message("text notifications after unregister");

        actor.write_line("quit");
        let _ = actor.read_response("actor quit");
        watcher.write_line("quit");
        let _ = watcher.read_response("watcher quit");
    })();

    shutdown.shutdown();
    join.join().expect("server thread should stop");
    cleanup_state_path(&state_path);
    result
}

#[test]
fn tcp_transport_persists_runtime_state_across_server_restart() {
    let state_path = temp_state_path("transport-persistence");
    cleanup_state_path(&state_path);

    {
        let server = QueryTransportServer::bind_with_state_path(
            workspace_root(),
            &state_path,
            "127.0.0.1:0",
        )
        .expect("server should bind");
        let shutdown = server.shutdown_handle();
        let addr = server
            .local_addr()
            .expect("server should expose local addr");
        let join = thread::spawn(move || server.run().expect("server should run"));

        {
            let mut client = TestClient::connect(addr);
            let _ = client.read_banner();
            client.write_line("login serveradmin serveradmin");
            let _ = client.read_response("login");
            client.write_line("servernotifyregister event=channel id=2");
            let _ = client.read_response("register persisted notify");
            client.write_line("clientmove cid=2");
            let _ = client.read_response("move to persisted channel");
            client.write_line(
                "channelcreate channel_name=Restart\\sRoom cpid=0 channel_topic=Cold\\sBoot",
            );
            let _ = client.read_response("persisted channelcreate");
            client.write_line(
                "querycreate client_login_name=restart_bot client_login_password=restart_secret",
            );
            let _ = client.read_response("persisted querycreate");
            client.write_line("quit");
            let _ = client.read_response("quit");
        }

        shutdown.shutdown();
        join.join().expect("server thread should stop");
    }

    {
        let server = QueryTransportServer::bind_with_state_path(
            workspace_root(),
            &state_path,
            "127.0.0.1:0",
        )
        .expect("server should rebind");
        let shutdown = server.shutdown_handle();
        let addr = server
            .local_addr()
            .expect("server should expose local addr");
        let join = thread::spawn(move || server.run().expect("server should run"));

        {
            let mut admin = TestClient::connect(addr);
            let _ = admin.read_banner();
            admin.write_line("login serveradmin serveradmin");
            let _ = admin.read_response("admin login after restart");

            admin.write_line("whoami");
            let whoami = admin.read_response("admin whoami after restart");
            assert!(whoami[0].contains("client_channel_id=2"));
            assert!(whoami[0].contains("notify_subscription_count=1"));

            admin.write_line("channellist");
            let channellist = admin.read_response("admin channellist after restart");
            assert!(
                channellist
                    .iter()
                    .any(|line| line.contains("channel_name=Restart\\sRoom"))
            );
            assert!(
                channellist
                    .iter()
                    .any(|line| line.contains("channel_topic=Cold\\sBoot"))
            );

            let mut bot = TestClient::connect(addr);
            let _ = bot.read_banner();
            bot.write_line("login restart_bot restart_secret");
            let bot_login = bot.read_response("bot login after restart");
            assert_eq!(
                bot_login.last().map(String::as_str),
                Some("error id=0 msg=ok")
            );

            bot.write_line("quit");
            let _ = bot.read_response("bot quit after restart");
            admin.write_line("quit");
            let _ = admin.read_response("admin quit after restart");
        }

        shutdown.shutdown();
        join.join().expect("server thread should stop");
    }

    cleanup_state_path(&state_path);
}

fn read_banner(reader: &mut BufReader<TcpStream>) -> Vec<String> {
    let mut lines = Vec::new();
    loop {
        let mut line = String::new();
        let bytes = reader
            .read_line(&mut line)
            .expect("banner line should read");
        assert!(bytes > 0, "connection closed while reading banner");
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            break;
        }
        lines.push(trimmed.to_string());
    }
    lines
}

fn read_response(reader: &mut BufReader<TcpStream>, label: &str) -> Vec<String> {
    let mut lines = Vec::new();
    loop {
        let mut line = String::new();
        let bytes = reader
            .read_line(&mut line)
            .unwrap_or_else(|error| panic!("response line should read for {label}: {error}"));
        assert!(
            bytes > 0,
            "connection closed while reading response for {label}"
        );
        let trimmed = line.trim_end_matches(['\r', '\n']).to_string();
        if trimmed.is_empty() {
            continue;
        }
        let is_error_line = trimmed.starts_with("error ");
        lines.push(trimmed);
        if is_error_line {
            break;
        }
    }
    lines
}

fn read_notification(reader: &mut BufReader<TcpStream>, label: &str) -> String {
    loop {
        let mut line = String::new();
        let bytes = reader
            .read_line(&mut line)
            .unwrap_or_else(|error| panic!("notification should read for {label}: {error}"));
        assert!(
            bytes > 0,
            "connection closed while reading notification for {label}"
        );
        let trimmed = line.trim_end_matches(['\r', '\n']).to_string();
        if !trimmed.is_empty() {
            return trimmed;
        }
    }
}

fn extract_field(line: &str, field_name: &str) -> Option<String> {
    line.split_whitespace().find_map(|part| {
        part.split_once('=')
            .and_then(|(key, value)| (key == field_name).then(|| value.to_string()))
    })
}

fn temp_state_path(label: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("BlackTeaSpeak-Server-{label}-{unique}.json"))
}

fn cleanup_state_path(path: &PathBuf) {
    let _ = fs::remove_file(path);
}
