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

#[test]
fn help_returns_command_metadata() {
    let mut runtime = create_test_runtime("help-metadata");
    let mut session = QuerySessionState::default();

    let rendered = runtime.execute("help version", &mut session);
    assert!(rendered.contains("command=version"));
    assert!(rendered.contains("category=query-session"));
}

#[test]
fn login_use_and_serverinfo() {
    let mut runtime = create_test_runtime("login-use-serverinfo");
    let mut session = QuerySessionState::default();

    assert!(
        runtime
            .execute("login serveradmin serveradmin", &mut session)
            .contains("error id=0 msg=ok")
    );
    assert!(
        runtime
            .execute("use sid=1", &mut session)
            .contains("error id=0 msg=ok")
    );

    let rendered = runtime.execute("serverinfo", &mut session);
    assert!(rendered.contains("virtualserver_name=BlackTeaSpeak\\sCompat"));
    assert!(rendered.contains("virtualserver_port=9987"));
}

#[test]
fn serveredit_updates_server_state() {
    let mut runtime = create_test_runtime("serveredit-state");
    let mut session = QuerySessionState::default();

    assert!(
        runtime
            .execute("login serveradmin serveradmin", &mut session)
            .contains("error id=0 msg=ok")
    );
    assert!(
        runtime
            .execute("use sid=1", &mut session)
            .contains("error id=0 msg=ok")
    );

    let edited = runtime.execute(
        "serveredit virtualserver_name=Compat\\sStaging virtualserver_welcomemessage=Hello\\sAdmins virtualserver_hostmessage=Observe\\sProtocol virtualserver_hostmessage_mode=2 virtualserver_ask_for_privilegekey=1 virtualserver_maxclients=64 virtualserver_antiflood_points_tick_reduce=0 virtualserver_antiflood_points_needed_command_block=3 virtualserver_antiflood_points_needed_ip_block=5 virtualserver_antiflood_ban_time=60",
        &mut session,
    );
    assert!(edited.contains("error id=0 msg=ok"));

    let serverinfo = runtime.execute("serverinfo", &mut session);
    assert!(serverinfo.contains("virtualserver_name=Compat\\sStaging"));
    assert!(serverinfo.contains("virtualserver_welcomemessage=Hello\\sAdmins"));
    assert!(serverinfo.contains("virtualserver_hostmessage=Observe\\sProtocol"));
    assert!(serverinfo.contains("virtualserver_hostmessage_mode=2"));
    assert!(serverinfo.contains("virtualserver_ask_for_privilegekey=1"));
    assert!(serverinfo.contains("virtualserver_maxclients=64"));
    assert!(serverinfo.contains("virtualserver_antiflood_points_tick_reduce=0"));
    assert!(serverinfo.contains("virtualserver_antiflood_points_needed_command_block=3"));
    assert!(serverinfo.contains("virtualserver_antiflood_points_needed_ip_block=5"));
    assert!(serverinfo.contains("virtualserver_antiflood_ban_time=60"));

    let serverlist = runtime.execute("serverlist -uid", &mut session);
    assert!(serverlist.contains("virtualserver_name=Compat\\sStaging"));
}

#[test]
fn channellist_returns_seeded_channels() {
    let mut runtime = create_test_runtime("channellist-seeded");
    let mut session = QuerySessionState::default();

    runtime.execute("login serveradmin serveradmin", &mut session);
    runtime.execute("use sid=1", &mut session);

    let rendered = runtime.execute("channellist", &mut session);
    assert!(rendered.contains("channel_name=Default\\sChannel"));
    assert!(rendered.contains("channel_name=Music\\sLounge"));
}

#[test]
fn musicbot_action_changes_state() {
    let mut runtime = create_test_runtime("musicbot-action");
    let mut session = QuerySessionState::default();

    runtime.execute("login serveradmin serveradmin", &mut session);
    runtime.execute("use sid=1", &mut session);

    let queued = runtime.execute(
        "musicbotqueueadd botid=1 type=yt url=https://streams.example.net/live",
        &mut session,
    );
    assert!(queued.contains("song_id=1"));

    let rendered = runtime.execute("musicbotplayeraction botid=1 action=1", &mut session);
    assert!(rendered.contains("state=playing"));
}

#[test]
fn runtime_persists_musicbot_playlist_state_permissions_and_channel_metadata() {
    let state_path = temp_state_path("runtime-musicbot-playlist-state");
    cleanup_state_path(&state_path);

    {
        let mut runtime = create_baseline_runtime_with_state_path(workspace_root(), &state_path)
            .expect("runtime should build with custom state path");
        let mut session = QuerySessionState::default();

        assert!(
            runtime
                .execute("login serveradmin serveradmin", &mut session)
                .contains("error id=0 msg=ok")
        );
        assert!(runtime.execute("use sid=1", &mut session).contains("error id=0 msg=ok"));

        let edited = runtime.execute(
            "playlistedit playlist_id=1 playlist_flag_finished=1 playlist_flag_delete_played=1 playlist_replay_mode=2 playlist_max_songs=55",
            &mut session,
        );
        assert!(edited.contains("error id=0 msg=ok"));

        let added_permission = runtime.execute(
            "playlistaddperm playlist_id=1 permsid=i_playlist_permission_modify_power permvalue=42 permnegated=0 permskip=0",
            &mut session,
        );
        assert!(added_permission.contains("permvalue=42"));

        let added_client_permission = runtime.execute(
            "playlistclientaddperm playlist_id=1 cldbid=41 permsid=i_playlist_delete_power permvalue=21 permnegated=0 permskip=0",
            &mut session,
        );
        assert!(added_client_permission.contains("cldbid=41"));

        let added_song = runtime.execute(
            "playlistsongadd playlist_id=1 type=channel url=channel://1/smoke-upload.txt",
            &mut session,
        );
        assert!(added_song.contains("song_url_loader=channel"));
        assert!(added_song.contains("song_loaded=1"));

        let song_list = runtime.execute(
            "playlistsonglist playlist_id=1 -extract-metadata",
            &mut session,
        );
        assert!(song_list.contains("song_metadata_title=smoke\\supload"));
        assert!(song_list.contains("song_url_loader=channel"));
    }

    {
        let mut runtime = create_baseline_runtime_with_state_path(workspace_root(), &state_path)
            .expect("runtime should reload with persisted state");
        let mut session = QuerySessionState::default();

        assert!(
            runtime
                .execute("login serveradmin serveradmin", &mut session)
                .contains("error id=0 msg=ok")
        );
        assert!(runtime.execute("use sid=1", &mut session).contains("error id=0 msg=ok"));

        let playlist_info = runtime.execute("playlistinfo playlist_id=1", &mut session);
        assert!(playlist_info.contains("playlist_flag_finished=1"));
        assert!(playlist_info.contains("playlist_flag_delete_played=1"));
        assert!(playlist_info.contains("playlist_replay_mode=2"));
        assert!(playlist_info.contains("playlist_max_songs=55"));

        let playlist_permissions =
            runtime.execute("playlistpermlist playlist_id=1 -permsid", &mut session);
        assert!(playlist_permissions.contains("permsid=i_playlist_permission_modify_power"));
        assert!(playlist_permissions.contains("permvalue=42"));

        let playlist_client_permissions = runtime.execute(
            "playlistclientpermlist playlist_id=1 cldbid=41 -permsid",
            &mut session,
        );
        assert!(playlist_client_permissions.contains("permsid=i_playlist_delete_power"));
        assert!(playlist_client_permissions.contains("permvalue=21"));

        let playlist_clients = runtime.execute("playlistclientlist playlist_id=1", &mut session);
        assert!(playlist_clients.contains("cldbid=41"));

        let persisted_song_list = runtime.execute(
            "playlistsonglist playlist_id=1 -extract-metadata",
            &mut session,
        );
        assert!(persisted_song_list.contains("song_url_loader=channel"));
        assert!(persisted_song_list.contains("song_metadata_title=smoke\\supload"));
    }

    cleanup_state_path(&state_path);
}

#[test]
fn playlistsongadd_uses_ytdlp_metadata_when_tool_is_configured() {
    let mut runtime = create_test_runtime("playlist-ytdlp-metadata");
    let mut session = QuerySessionState::default();
    let temp_dir = TempDirectory::new(unique_temp_dir("fake-ytdlp"));
    fs::create_dir_all(temp_dir.path()).expect("temp ytdlp directory should be creatable");
    let fake_ytdlp = write_fake_ytdlp_command(temp_dir.path());
    let _ytdlp_env = LockedEnvVar::set("TEASPEAK_COMPAT_YTDLP", fake_ytdlp.into_os_string());

    assert!(
        runtime
            .execute("login serveradmin serveradmin", &mut session)
            .contains("error id=0 msg=ok")
    );
    assert!(runtime.execute("use sid=1", &mut session).contains("error id=0 msg=ok"));

    let added_song = runtime.execute(
        "playlistsongadd playlist_id=1 type=youtube url=https://youtu.be/fake-ytdlp-regression",
        &mut session,
    );
    assert!(added_song.contains("song_url_loader=youtube"));
    assert!(added_song.contains("song_loaded=1"));

    let songs = extract_rows(&runtime.execute(
        "playlistsonglist playlist_id=1 -extract-metadata",
        &mut session,
    ));
    let first_song = songs.first().expect("playlist should contain the added youtube song");
    assert_eq!(first_song.get("song_url_loader"), Some(&String::from("youtube")));
    assert_eq!(
        first_song.get("song_metadata_title"),
        Some(&String::from("Fake\\sYouTube\\sTitle"))
    );
    assert_eq!(
        first_song.get("song_metadata_description"),
        Some(&String::from("Synthetic\\sregression\\sdescription"))
    );
    assert_eq!(
        first_song.get("song_metadata_thumbnail_url"),
        Some(&String::from("https:\\/\\/img.example.test\\/thumb.jpg"))
    );
    assert_eq!(
        first_song.get("song_metadata_length"),
        Some(&String::from("321"))
    );

    let set_current = runtime.execute(
        "playlistsongsetcurrent playlist_id=1 song_id=1",
        &mut session,
    );
    assert!(set_current.contains("error id=0 msg=ok"));

    let player_info = runtime.execute("musicbotplayerinfo botid=1", &mut session);
    assert!(player_info.contains("player_title=Fake\\sYouTube\\sTitle"));
    assert!(player_info.contains("player_seekable=0"));
}

#[test]
fn query_account_management_roundtrip() {
    let mut runtime = create_test_runtime("query-account-roundtrip");
    let mut session = QuerySessionState::default();

    runtime.execute("login serveradmin serveradmin", &mut session);

    let created = runtime.execute("querycreate client_login_name=server_bot", &mut session);
    assert!(created.contains("client_login_name=server_bot"));

    let renamed = runtime.execute(
        "queryrename client_login_name=server_bot client_new_login_name=server_bot_a",
        &mut session,
    );
    assert!(renamed.contains("client_login_name=server_bot_a"));

    let changed = runtime.execute(
        "querychangepassword client_login_name=server_bot_a",
        &mut session,
    );
    assert!(changed.contains("generated-server_bot_a"));

    let deleted = runtime.execute("querydelete client_login_name=server_bot_a", &mut session);
    assert!(deleted.contains("error id=0 msg=ok"));
}

#[test]
fn phase_one_query_and_client_lookup_commands_work() {
    let mut runtime = create_test_runtime("phase-one-lookups");
    let mut session = QuerySessionState::default();

    assert!(
        runtime
            .execute("login serveradmin serveradmin", &mut session)
            .contains("error id=0 msg=ok")
    );
    assert!(
        runtime
            .execute(
                "querycreate client_login_name=server_bot server_id=2",
                &mut session
            )
            .contains("client_login_name=server_bot")
    );

    let querylist = runtime.execute("querylist", &mut session);
    assert!(querylist.contains("client_login_name=serveradmin"));
    assert!(querylist.contains("client_login_name=server_bot"));
    assert!(querylist.contains("client_bounded_server=2"));

    let querylist_server = runtime.execute("querylist server_id=2", &mut session);
    assert!(querylist_server.contains("client_login_name=server_bot"));
    assert!(!querylist_server.contains("client_login_name=serveradmin"));

    assert!(
        runtime
            .execute("use sid=1", &mut session)
            .contains("error id=0 msg=ok")
    );

    let clientfind = runtime.execute("clientfind pattern=sc", &mut session);
    assert!(clientfind.contains("clid=10"));
    assert!(clientfind.contains("client_nickname=ScP"));

    let clientgetids = runtime.execute("clientgetids cluid=compat-seed-user-40", &mut session);
    assert!(clientgetids.contains("clid=10"));
    assert!(clientgetids.contains("cluid=compat-seed-user-40"));
    assert!(clientgetids.contains("name=ScP"));

    let clientgetdbidfromuid = runtime.execute(
        "clientgetdbidfromuid cluid=compat-seed-user-40",
        &mut session,
    );
    assert!(clientgetdbidfromuid.contains("cluid=compat-seed-user-40"));
    assert!(clientgetdbidfromuid.contains("cldbid=40"));

    let clientgetnamefromdbid = runtime.execute("clientgetnamefromdbid cldbid=41", &mut session);
    assert!(clientgetnamefromdbid.contains("cluid=compat-seed-user-41"));
    assert!(clientgetnamefromdbid.contains("cldbid=41"));
    assert!(clientgetnamefromdbid.contains("name=Rabe85"));

    let clientgetnamefromuid = runtime.execute(
        "clientgetnamefromuid cluid=compat-seed-user-42",
        &mut session,
    );
    assert!(clientgetnamefromuid.contains("cluid=compat-seed-user-42"));
    assert!(clientgetnamefromuid.contains("cldbid=42"));
    assert!(clientgetnamefromuid.contains(r"name=DJ\sMix"));

    let clientgetuidfromclid = runtime.execute("clientgetuidfromclid clid=12", &mut session);
    assert!(clientgetuidfromclid.contains("clid=12"));
    assert!(clientgetuidfromclid.contains("cluid=compat-seed-user-42"));
    assert!(clientgetuidfromclid.contains(r"nickname=DJ\sMix"));
}

#[test]
fn phase_one_metadata_and_property_commands_work() {
    let mut runtime = create_test_runtime("phase-one-metadata");
    let mut session = QuerySessionState::default();

    assert!(
        runtime
            .execute("login serveradmin serveradmin", &mut session)
            .contains("error id=0 msg=ok")
    );
    let created = runtime.execute(
        "querycreate client_login_name=overview_bot client_login_password=overview_secret server_id=1",
        &mut session,
    );
    let client_database_id = extract_field(&created, "cldbid")
        .expect("querycreate should expose cldbid")
        .parse::<u64>()
        .expect("cldbid should parse");
    assert!(
        runtime
            .execute("use sid=1", &mut session)
            .contains("error id=0 msg=ok")
    );
    assert!(
        runtime
            .execute(
                &format!(
                    "clientaddperm cldbid={} permsid=i_client_poke_power permvalue=25 permskip=1",
                    client_database_id
                ),
                &mut session,
            )
            .contains("error id=0 msg=ok")
    );

    let channelinfo = runtime.execute("channelinfo cid=1", &mut session);
    assert!(channelinfo.contains(r"channel_name=Default\sChannel"));
    assert!(channelinfo.contains("channel_flag_default=1"));

    let channelpermlist = runtime.execute("channelpermlist cid=1 -permsid", &mut session);
    assert!(channelpermlist.contains("error id=0 msg=ok"));

    let serverrequestconnectioninfo = runtime.execute("serverrequestconnectioninfo", &mut session);
    assert!(serverrequestconnectioninfo.contains("virtualserver_id=1"));
    assert!(serverrequestconnectioninfo.contains("connection_packets_sent_total=0"));

    let serveridgetbyport =
        runtime.execute("serveridgetbyport virtualserver_port=9987", &mut session);
    assert!(serveridgetbyport.contains("server_id=1"));

    let hostinfo = runtime.execute("hostinfo", &mut session);
    assert!(hostinfo.contains("virtualservers_running_total=1"));
    assert!(hostinfo.contains("host_timestamp_utc="));

    let instanceinfo = runtime.execute("instanceinfo", &mut session);
    assert!(instanceinfo.contains("serverinstance_database_version=11"));
    assert!(instanceinfo.contains("serverinstance_filetransfer_port=30303"));
    assert!(instanceinfo.contains("serverinstance_query_port=10101"));

    let listfeaturesupport = runtime.execute("listfeaturesupport", &mut session);
    assert!(listfeaturesupport.contains("name=error-bulks"));
    assert!(listfeaturesupport.contains("name=advanced-channel-chat"));
        assert!(listfeaturesupport.contains("name=whisper-echo"));
        assert!(listfeaturesupport.contains("name=video"));

    let bindinglist = runtime.execute("bindinglist subsystem=query", &mut session);
    assert!(bindinglist.contains("ip=0.0.0.0"));
    assert!(bindinglist.contains("ip=0::0"));

    let permoverview = runtime.execute(
        &format!("permoverview cldbid={} cid=1 permid=0", client_database_id),
        &mut session,
    );
    assert!(permoverview.contains(&format!("id1={}", client_database_id)));
    assert!(permoverview.contains("t=1"));
    assert!(permoverview.contains("v=25"));
    assert!(permoverview.contains("s=1"));

    let propertylist = runtime.execute("propertylist -server -connection", &mut session);
    assert!(propertylist.contains("name=virtualserver_name"));
    assert!(propertylist.contains("type=SERVER"));
    assert!(propertylist.contains("name=connection_packets_sent_total"));
    assert!(propertylist.contains("type=CONNECTION"));
}

#[test]
fn phase_two_admin_group_and_token_list_commands_work() {
    let mut runtime = create_test_runtime("phase-two-admin-group-token");
    let mut session = QuerySessionState::default();

    assert!(
        runtime
            .execute("login serveradmin serveradmin", &mut session)
            .contains("error id=0 msg=ok")
    );
    let created_account = runtime.execute(
        "querycreate client_login_name=group_bot client_login_password=group_secret server_id=1",
        &mut session,
    );
    let client_database_id = extract_field(&created_account, "cldbid")
        .expect("querycreate should expose cldbid")
        .parse::<u64>()
        .expect("cldbid should parse");
    assert!(
        runtime
            .execute("use sid=1", &mut session)
            .contains("error id=0 msg=ok")
    );

    let created_group = runtime.execute(r"servergroupadd name=Ops\sAdmins type=1", &mut session);
    let group_id = extract_field(&created_group, "sgid")
        .expect("servergroupadd should expose sgid")
        .parse::<u32>()
        .expect("sgid should parse");

    let groups = runtime.execute("servergrouplist", &mut session);
    assert!(groups.contains(&format!("sgid={}", group_id)));
    assert!(groups.contains(r"name=Ops\sAdmins"));

    assert!(
        runtime
            .execute(
                &format!(
                    "servergroupaddclient sgid={} cldbid={}",
                    group_id, client_database_id
                ),
                &mut session,
            )
            .contains("error id=0 msg=ok")
    );

    let delete_without_force = runtime.execute(
        &format!("servergroupdel sgid={} force=0", group_id),
        &mut session,
    );
    assert!(delete_without_force.contains("error id=512"));

    let delete_with_force = runtime.execute(
        &format!("servergroupdel sgid={} force=1", group_id),
        &mut session,
    );
    assert!(delete_with_force.contains("error id=0 msg=ok"));

    let groups_after_delete = runtime.execute("servergrouplist", &mut session);
    assert!(!groups_after_delete.contains(&format!("sgid={}", group_id)));

    let created_token = runtime.execute(
        r"tokenadd token_description=Deploy\sKey token_max_uses=3 action_type=2 action_id1=8|action_type=1 action_text=deploy",
        &mut session,
    );
    let token_id = extract_field(&created_token, "token_id")
        .expect("tokenadd should expose token_id")
        .parse::<u32>()
        .expect("token_id should parse");
    let token_value = extract_field(&created_token, "token")
        .expect("tokenadd should expose token")
        .to_string();
    let first_action_id = extract_field(&created_token, "action_id")
        .expect("tokenadd should expose the first action_id")
        .parse::<u32>()
        .expect("action_id should parse");

    let tokenlist = runtime.execute("tokenlist -new", &mut session);
    assert!(tokenlist.contains(&format!("token_id={}", token_id)));
    assert!(tokenlist.contains(&format!("token={}", token_value.clone())));
    assert!(tokenlist.contains(r"token_description=Deploy\sKey"));

    let privilegekeylist = runtime.execute("privilegekeylist", &mut session);
    assert!(privilegekeylist.contains(&format!("token_id={}", token_id)));

    let token_actions = runtime.execute(
        &format!("tokenactionlist token_id={}", token_id),
        &mut session,
    );
    assert!(token_actions.contains(&format!("action_id={}", first_action_id)));
    assert!(token_actions.contains("action_type=2"));
    assert!(token_actions.contains("action_type=1"));

    let edited_token = runtime.execute(
        &format!(
            r"tokenedit token_id={} token_description=Deploy\sKey\sV2 action_id={}|action_type=3 action_id1=9 action_id2=1 action_text=ops",
            token_id, first_action_id
        ),
        &mut session,
    );
    let new_action_id = extract_field(&edited_token, "action_id")
        .expect("tokenedit should expose the newly created action_id")
        .parse::<u32>()
        .expect("new action_id should parse");

    let edited_actions = runtime.execute(
        &format!("tokenactionlist token={}", token_value.clone()),
        &mut session,
    );
    assert!(!edited_actions.contains(&format!("action_id={}", first_action_id)));
    assert!(edited_actions.contains(&format!("action_id={}", new_action_id)));
    assert!(edited_actions.contains("action_type=3"));

    let tokenlist_after_edit = runtime.execute("tokenlist -new", &mut session);
    assert!(tokenlist_after_edit.contains(r"token_description=Deploy\sKey\sV2"));

    let deleted_token = runtime.execute(
        &format!("tokendelete token={}", token_value.clone()),
        &mut session,
    );
    assert!(deleted_token.contains("error id=0 msg=ok"));

    let tokenlist_after_delete = runtime.execute("tokenlist", &mut session);
    assert!(!tokenlist_after_delete.contains(&format!("token_id={}", token_id)));
}

#[test]
fn phase_two_token_use_applies_server_group_actions() {
    let mut runtime = create_test_runtime("phase-two-tokenuse");
    let mut admin_session = QuerySessionState::default();
    let mut token_session = QuerySessionState::default();

    assert!(
        runtime
            .execute("login serveradmin serveradmin", &mut admin_session)
            .contains("error id=0 msg=ok")
    );
    assert!(
        runtime
            .execute("use sid=1", &mut admin_session)
            .contains("error id=0 msg=ok")
    );

    let created_account = runtime.execute(
        "querycreate client_login_name=token_bot client_login_password=token_secret server_id=1",
        &mut admin_session,
    );
    let client_database_id = extract_field(&created_account, "cldbid")
        .expect("querycreate should expose cldbid")
        .parse::<u64>()
        .expect("cldbid should parse");

    let first_group = runtime.execute(r"servergroupadd name=Deployers type=1", &mut admin_session);
    let first_group_id = extract_field(&first_group, "sgid")
        .expect("servergroupadd should expose sgid")
        .parse::<u32>()
        .expect("sgid should parse");
    let first_token = runtime.execute(
        &format!(r"tokenadd token_description=Deploy\sGrant token_max_uses=1 action_type=2 action_id1={}", first_group_id),
        &mut admin_session,
    );
    let first_token_value = extract_field(&first_token, "token")
        .expect("tokenadd should expose token")
        .to_string();

    assert!(
        runtime
            .execute("login token_bot token_secret", &mut token_session)
            .contains("error id=0 msg=ok")
    );
    assert!(
        runtime
            .execute("use sid=1", &mut token_session)
            .contains("error id=0 msg=ok")
    );
    assert!(
        runtime
            .execute(
                &format!("tokenuse token={}", first_token_value),
                &mut token_session
            )
            .contains("error id=0 msg=ok")
    );

    let groups_after_tokenuse = runtime.execute(
        &format!("servergroupsbyclientid cldbid={}", client_database_id),
        &mut admin_session,
    );
    assert!(groups_after_tokenuse.contains(&format!("sgid={}", first_group_id)));

    let tokens_after_first_use = runtime.execute("tokenlist", &mut admin_session);
    assert!(!tokens_after_first_use.contains(&first_token_value));

    let second_group = runtime.execute(r"servergroupadd name=Auditors type=1", &mut admin_session);
    let second_group_id = extract_field(&second_group, "sgid")
        .expect("servergroupadd should expose sgid")
        .parse::<u32>()
        .expect("sgid should parse");
    let second_token = runtime.execute(
        &format!(
            r"tokenadd token_description=Audit\sGrant token_max_uses=2 action_type=2 action_id1={}",
            second_group_id
        ),
        &mut admin_session,
    );
    let second_token_value = extract_field(&second_token, "token")
        .expect("tokenadd should expose token")
        .to_string();

    assert!(
        runtime
            .execute(
                &format!("privilegekeyuse token={}", second_token_value),
                &mut token_session
            )
            .contains("error id=0 msg=ok")
    );

    let groups_after_alias = runtime.execute(
        &format!("servergroupsbyclientid cldbid={}", client_database_id),
        &mut admin_session,
    );
    assert!(groups_after_alias.contains(&format!("sgid={}", second_group_id)));

    let tokens_after_alias = runtime.execute("tokenlist", &mut admin_session);
    assert!(tokens_after_alias.contains(r"token_description=Audit\sGrant"));
    assert!(tokens_after_alias.contains(&second_token_value));

    assert!(
        runtime
            .execute(
                &format!("tokenuse token={}", second_token_value),
                &mut token_session
            )
            .contains("error id=0 msg=ok")
    );

    let tokens_after_second_use = runtime.execute("tokenlist", &mut admin_session);
    assert!(!tokens_after_second_use.contains(&second_token_value));
}

#[test]
fn phase_two_privilegekey_add_and_delete_alias_token_commands() {
    let mut runtime = create_test_runtime("phase-two-privilegekey-aliases");
    let mut session = QuerySessionState::default();

    assert!(
        runtime
            .execute("login serveradmin serveradmin", &mut session)
            .contains("error id=0 msg=ok")
    );
    assert!(
        runtime
            .execute("use sid=1", &mut session)
            .contains("error id=0 msg=ok")
    );

    let created_group = runtime.execute(r"servergroupadd name=Alias\sGrant type=1", &mut session);
    let group_id = extract_field(&created_group, "sgid")
        .expect("servergroupadd should expose sgid")
        .parse::<u32>()
        .expect("sgid should parse");

    let created_privilegekey = runtime.execute(
        &format!(
            r"privilegekeyadd token_description=Alias\sKey token_max_uses=1 action_type=2 action_id1={}",
            group_id
        ),
        &mut session,
    );
    let token_id = extract_field(&created_privilegekey, "token_id")
        .expect("privilegekeyadd should expose token_id")
        .parse::<u32>()
        .expect("token_id should parse");
    let token_value = extract_field(&created_privilegekey, "token")
        .expect("privilegekeyadd should expose token")
        .to_string();

    let tokenlist = runtime.execute("tokenlist", &mut session);
    assert!(tokenlist.contains(&format!("token_id={}", token_id)));
    assert!(tokenlist.contains(&token_value));
    assert!(tokenlist.contains(r"token_description=Alias\sKey"));

    let deleted_privilegekey = runtime.execute(
        &format!("privilegekeydelete token={}", token_value),
        &mut session,
    );
    assert!(deleted_privilegekey.contains("error id=0 msg=ok"));

    let tokenlist_after_delete = runtime.execute("tokenlist", &mut session);
    assert!(!tokenlist_after_delete.contains(&format!("token_id={}", token_id)));
}

#[test]
fn phase_two_servergroup_copy_rename_and_auto_perm_commands_work() {
    let mut runtime = create_test_runtime("phase-two-servergroup-copy-auto");
    let mut session = QuerySessionState::default();

    assert!(
        runtime
            .execute("login serveradmin serveradmin", &mut session)
            .contains("error id=0 msg=ok")
    );
    assert!(
        runtime
            .execute("use sid=1", &mut session)
            .contains("error id=0 msg=ok")
    );

    let created_source_group =
        runtime.execute(r"servergroupadd name=Ops\sBase type=1", &mut session);
    let source_group_id = extract_field(&created_source_group, "sgid")
        .expect("servergroupadd should expose sgid")
        .parse::<u32>()
        .expect("sgid should parse");

    let seeded_permissions = runtime.execute(
        &format!(
            "servergroupaddperm sgid={} permsid=i_group_auto_update_type permvalue=45 permnegated=0 permskip=0|permsid=b_virtualserver_notify_register permvalue=1 permnegated=0 permskip=0",
            source_group_id
        ),
        &mut session,
    );
    assert!(seeded_permissions.contains("error id=0 msg=ok"));

    let copied_group = runtime.execute(
        &format!(
            r"servergroupcopy ssgid={} tsgid=0 name=Ops\sCopy type=1",
            source_group_id
        ),
        &mut session,
    );
    let copied_group_id = extract_field(&copied_group, "sgid")
        .expect("servergroupcopy should expose sgid")
        .parse::<u32>()
        .expect("copied sgid should parse");

    let copied_permissions = runtime.execute(
        &format!("servergrouppermlist sgid={} -permsid", copied_group_id),
        &mut session,
    );
    assert!(copied_permissions.contains("permsid=i_group_auto_update_type"));
    assert!(copied_permissions.contains("permvalue=45"));
    assert!(copied_permissions.contains("permsid=b_virtualserver_notify_register"));

    let renamed_group = runtime.execute(
        &format!(
            r"servergrouprename sgid={} name=Ops\sRenamed",
            copied_group_id
        ),
        &mut session,
    );
    assert!(renamed_group.contains("error id=0 msg=ok"));

    let groups_after_rename = runtime.execute("servergrouplist", &mut session);
    assert!(groups_after_rename.contains(&format!("sgid={}", copied_group_id)));
    assert!(groups_after_rename.contains(r"name=Ops\sRenamed"));

    let autoadd = runtime.execute(
        "servergroupautoaddperm sgtype=45 permsid=b_virtualserver_start permvalue=1 permnegated=0 permskip=0",
        &mut session,
    );
    assert!(autoadd.contains("error id=0 msg=ok"));

    let source_permissions_after_autoadd = runtime.execute(
        &format!("servergrouppermlist sgid={} -permsid", source_group_id),
        &mut session,
    );
    assert!(source_permissions_after_autoadd.contains("permsid=b_virtualserver_start"));

    let copied_permissions_after_autoadd = runtime.execute(
        &format!("servergrouppermlist sgid={} -permsid", copied_group_id),
        &mut session,
    );
    assert!(copied_permissions_after_autoadd.contains("permsid=b_virtualserver_start"));

    let autodel = runtime.execute(
        "servergroupautodelperm sgtype=45 permsid=b_virtualserver_start",
        &mut session,
    );
    assert!(autodel.contains("error id=0 msg=ok"));

    let source_permissions_after_autodel = runtime.execute(
        &format!("servergrouppermlist sgid={} -permsid", source_group_id),
        &mut session,
    );
    assert!(!source_permissions_after_autodel.contains("permsid=b_virtualserver_start"));

    let copied_permissions_after_autodel = runtime.execute(
        &format!("servergrouppermlist sgid={} -permsid", copied_group_id),
        &mut session,
    );
    assert!(!copied_permissions_after_autodel.contains("permsid=b_virtualserver_start"));
}

#[test]
fn phase_two_servergroup_overwrite_delete_and_permlist_fidelity_work() {
    let mut runtime = create_test_runtime("phase-two-servergroup-overwrite-fidelity");
    let mut admin_session = QuerySessionState::default();

    assert!(
        runtime
            .execute("login serveradmin serveradmin", &mut admin_session)
            .contains("error id=0 msg=ok")
    );
    assert!(
        runtime
            .execute("use sid=1", &mut admin_session)
            .contains("error id=0 msg=ok")
    );

    let created_source_group = runtime.execute(
        r"servergroupadd name=Source\sOps type=1",
        &mut admin_session,
    );
    let source_group_id = extract_field(&created_source_group, "sgid")
        .expect("servergroupadd should expose sgid")
        .parse::<u32>()
        .expect("sgid should parse");

    assert!(runtime
        .execute(
            &format!(
                "servergroupaddperm sgid={} permsid=b_virtualserver_notify_register permvalue=1 permnegated=0 permskip=0",
                source_group_id
            ),
            &mut admin_session,
        )
        .contains("error id=0 msg=ok"));

    let created_target_group = runtime.execute(
        r"servergroupadd name=Pinned\sTarget type=1",
        &mut admin_session,
    );
    let target_group_id = extract_field(&created_target_group, "sgid")
        .expect("servergroupadd should expose sgid")
        .parse::<u32>()
        .expect("target sgid should parse");

    assert!(runtime
        .execute(
            &format!(
                "servergroupaddperm sgid={} permsid=b_virtualserver_notify_unregister permvalue=1 permnegated=0 permskip=0",
                target_group_id
            ),
            &mut admin_session,
        )
        .contains("error id=0 msg=ok"));

    let overwritten_group = runtime.execute(
        &format!(
            r"servergroupcopy ssgid={} tsgid={} name=Ignored\sName type=2",
            source_group_id, target_group_id
        ),
        &mut admin_session,
    );
    assert!(overwritten_group.contains(&format!("sgid={}", target_group_id)));

    let groups_after_overwrite = runtime.execute("servergrouplist", &mut admin_session);
    assert!(groups_after_overwrite.contains(&format!("sgid={}", target_group_id)));
    assert!(groups_after_overwrite.contains(r"name=Pinned\sTarget"));

    let numeric_permlist = runtime.execute(
        &format!("servergrouppermlist sgid={}", target_group_id),
        &mut admin_session,
    );
    assert!(numeric_permlist.contains("permid="));
    assert!(!numeric_permlist.contains("permsid="));
    assert!(!numeric_permlist.contains("sgid="));

    let named_permlist = runtime.execute(
        &format!("servergrouppermlist sgid={} -permsid", target_group_id),
        &mut admin_session,
    );
    assert!(named_permlist.contains("permsid=b_virtualserver_notify_register"));
    assert!(!named_permlist.contains("permsid=b_virtualserver_notify_unregister"));
    assert!(!named_permlist.contains("sgid="));

    let created_account = runtime.execute(
        "querycreate client_login_name=overwrite_bot client_login_password=overwrite_secret server_id=1",
        &mut admin_session,
    );
    let client_database_id = extract_field(&created_account, "cldbid")
        .expect("querycreate should expose cldbid")
        .parse::<u64>()
        .expect("cldbid should parse");

    assert!(
        runtime
            .execute(
                &format!(
                    "servergroupaddclient sgid={} cldbid={}",
                    target_group_id, client_database_id
                ),
                &mut admin_session,
            )
            .contains("error id=0 msg=ok")
    );

    let delete_without_force = runtime.execute(
        &format!("servergroupdel sgid={} force=0", target_group_id),
        &mut admin_session,
    );
    assert!(delete_without_force.contains("error id=512"));

    let delete_with_force = runtime.execute(
        &format!("servergroupdel sgid={} force=1", target_group_id),
        &mut admin_session,
    );
    assert!(delete_with_force.contains("error id=0 msg=ok"));

    let groups_after_delete = runtime.execute("servergroupsbyclientid", &mut admin_session);
    let fallback_groups = runtime.execute(
        &format!("servergroupsbyclientid cldbid={}", client_database_id),
        &mut admin_session,
    );
    assert!(!groups_after_delete.contains(&format!("sgid={}", target_group_id)));
    assert!(fallback_groups.contains("sgid=7"));
}

#[test]
fn phase_two_channelgroup_commands_and_assignments_work() {
    let mut runtime = create_test_runtime("phase-two-channelgroup");
    let mut admin_session = QuerySessionState::default();

    assert!(
        runtime
            .execute("login serveradmin serveradmin", &mut admin_session)
            .contains("error id=0 msg=ok")
    );
    assert!(
        runtime
            .execute("use sid=1", &mut admin_session)
            .contains("error id=0 msg=ok")
    );

    let seeded_groups = runtime.execute("channelgrouplist", &mut admin_session);
    assert!(seeded_groups.contains("cgid=8"));
    assert!(seeded_groups.contains(r"name=Channel\sAdmin"));
    assert!(seeded_groups.contains("cgid=10"));
    assert!(seeded_groups.contains("name=Guest"));

    let created_source_group = runtime.execute(
        r"channelgroupadd name=Channel\sModerators type=1",
        &mut admin_session,
    );
    let source_group_id = extract_field(&created_source_group, "cgid")
        .expect("channelgroupadd should expose cgid")
        .parse::<u32>()
        .expect("cgid should parse");

    assert!(runtime
        .execute(
            &format!(
                "channelgroupaddperm cgid={} permsid=i_client_talk_power permvalue=70 permnegated=0 permskip=0",
                source_group_id
            ),
            &mut admin_session,
        )
        .contains("error id=0 msg=ok"));

    let numeric_permlist = runtime.execute(
        &format!("channelgrouppermlist cgid={}", source_group_id),
        &mut admin_session,
    );
    assert!(numeric_permlist.contains("permid="));
    assert!(!numeric_permlist.contains("permsid="));

    let copied_group = runtime.execute(
        &format!(
            r"channelgroupcopy scgid={} tcgid=0 name=Channel\sModerators\sCopy type=1",
            source_group_id
        ),
        &mut admin_session,
    );
    let copied_group_id = extract_field(&copied_group, "cgid")
        .expect("channelgroupcopy should expose cgid")
        .parse::<u32>()
        .expect("copied cgid should parse");

    let created_target_group = runtime.execute(
        r"channelgroupadd name=Pinned\sChannel type=1",
        &mut admin_session,
    );
    let target_group_id = extract_field(&created_target_group, "cgid")
        .expect("channelgroupadd should expose cgid")
        .parse::<u32>()
        .expect("target cgid should parse");

    assert!(runtime
        .execute(
            &format!(
                "channelgroupaddperm cgid={} permsid=b_client_use_channel_commander permvalue=1 permnegated=0 permskip=0",
                target_group_id
            ),
            &mut admin_session,
        )
        .contains("error id=0 msg=ok"));

    let overwritten_group = runtime.execute(
        &format!(
            r"channelgroupcopy scgid={} tsgid={} name=Ignored\sName type=2",
            source_group_id, target_group_id
        ),
        &mut admin_session,
    );
    assert!(overwritten_group.contains(&format!("cgid={}", target_group_id)));

    let groups_after_overwrite = runtime.execute("channelgrouplist", &mut admin_session);
    assert!(groups_after_overwrite.contains(&format!("cgid={}", target_group_id)));
    assert!(groups_after_overwrite.contains(r"name=Pinned\sChannel"));

    let named_permlist = runtime.execute(
        &format!("channelgrouppermlist cgid={} -permsid", target_group_id),
        &mut admin_session,
    );
    assert!(named_permlist.contains("permsid=i_client_talk_power"));
    assert!(!named_permlist.contains("permsid=b_client_use_channel_commander"));

    assert!(
        runtime
            .execute(
                &format!(
                    r"channelgrouprename cgid={} name=Pinned\sRenamed",
                    target_group_id
                ),
                &mut admin_session,
            )
            .contains("error id=0 msg=ok")
    );

    let renamed_groups = runtime.execute("channelgrouplist", &mut admin_session);
    assert!(renamed_groups.contains(r"name=Pinned\sRenamed"));
    assert!(renamed_groups.contains(&format!("cgid={}", copied_group_id)));

    let created_account = runtime.execute(
        "querycreate client_login_name=channelgroup_bot client_login_password=channelgroup_secret server_id=1",
        &mut admin_session,
    );
    let client_database_id = extract_field(&created_account, "cldbid")
        .expect("querycreate should expose cldbid")
        .parse::<u64>()
        .expect("cldbid should parse");

    let default_group_overview = runtime.execute(
        &format!(
            "permoverview cid=2 cldbid={} permsid=i_channel_group_needed_modify_power",
            client_database_id
        ),
        &mut admin_session,
    );
    assert!(default_group_overview.contains("t=3"));
    assert!(default_group_overview.contains("id1=0"));
    assert!(default_group_overview.contains("id2=10"));
    assert!(default_group_overview.contains("v=75"));

    assert!(
        runtime
            .execute(
                &format!(
                    "setclientchannelgroup cgid={} cid=2 cldbid={}",
                    target_group_id, client_database_id
                ),
                &mut admin_session,
            )
            .contains("error id=0 msg=ok")
    );

    let assignments_for_group = runtime.execute(
        &format!("channelgroupclientlist cid=2 cgid={}", target_group_id),
        &mut admin_session,
    );
    assert!(assignments_for_group.contains(&format!("cldbid={}", client_database_id)));
    assert!(assignments_for_group.contains("cid=2"));

    let assignments_for_client = runtime.execute(
        &format!("channelgroupclientlist cldbid={}", client_database_id),
        &mut admin_session,
    );
    assert!(assignments_for_client.contains(&format!("cgid={}", target_group_id)));

    let delete_without_force = runtime.execute(
        &format!("channelgroupdel cgid={} force=0", target_group_id),
        &mut admin_session,
    );
    assert!(delete_without_force.contains("error id=512"));

    let delete_with_force = runtime.execute(
        &format!("channelgroupdel cgid={} force=1", target_group_id),
        &mut admin_session,
    );
    assert!(delete_with_force.contains("error id=0 msg=ok"));

    let groups_after_delete = runtime.execute("channelgrouplist", &mut admin_session);
    assert!(!groups_after_delete.contains(&format!("cgid={}", target_group_id)));

    let assignments_after_delete = runtime.execute(
        &format!("channelgroupclientlist cldbid={}", client_database_id),
        &mut admin_session,
    );
    assert!(!assignments_after_delete.contains(&format!("cgid={}", target_group_id)));

    let fallback_group_overview = runtime.execute(
        &format!(
            "permoverview cid=2 cldbid={} permsid=i_channel_group_needed_modify_power",
            client_database_id
        ),
        &mut admin_session,
    );
    assert!(fallback_group_overview.contains("t=3"));
    assert!(fallback_group_overview.contains("id1=0"));
    assert!(fallback_group_overview.contains("id2=10"));
    assert!(fallback_group_overview.contains("v=75"));
}

#[test]
fn runtime_persists_channelgroup_metadata_and_assignments() {
    let state_path = temp_state_path("channelgroup-persistence");
    cleanup_state_path(&state_path);
    let source_group_id;
    let copied_group_id;
    let client_database_id;

    {
        let mut runtime = create_baseline_runtime_with_state_path(workspace_root(), &state_path)
            .expect("runtime should build with custom state path");
        let mut session = QuerySessionState::default();

        assert!(
            runtime
                .execute("login serveradmin serveradmin", &mut session)
                .contains("error id=0 msg=ok")
        );
        assert!(
            runtime
                .execute("use sid=1", &mut session)
                .contains("error id=0 msg=ok")
        );

        let created_source_group = runtime.execute(
            r"channelgroupadd name=Persisted\sChannel\sOps type=1",
            &mut session,
        );
        source_group_id = extract_field(&created_source_group, "cgid")
            .expect("channelgroupadd should expose cgid")
            .parse::<u32>()
            .expect("cgid should parse");

        assert!(runtime
            .execute(
                &format!(
                    "channelgroupaddperm cgid={} permsid=i_client_talk_power permvalue=80 permnegated=0 permskip=0",
                    source_group_id
                ),
                &mut session,
            )
            .contains("error id=0 msg=ok"));

        let copied_group = runtime.execute(
            &format!(
                r"channelgroupcopy scgid={} tcgid=0 name=Persisted\sChannel\sCopy type=1",
                source_group_id
            ),
            &mut session,
        );
        copied_group_id = extract_field(&copied_group, "cgid")
            .expect("channelgroupcopy should expose cgid")
            .parse::<u32>()
            .expect("copied cgid should parse");

        assert!(
            runtime
                .execute(
                    &format!(
                        r"channelgrouprename cgid={} name=Persisted\sChannel\sRenamed",
                        copied_group_id
                    ),
                    &mut session,
                )
                .contains("error id=0 msg=ok")
        );

        let created_account = runtime.execute(
            "querycreate client_login_name=channelgroup_persist_bot client_login_password=channelgroup_persist_secret server_id=1",
            &mut session,
        );
        client_database_id = extract_field(&created_account, "cldbid")
            .expect("querycreate should expose cldbid")
            .parse::<u64>()
            .expect("cldbid should parse");

        assert!(
            runtime
                .execute(
                    &format!(
                        "setclientchannelgroup cgid={} cid=2 cldbid={}",
                        copied_group_id, client_database_id
                    ),
                    &mut session,
                )
                .contains("error id=0 msg=ok")
        );
    }

    {
        let mut runtime = create_baseline_runtime_with_state_path(workspace_root(), &state_path)
            .expect("runtime should reload with channel group metadata");
        let mut session = QuerySessionState::default();

        assert!(
            runtime
                .execute("login serveradmin serveradmin", &mut session)
                .contains("error id=0 msg=ok")
        );
        assert!(
            runtime
                .execute("use sid=1", &mut session)
                .contains("error id=0 msg=ok")
        );

        let groups = runtime.execute("channelgrouplist", &mut session);
        assert!(groups.contains(&format!("cgid={}", source_group_id)));
        assert!(groups.contains(&format!("cgid={}", copied_group_id)));
        assert!(groups.contains(r"name=Persisted\sChannel\sRenamed"));

        let permissions = runtime.execute(
            &format!("channelgrouppermlist cgid={} -permsid", copied_group_id),
            &mut session,
        );
        assert!(permissions.contains("permsid=i_client_talk_power"));
        assert!(permissions.contains("permvalue=80"));

        let assignments = runtime.execute(
            &format!("channelgroupclientlist cldbid={}", client_database_id),
            &mut session,
        );
        assert!(assignments.contains(&format!("cgid={}", copied_group_id)));
        assert!(assignments.contains("cid=2"));
    }

    cleanup_state_path(&state_path);
}

#[test]
fn phase_two_channelclient_permission_commands_work() {
    let mut runtime = create_test_runtime("phase-two-channelclient-permissions");
    let mut session = QuerySessionState::default();

    assert!(
        runtime
            .execute("login serveradmin serveradmin", &mut session)
            .contains("error id=0 msg=ok")
    );
    assert!(
        runtime
            .execute("use sid=1", &mut session)
            .contains("error id=0 msg=ok")
    );

    let created_account = runtime.execute(
        "querycreate client_login_name=channelclient_bot client_login_password=channelclient_secret server_id=1",
        &mut session,
    );
    let client_database_id = extract_field(&created_account, "cldbid")
        .expect("querycreate should expose cldbid")
        .parse::<u64>()
        .expect("cldbid should parse");

    assert!(runtime
        .execute(
            &format!(
                "channelclientaddperm cid=1 cldbid={} permsid=i_client_poke_power permvalue=33 permnegated=0 permskip=1|permsid=b_client_skip_channelgroup_permissions permvalue=1 permnegated=0 permskip=0",
                client_database_id
            ),
            &mut session,
        )
        .contains("error id=0 msg=ok"));

    let numeric_permlist = runtime.execute(
        &format!("channelclientpermlist cid=1 cldbid={}", client_database_id),
        &mut session,
    );
    assert!(numeric_permlist.contains("cid=1"));
    assert!(numeric_permlist.contains(&format!("cldbid={}", client_database_id)));
    assert!(numeric_permlist.contains("permid="));
    assert!(!numeric_permlist.contains("permsid="));

    let named_permlist = runtime.execute(
        &format!(
            "channelclientpermlist cid=1 cldbid={} -permsid",
            client_database_id
        ),
        &mut session,
    );
    assert!(named_permlist.contains("permsid=i_client_poke_power"));
    assert!(named_permlist.contains("permvalue=33"));
    assert!(named_permlist.contains("permskip=1"));
    assert!(named_permlist.contains("permsid=b_client_skip_channelgroup_permissions"));

    let permfind = runtime.execute("permfind permsid=i_client_poke_power", &mut session);
    assert!(permfind.contains("t=4"));
    assert!(permfind.contains("id1=1"));
    assert!(permfind.contains(&format!("id2={}", client_database_id)));

    let permoverview = runtime.execute(
        &format!(
            "permoverview cid=1 cldbid={} permsid=i_client_poke_power",
            client_database_id
        ),
        &mut session,
    );
    assert!(permoverview.contains("t=4"));
    assert!(permoverview.contains("id1=1"));
    assert!(permoverview.contains(&format!("id2={}", client_database_id)));
    assert!(permoverview.contains("v=33"));
    assert!(permoverview.contains("s=1"));

    assert!(
        runtime
            .execute(
                &format!(
                    "channelclientdelperm cid=1 cldbid={} permsid=i_client_poke_power",
                    client_database_id
                ),
                &mut session,
            )
            .contains("error id=0 msg=ok")
    );

    let permlist_after_delete = runtime.execute(
        &format!(
            "channelclientpermlist cid=1 cldbid={} -permsid",
            client_database_id
        ),
        &mut session,
    );
    assert!(!permlist_after_delete.contains("permsid=i_client_poke_power"));
    assert!(permlist_after_delete.contains("permsid=b_client_skip_channelgroup_permissions"));
}

#[test]
fn runtime_persists_channelclient_permissions() {
    let state_path = temp_state_path("channelclient-permissions-persistence");
    cleanup_state_path(&state_path);
    let client_database_id;

    {
        let mut runtime = create_baseline_runtime_with_state_path(workspace_root(), &state_path)
            .expect("runtime should build with custom state path");
        let mut session = QuerySessionState::default();

        assert!(
            runtime
                .execute("login serveradmin serveradmin", &mut session)
                .contains("error id=0 msg=ok")
        );
        assert!(
            runtime
                .execute("use sid=1", &mut session)
                .contains("error id=0 msg=ok")
        );

        let created_account = runtime.execute(
            "querycreate client_login_name=channelclient_persist_bot client_login_password=channelclient_persist_secret server_id=1",
            &mut session,
        );
        client_database_id = extract_field(&created_account, "cldbid")
            .expect("querycreate should expose cldbid")
            .parse::<u64>()
            .expect("cldbid should parse");

        assert!(runtime
            .execute(
                &format!(
                    "channelclientaddperm cid=2 cldbid={} permsid=i_client_poke_power permvalue=44 permnegated=0 permskip=1",
                    client_database_id
                ),
                &mut session,
            )
            .contains("error id=0 msg=ok"));
    }

    {
        let mut runtime = create_baseline_runtime_with_state_path(workspace_root(), &state_path)
            .expect("runtime should reload with channel-client permission metadata");
        let mut session = QuerySessionState::default();

        assert!(
            runtime
                .execute("login serveradmin serveradmin", &mut session)
                .contains("error id=0 msg=ok")
        );
        assert!(
            runtime
                .execute("use sid=1", &mut session)
                .contains("error id=0 msg=ok")
        );

        let permlist = runtime.execute(
            &format!(
                "channelclientpermlist cid=2 cldbid={} -permsid",
                client_database_id
            ),
            &mut session,
        );
        assert!(permlist.contains("permsid=i_client_poke_power"));
        assert!(permlist.contains("permvalue=44"));
        assert!(permlist.contains("permskip=1"));

        let permoverview = runtime.execute(
            &format!(
                "permoverview cid=2 cldbid={} permsid=i_client_poke_power",
                client_database_id
            ),
            &mut session,
        );
        assert!(permoverview.contains("t=4"));
        assert!(permoverview.contains("id1=2"));
        assert!(permoverview.contains(&format!("id2={}", client_database_id)));
        assert!(permoverview.contains("v=44"));
    }

    cleanup_state_path(&state_path);
}

#[test]
fn phase_two_channel_permission_commands_work() {
    let mut runtime = create_test_runtime("phase-two-channel-permissions");
    let mut session = QuerySessionState::default();

    assert!(
        runtime
            .execute("login serveradmin serveradmin", &mut session)
            .contains("error id=0 msg=ok")
    );
    assert!(
        runtime
            .execute("use sid=1", &mut session)
            .contains("error id=0 msg=ok")
    );

    assert!(runtime
        .execute(
            "channeladdperm cid=2 permsid=i_client_talk_power permvalue=66 permnegated=0 permskip=0|permsid=b_channel_join_permanent permvalue=1 permnegated=0 permskip=0",
            &mut session,
        )
        .contains("error id=0 msg=ok"));

    let numeric_permlist = runtime.execute("channelpermlist cid=2", &mut session);
    assert!(numeric_permlist.contains("cid=2"));
    assert!(numeric_permlist.contains("permid="));
    assert!(!numeric_permlist.contains("permsid="));

    let named_permlist = runtime.execute("channelpermlist cid=2 -permsid", &mut session);
    assert!(named_permlist.contains("permsid=i_client_talk_power"));
    assert!(named_permlist.contains("permvalue=66"));
    assert!(named_permlist.contains("permsid=b_channel_join_permanent"));

    let permfind = runtime.execute("permfind permsid=i_client_talk_power", &mut session);
    assert!(permfind.contains("t=2"));
    assert!(permfind.contains("id1=2"));
    assert!(permfind.contains("id2=0"));

    let created_account = runtime.execute(
        "querycreate client_login_name=channel_perm_bot client_login_password=channel_perm_secret server_id=1",
        &mut session,
    );
    let client_database_id = extract_field(&created_account, "cldbid")
        .expect("querycreate should expose cldbid")
        .parse::<u64>()
        .expect("cldbid should parse");

    let permoverview = runtime.execute(
        &format!(
            "permoverview cid=2 cldbid={} permsid=i_client_talk_power",
            client_database_id
        ),
        &mut session,
    );
    assert!(permoverview.contains("t=2"));
    assert!(permoverview.contains("id1=2"));
    assert!(permoverview.contains("id2=0"));
    assert!(permoverview.contains("v=66"));

    assert!(
        runtime
            .execute(
                "channeldelperm cid=2 permsid=i_client_talk_power",
                &mut session
            )
            .contains("error id=0 msg=ok")
    );

    let permlist_after_delete = runtime.execute("channelpermlist cid=2 -permsid", &mut session);
    assert!(!permlist_after_delete.contains("permsid=i_client_talk_power"));
    assert!(permlist_after_delete.contains("permsid=b_channel_join_permanent"));
}

#[test]
fn permoverview_orders_overlapping_targets_stably() {
    let mut runtime = create_test_runtime("permoverview-overlap-order");
    let mut session = QuerySessionState::default();

    assert!(
        runtime
            .execute("login serveradmin serveradmin", &mut session)
            .contains("error id=0 msg=ok")
    );
    assert!(
        runtime
            .execute("use sid=1", &mut session)
            .contains("error id=0 msg=ok")
    );

    let permission_name = "i_compat_permoverview_order";

    let created_account = runtime.execute(
        "querycreate client_login_name=permoverview_bot client_login_password=permoverview_secret server_id=1",
        &mut session,
    );
    let client_database_id = extract_field(&created_account, "cldbid")
        .expect("querycreate should expose cldbid")
        .parse::<u64>()
        .expect("cldbid should parse");

    let created_server_group = runtime.execute(
        r"servergroupadd name=Permoverview\sOrder\sSG type=1",
        &mut session,
    );
    let server_group_id = extract_field(&created_server_group, "sgid")
        .expect("servergroupadd should expose sgid")
        .parse::<u32>()
        .expect("sgid should parse");

    assert!(
        runtime
            .execute(
                &format!(
                    "servergroupaddperm sgid={} permsid={} permvalue=10 permnegated=0 permskip=0",
                    server_group_id, permission_name
                ),
                &mut session,
            )
            .contains("error id=0 msg=ok")
    );
    assert!(
        runtime
            .execute(
                &format!(
                    "servergroupaddclient sgid={} cldbid={}",
                    server_group_id, client_database_id
                ),
                &mut session,
            )
            .contains("error id=0 msg=ok")
    );

    assert!(
        runtime
            .execute(
                &format!(
                    "clientaddperm cldbid={} permsid={} permvalue=20 permnegated=0 permskip=0",
                    client_database_id, permission_name
                ),
                &mut session,
            )
            .contains("error id=0 msg=ok")
    );

    assert!(
        runtime
            .execute(
                &format!(
                    "channeladdperm cid=2 permsid={} permvalue=30 permnegated=0 permskip=0",
                    permission_name
                ),
                &mut session,
            )
            .contains("error id=0 msg=ok")
    );

    let created_channel_group = runtime.execute(
        r"channelgroupadd name=Permoverview\sOrder\sCG type=1",
        &mut session,
    );
    let channel_group_id = extract_field(&created_channel_group, "cgid")
        .expect("channelgroupadd should expose cgid")
        .parse::<u32>()
        .expect("cgid should parse");

    assert!(
        runtime
            .execute(
                &format!(
                    "channelgroupaddperm cgid={} permsid={} permvalue=40 permnegated=0 permskip=0",
                    channel_group_id, permission_name
                ),
                &mut session,
            )
            .contains("error id=0 msg=ok")
    );
    assert!(
        runtime
            .execute(
                &format!(
                    "setclientchannelgroup cgid={} cid=2 cldbid={}",
                    channel_group_id, client_database_id
                ),
                &mut session,
            )
            .contains("error id=0 msg=ok")
    );

    assert!(runtime
        .execute(
            &format!(
                "channelclientaddperm cid=2 cldbid={} permsid={} permvalue=50 permnegated=0 permskip=0",
                client_database_id, permission_name
            ),
            &mut session,
        )
        .contains("error id=0 msg=ok"));

    let permoverview = runtime.execute(
        &format!(
            "permoverview cid=2 cldbid={} permsid={}",
            client_database_id, permission_name
        ),
        &mut session,
    );
    let rows = extract_rows(&permoverview);
    assert_eq!(rows.len(), 5);

    let target_types = rows
        .iter()
        .map(|row| {
            row.get("t")
                .expect("permoverview row should have target type")
                .as_str()
        })
        .collect::<Vec<_>>();
    assert_eq!(target_types, vec!["0", "1", "2", "3", "4"]);

    let values = rows
        .iter()
        .map(|row| {
            row.get("v")
                .expect("permoverview row should have value")
                .as_str()
        })
        .collect::<Vec<_>>();
    assert_eq!(values, vec!["10", "20", "30", "40", "50"]);

    assert_eq!(
        rows[0].get("id1").map(String::as_str),
        Some(server_group_id.to_string().as_str())
    );
    assert_eq!(
        rows[1].get("id1").map(String::as_str),
        Some(client_database_id.to_string().as_str())
    );
    assert_eq!(rows[2].get("id1").map(String::as_str), Some("2"));
    assert_eq!(
        rows[3].get("id2").map(String::as_str),
        Some(channel_group_id.to_string().as_str())
    );
    assert_eq!(
        rows[4].get("id2").map(String::as_str),
        Some(client_database_id.to_string().as_str())
    );
}

#[test]
fn permfind_orders_overlapping_targets_stably() {
    let mut runtime = create_test_runtime("permfind-overlap-order");
    let mut session = QuerySessionState::default();

    assert!(
        runtime
            .execute("login serveradmin serveradmin", &mut session)
            .contains("error id=0 msg=ok")
    );
    assert!(
        runtime
            .execute("use sid=1", &mut session)
            .contains("error id=0 msg=ok")
    );

    let permission_name = "i_compat_permfind_order";

    let created_account = runtime.execute(
        "querycreate client_login_name=permfind_bot client_login_password=permfind_secret server_id=1",
        &mut session,
    );
    let client_database_id = extract_field(&created_account, "cldbid")
        .expect("querycreate should expose cldbid")
        .parse::<u64>()
        .expect("cldbid should parse");

    let created_server_group = runtime.execute(
        r"servergroupadd name=Permfind\sOrder\sSG type=1",
        &mut session,
    );
    let server_group_id = extract_field(&created_server_group, "sgid")
        .expect("servergroupadd should expose sgid")
        .parse::<u32>()
        .expect("sgid should parse");

    assert!(
        runtime
            .execute(
                &format!(
                    "servergroupaddperm sgid={} permsid={} permvalue=10 permnegated=0 permskip=0",
                    server_group_id, permission_name
                ),
                &mut session,
            )
            .contains("error id=0 msg=ok")
    );
    assert!(
        runtime
            .execute(
                &format!(
                    "servergroupaddclient sgid={} cldbid={}",
                    server_group_id, client_database_id
                ),
                &mut session,
            )
            .contains("error id=0 msg=ok")
    );

    assert!(
        runtime
            .execute(
                &format!(
                    "clientaddperm cldbid={} permsid={} permvalue=20 permnegated=0 permskip=0",
                    client_database_id, permission_name
                ),
                &mut session,
            )
            .contains("error id=0 msg=ok")
    );

    assert!(
        runtime
            .execute(
                &format!(
                    "channeladdperm cid=2 permsid={} permvalue=30 permnegated=0 permskip=0",
                    permission_name
                ),
                &mut session,
            )
            .contains("error id=0 msg=ok")
    );

    let created_channel_group = runtime.execute(
        r"channelgroupadd name=Permfind\sOrder\sCG type=1",
        &mut session,
    );
    let channel_group_id = extract_field(&created_channel_group, "cgid")
        .expect("channelgroupadd should expose cgid")
        .parse::<u32>()
        .expect("cgid should parse");

    assert!(
        runtime
            .execute(
                &format!(
                    "channelgroupaddperm cgid={} permsid={} permvalue=40 permnegated=0 permskip=0",
                    channel_group_id, permission_name
                ),
                &mut session,
            )
            .contains("error id=0 msg=ok")
    );
    assert!(
        runtime
            .execute(
                &format!(
                    "setclientchannelgroup cgid={} cid=2 cldbid={}",
                    channel_group_id, client_database_id
                ),
                &mut session,
            )
            .contains("error id=0 msg=ok")
    );

    assert!(runtime
        .execute(
            &format!(
                "channelclientaddperm cid=2 cldbid={} permsid={} permvalue=50 permnegated=0 permskip=0",
                client_database_id, permission_name
            ),
            &mut session,
        )
        .contains("error id=0 msg=ok"));

    let permfind = runtime.execute(
        &format!("permfind permsid={}", permission_name),
        &mut session,
    );
    let rows = extract_rows(&permfind);
    assert_eq!(rows.len(), 5);

    let target_types = rows
        .iter()
        .map(|row| {
            row.get("t")
                .expect("permfind row should have target type")
                .as_str()
        })
        .collect::<Vec<_>>();
    assert_eq!(target_types, vec!["0", "1", "2", "3", "4"]);

    let permission_ids = rows
        .iter()
        .map(|row| {
            row.get("p")
                .expect("permfind row should have permission id")
                .as_str()
        })
        .collect::<Vec<_>>();
    assert!(
        permission_ids
            .windows(2)
            .all(|window| window[0] == window[1])
    );

    assert_eq!(
        rows[0].get("id1").map(String::as_str),
        Some(server_group_id.to_string().as_str())
    );
    assert_eq!(
        rows[1].get("id1").map(String::as_str),
        Some(client_database_id.to_string().as_str())
    );
    assert_eq!(rows[2].get("id1").map(String::as_str), Some("2"));
    assert_eq!(
        rows[3].get("id2").map(String::as_str),
        Some(channel_group_id.to_string().as_str())
    );
    assert_eq!(
        rows[4].get("id2").map(String::as_str),
        Some(client_database_id.to_string().as_str())
    );
}

#[test]
fn runtime_persists_channel_permissions() {
    let state_path = temp_state_path("channel-permissions-persistence");
    cleanup_state_path(&state_path);

    {
        let mut runtime = create_baseline_runtime_with_state_path(workspace_root(), &state_path)
            .expect("runtime should build with custom state path");
        let mut session = QuerySessionState::default();

        assert!(
            runtime
                .execute("login serveradmin serveradmin", &mut session)
                .contains("error id=0 msg=ok")
        );
        assert!(
            runtime
                .execute("use sid=1", &mut session)
                .contains("error id=0 msg=ok")
        );

        assert!(runtime
            .execute(
                "channeladdperm cid=1 permsid=i_client_needed_talk_power permvalue=23 permnegated=0 permskip=1",
                &mut session,
            )
            .contains("error id=0 msg=ok"));
    }

    {
        let mut runtime = create_baseline_runtime_with_state_path(workspace_root(), &state_path)
            .expect("runtime should reload with channel permission metadata");
        let mut session = QuerySessionState::default();

        assert!(
            runtime
                .execute("login serveradmin serveradmin", &mut session)
                .contains("error id=0 msg=ok")
        );
        assert!(
            runtime
                .execute("use sid=1", &mut session)
                .contains("error id=0 msg=ok")
        );

        let permlist = runtime.execute("channelpermlist cid=1 -permsid", &mut session);
        assert!(permlist.contains("permsid=i_client_needed_talk_power"));
        assert!(permlist.contains("permvalue=23"));
        assert!(permlist.contains("permskip=1"));

        let permfind = runtime.execute("permfind permsid=i_client_needed_talk_power", &mut session);
        assert!(permfind.contains("t=2"));
        assert!(permfind.contains("id1=1"));
        assert!(permfind.contains("id2=0"));
    }

    cleanup_state_path(&state_path);
}

#[test]
fn runtime_persists_tokens_and_actions() {
    let state_path = temp_state_path("token-persistence");
    cleanup_state_path(&state_path);
    let token_id;
    let token_value;
    let action_id;

    {
        let mut runtime = create_baseline_runtime_with_state_path(workspace_root(), &state_path)
            .expect("runtime should build with custom state path");
        let mut session = QuerySessionState::default();

        assert!(
            runtime
                .execute("login serveradmin serveradmin", &mut session)
                .contains("error id=0 msg=ok")
        );
        assert!(
            runtime
                .execute("use sid=1", &mut session)
                .contains("error id=0 msg=ok")
        );

        let created_token = runtime.execute(
            r"tokenadd token_description=Persisted\sToken token_max_uses=5 action_type=2 action_id1=8",
            &mut session,
        );
        token_id = extract_field(&created_token, "token_id")
            .expect("tokenadd should expose token_id")
            .parse::<u32>()
            .expect("token_id should parse");
        token_value = extract_field(&created_token, "token")
            .expect("tokenadd should expose token")
            .to_string();
        action_id = extract_field(&created_token, "action_id")
            .expect("tokenadd should expose action_id")
            .parse::<u32>()
            .expect("action_id should parse");
    }

    {
        let mut runtime = create_baseline_runtime_with_state_path(workspace_root(), &state_path)
            .expect("runtime should reload with token metadata");
        let mut session = QuerySessionState::default();

        assert!(
            runtime
                .execute("login serveradmin serveradmin", &mut session)
                .contains("error id=0 msg=ok")
        );
        assert!(
            runtime
                .execute("use sid=1", &mut session)
                .contains("error id=0 msg=ok")
        );

        let tokenlist = runtime.execute("tokenlist -new", &mut session);
        assert!(tokenlist.contains(&format!("token_id={}", token_id)));
        assert!(tokenlist.contains(&format!("token={}", token_value.clone())));
        assert!(tokenlist.contains(r"token_description=Persisted\sToken"));

        let actions = runtime.execute(
            &format!("tokenactionlist token={}", token_value.clone()),
            &mut session,
        );
        assert!(actions.contains(&format!("action_id={}", action_id)));
        assert!(actions.contains("action_type=2"));
        assert!(actions.contains("action_id1=8"));
    }

    cleanup_state_path(&state_path);
}

#[test]
fn runtime_persists_token_use_counters() {
    let state_path = temp_state_path("token-use-persistence");
    cleanup_state_path(&state_path);
    let token_value;

    {
        let mut runtime = create_baseline_runtime_with_state_path(workspace_root(), &state_path)
            .expect("runtime should build with custom state path");
        let mut admin_session = QuerySessionState::default();
        let mut token_session = QuerySessionState::default();

        assert!(
            runtime
                .execute("login serveradmin serveradmin", &mut admin_session)
                .contains("error id=0 msg=ok")
        );
        assert!(
            runtime
                .execute("use sid=1", &mut admin_session)
                .contains("error id=0 msg=ok")
        );
        assert!(runtime
            .execute(
                "querycreate client_login_name=token_persist_bot client_login_password=token_persist_secret server_id=1",
                &mut admin_session,
            )
            .contains("client_login_name=token_persist_bot"));

        let created_token = runtime.execute(
            r"tokenadd token_description=Restartable\sGrant token_max_uses=2 action_type=2 action_id1=8",
            &mut admin_session,
        );
        token_value = extract_field(&created_token, "token")
            .expect("tokenadd should expose token")
            .to_string();

        assert!(
            runtime
                .execute(
                    "login token_persist_bot token_persist_secret",
                    &mut token_session
                )
                .contains("error id=0 msg=ok")
        );
        assert!(
            runtime
                .execute("use sid=1", &mut token_session)
                .contains("error id=0 msg=ok")
        );
        assert!(
            runtime
                .execute(
                    &format!("tokenuse token={}", token_value.clone()),
                    &mut token_session
                )
                .contains("error id=0 msg=ok")
        );
    }

    {
        let mut runtime = create_baseline_runtime_with_state_path(workspace_root(), &state_path)
            .expect("runtime should reload with token usage state");
        let mut admin_session = QuerySessionState::default();
        let mut token_session = QuerySessionState::default();

        assert!(
            runtime
                .execute("login serveradmin serveradmin", &mut admin_session)
                .contains("error id=0 msg=ok")
        );
        assert!(
            runtime
                .execute("use sid=1", &mut admin_session)
                .contains("error id=0 msg=ok")
        );

        let tokenlist_after_reload = runtime.execute("tokenlist", &mut admin_session);
        assert!(tokenlist_after_reload.contains(&token_value));

        assert!(
            runtime
                .execute(
                    "login token_persist_bot token_persist_secret",
                    &mut token_session
                )
                .contains("error id=0 msg=ok")
        );
        assert!(
            runtime
                .execute("use sid=1", &mut token_session)
                .contains("error id=0 msg=ok")
        );
        assert!(
            runtime
                .execute(
                    &format!("privilegekeyuse token={}", token_value.clone()),
                    &mut token_session
                )
                .contains("error id=0 msg=ok")
        );

        let tokenlist_after_second_use = runtime.execute("tokenlist", &mut admin_session);
        assert!(!tokenlist_after_second_use.contains(&token_value));
    }

    cleanup_state_path(&state_path);
}

#[test]
fn runtime_persists_servergroup_copy_rename_and_auto_permission_state() {
    let state_path = temp_state_path("servergroup-copy-auto-persistence");
    cleanup_state_path(&state_path);
    let source_group_id;
    let copied_group_id;

    {
        let mut runtime = create_baseline_runtime_with_state_path(workspace_root(), &state_path)
            .expect("runtime should build with custom state path");
        let mut session = QuerySessionState::default();

        assert!(
            runtime
                .execute("login serveradmin serveradmin", &mut session)
                .contains("error id=0 msg=ok")
        );
        assert!(
            runtime
                .execute("use sid=1", &mut session)
                .contains("error id=0 msg=ok")
        );

        let created_source_group =
            runtime.execute(r"servergroupadd name=Persisted\sOps type=1", &mut session);
        source_group_id = extract_field(&created_source_group, "sgid")
            .expect("servergroupadd should expose sgid")
            .parse::<u32>()
            .expect("sgid should parse");

        assert!(runtime
            .execute(
                &format!(
                    "servergroupaddperm sgid={} permsid=i_group_auto_update_type permvalue=45 permnegated=0 permskip=0",
                    source_group_id
                ),
                &mut session,
            )
            .contains("error id=0 msg=ok"));

        let copied_group = runtime.execute(
            &format!(
                r"servergroupcopy ssgid={} tsgid=0 name=Persisted\sCopy type=1",
                source_group_id
            ),
            &mut session,
        );
        copied_group_id = extract_field(&copied_group, "sgid")
            .expect("servergroupcopy should expose sgid")
            .parse::<u32>()
            .expect("copied sgid should parse");

        assert!(
            runtime
                .execute(
                    &format!(
                        r"servergrouprename sgid={} name=Persisted\sRenamed",
                        copied_group_id
                    ),
                    &mut session,
                )
                .contains("error id=0 msg=ok")
        );

        assert!(runtime
            .execute(
                "servergroupautoaddperm sgtype=45 permsid=b_virtualserver_start permvalue=1 permnegated=0 permskip=0",
                &mut session,
            )
            .contains("error id=0 msg=ok"));
        assert!(
            runtime
                .execute(
                    "servergroupautodelperm sgtype=45 permsid=b_virtualserver_start",
                    &mut session
                )
                .contains("error id=0 msg=ok")
        );
    }

    {
        let mut runtime = create_baseline_runtime_with_state_path(workspace_root(), &state_path)
            .expect("runtime should reload with server group metadata");
        let mut session = QuerySessionState::default();

        assert!(
            runtime
                .execute("login serveradmin serveradmin", &mut session)
                .contains("error id=0 msg=ok")
        );
        assert!(
            runtime
                .execute("use sid=1", &mut session)
                .contains("error id=0 msg=ok")
        );

        let groups = runtime.execute("servergrouplist", &mut session);
        assert!(groups.contains(&format!("sgid={}", source_group_id)));
        assert!(groups.contains(&format!("sgid={}", copied_group_id)));
        assert!(groups.contains(r"name=Persisted\sRenamed"));

        let source_permissions = runtime.execute(
            &format!("servergrouppermlist sgid={} -permsid", source_group_id),
            &mut session,
        );
        assert!(source_permissions.contains("permsid=i_group_auto_update_type"));
        assert!(!source_permissions.contains("permsid=b_virtualserver_start"));

        let copied_permissions = runtime.execute(
            &format!("servergrouppermlist sgid={} -permsid", copied_group_id),
            &mut session,
        );
        assert!(copied_permissions.contains("permsid=i_group_auto_update_type"));
        assert!(!copied_permissions.contains("permsid=b_virtualserver_start"));
    }

    cleanup_state_path(&state_path);
}

#[test]
fn runtime_persists_group_and_permission_metadata() {
    let state_path = temp_state_path("permission-metadata");
    cleanup_state_path(&state_path);
    let client_database_id;

    {
        let mut runtime = create_baseline_runtime_with_state_path(workspace_root(), &state_path)
            .expect("runtime should build with custom state path");
        let mut session = QuerySessionState::default();

        assert!(
            runtime
                .execute("login serveradmin serveradmin", &mut session)
                .contains("error id=0 msg=ok")
        );
        let created = runtime.execute(
            "querycreate client_login_name=perm_bot client_login_password=perm_secret",
            &mut session,
        );
        assert!(created.contains("client_login_name=perm_bot"));
        client_database_id = extract_field(&created, "cldbid")
            .expect("querycreate should expose cldbid")
            .parse::<u64>()
            .expect("cldbid should parse");

        let group_permissions = runtime.execute(
            "servergroupaddperm sgid=8 permsid=b_virtualserver_notify_register permvalue=1 permnegated=0 permskip=0|permsid=b_virtualserver_notify_unregister permvalue=1 permnegated=0 permskip=0",
            &mut session,
        );
        assert!(group_permissions.contains("error id=0 msg=ok"));
        assert!(
            runtime
                .execute(
                    &format!("servergroupaddclient sgid=8 cldbid={}", client_database_id),
                    &mut session
                )
                .contains("error id=0 msg=ok")
        );

        let client_permissions = runtime.execute(
            &format!(
                "clientaddperm cldbid={} permsid=i_client_private_textmessage_power permvalue=50 permskip=0|permsid=i_client_poke_power permvalue=25 permskip=1",
                client_database_id
            ),
            &mut session,
        );
        assert!(client_permissions.contains("error id=0 msg=ok"));
    }

    {
        let mut runtime = create_baseline_runtime_with_state_path(workspace_root(), &state_path)
            .expect("runtime should reload with group metadata");
        let mut admin_session = QuerySessionState::default();
        let mut bot_session = QuerySessionState::default();

        assert!(
            runtime
                .execute("login serveradmin serveradmin", &mut admin_session)
                .contains("error id=0 msg=ok")
        );
        let groups = runtime.execute(
            &format!("servergroupsbyclientid cldbid={}", client_database_id),
            &mut admin_session,
        );
        assert!(groups.contains("sgid=7"));
        assert!(groups.contains("sgid=8"));

        let group_clients =
            runtime.execute("servergroupclientlist sgid=8 -names", &mut admin_session);
        assert!(group_clients.contains(&format!("cldbid={}", client_database_id)));
        assert!(group_clients.contains("name=perm_bot"));

        let group_permissions =
            runtime.execute("servergrouppermlist sgid=8 -permsid", &mut admin_session);
        assert!(group_permissions.contains("permsid=b_virtualserver_notify_register"));
        assert!(group_permissions.contains("permsid=b_virtualserver_notify_unregister"));

        let client_permissions = runtime.execute(
            &format!("clientpermlist cldbid={} -permsid", client_database_id),
            &mut admin_session,
        );
        assert!(client_permissions.contains("permsid=i_client_private_textmessage_power"));
        assert!(client_permissions.contains("permsid=i_client_poke_power"));

        let found_client_permission = runtime.execute(
            "permfind permsid=i_client_private_textmessage_power",
            &mut admin_session,
        );
        assert!(found_client_permission.contains("t=1"));
        assert!(found_client_permission.contains(&format!("id1={}", client_database_id)));

        assert!(
            runtime
                .execute("login perm_bot perm_secret", &mut bot_session)
                .contains("error id=0 msg=ok")
        );
        let effective_group_permission = runtime.execute(
            "permget permsid=b_virtualserver_notify_register",
            &mut bot_session,
        );
        assert!(effective_group_permission.contains("permsid=b_virtualserver_notify_register"));
        assert!(effective_group_permission.contains("permvalue=1"));

        let effective_direct_permission = runtime.execute(
            "permget permsid=i_client_private_textmessage_power",
            &mut bot_session,
        );
        assert!(effective_direct_permission.contains("permsid=i_client_private_textmessage_power"));
        assert!(effective_direct_permission.contains("permvalue=50"));
    }

    cleanup_state_path(&state_path);
}

#[test]
fn permission_discovery_lists_and_resolves_known_and_dynamic_permissions() {
    let mut runtime = create_test_runtime("permission-discovery");
    let mut session = QuerySessionState::default();

    assert!(
        runtime
            .execute("login serveradmin serveradmin", &mut session)
            .contains("error id=0 msg=ok")
    );
    let created = runtime.execute(
        "querycreate client_login_name=lookup_bot client_login_password=lookup_secret",
        &mut session,
    );
    let client_database_id = extract_field(&created, "cldbid")
        .expect("querycreate should expose cldbid")
        .parse::<u64>()
        .expect("cldbid should parse");

    let add_dynamic_permission = runtime.execute(
        &format!(
            "clientaddperm cldbid={} permsid=custom_runtime_probe permvalue=7 permskip=0",
            client_database_id
        ),
        &mut session,
    );
    assert!(add_dynamic_permission.contains("error id=0 msg=ok"));

    let permissionlist = runtime.execute("permissionlist", &mut session);
    assert!(permissionlist.contains("permname=b_serverinstance_help_view"));
    assert!(permissionlist.contains("permname=b_client_channel_textmessage_send"));
    assert!(permissionlist.contains(r"permdesc=Send\stext\smessages\sto\schannel"));
    assert!(permissionlist.contains("permname=custom_runtime_probe"));
    assert!(permissionlist.contains(r"permdesc=Custom\sruntime\sprobe"));

    let resolved = runtime.execute(
        "permidgetbyname permsid=b_serverinstance_help_view|permsid=custom_runtime_probe",
        &mut session,
    );
    assert!(resolved.contains("permsid=b_serverinstance_help_view"));
    assert!(resolved.contains("permid=4353"));
    assert!(resolved.contains("permsid=custom_runtime_probe"));
    assert!(resolved.contains("permid="));
}

#[test]
fn notification_registration_roundtrip() {
    let mut runtime = create_test_runtime("notification-roundtrip");
    let mut session = QuerySessionState::default();

    runtime.execute("login serveradmin serveradmin", &mut session);

    let register_server = runtime.execute("servernotifyregister event=server", &mut session);
    assert!(register_server.contains("error id=0 msg=ok"));
    assert_eq!(session.notification_subscriptions.len(), 1);

    let register_channel = runtime.execute("servernotifyregister event=channel id=1", &mut session);
    assert!(register_channel.contains("error id=0 msg=ok"));
    assert_eq!(session.notification_subscriptions.len(), 2);

    let unregister = runtime.execute("servernotifyunregister", &mut session);
    assert!(unregister.contains("error id=0 msg=ok"));
    assert!(session.notification_subscriptions.is_empty());
}

#[test]
fn clientmove_and_channeledit_update_channel_state() {
    let mut runtime = create_test_runtime("clientmove-channeledit");
    let mut session = QuerySessionState::default();

    runtime.execute("login serveradmin serveradmin", &mut session);

    let moved = runtime.execute("clientmove cid=2", &mut session);
    assert!(moved.contains("error id=0 msg=ok"));
    assert_eq!(session.current_channel_id, Some(2));

    let edited = runtime.execute(
        "channeledit cid=2 channel_name=Focus\\sRoom channel_topic=Deep\\sWork",
        &mut session,
    );
    assert!(edited.contains("error id=0 msg=ok"));

    let rendered = runtime.execute("channellist", &mut session);
    assert!(rendered.contains("channel_name=Focus\\sRoom"));
    assert!(rendered.contains("channel_topic=Deep\\sWork"));
}

#[test]
fn channelcreate_move_and_delete_update_channel_tree() {
    let mut runtime = create_test_runtime("channel-tree-roundtrip");
    let mut session = QuerySessionState::default();

    runtime.execute("login serveradmin serveradmin", &mut session);

    let created = runtime.execute(
        "channelcreate channel_name=Ops\\sRoom cpid=1 order=0 channel_topic=Build\\sQueue",
        &mut session,
    );
    let channel_id = created
        .lines()
        .next()
        .and_then(|line| {
            line.split_whitespace().find_map(|part| {
                part.split_once('=')
                    .and_then(|(key, value)| (key == "cid").then(|| value.to_string()))
            })
        })
        .expect("channelcreate should expose cid");
    assert!(created.contains("error id=0 msg=ok"));

    let listed_after_create = runtime.execute("channellist", &mut session);
    assert!(listed_after_create.contains(&format!("cid={} pid=1", channel_id)));
    assert!(listed_after_create.contains("channel_name=Ops\\sRoom"));

    let moved = runtime.execute(
        &format!("channelmove cid={} cpid=0 order=0", channel_id),
        &mut session,
    );
    assert!(moved.contains("error id=0 msg=ok"));

    let listed_after_move = runtime.execute("channellist", &mut session);
    assert!(listed_after_move.contains(&format!("cid={} pid=0", channel_id)));

    let deleted = runtime.execute(
        &format!("channeldelete cid={} force=1", channel_id),
        &mut session,
    );
    assert!(deleted.contains("error id=0 msg=ok"));

    let listed_after_delete = runtime.execute("channellist", &mut session);
    assert!(!listed_after_delete.contains(&format!("cid={}", channel_id)));
}

#[test]
fn serverlist_clientlist_and_clientinfo_reflect_online_clients() {
    let mut runtime = create_test_runtime("server-client-views");
    let mut session = QuerySessionState::default();

    let login = runtime.execute("login serveradmin serveradmin", &mut session);
    assert!(login.contains("error id=0 msg=ok"));

    let serverlist = runtime.execute("serverlist -uid", &mut session);
    assert!(serverlist.contains("virtualserver_id=1"));
    assert!(serverlist.contains("virtualserver_status=online"));
    assert!(serverlist.contains("virtualserver_clientsonline=4"));
    assert!(serverlist.contains("virtualserver_unique_identifier=compat-baseline-uid"));

    let clientlist = runtime.execute("clientlist -uid -groups -country -ip", &mut session);
    assert!(clientlist.contains("client_nickname=serveradmin"));
    assert!(clientlist.contains("client_nickname=ScP"));
    assert!(clientlist.contains(r"client_nickname=DJ\sMix"));
    assert!(clientlist.contains("client_unique_identifier=serveradmin"));

    let whoami = runtime.execute("whoami", &mut session);
    let clid = whoami
        .split_whitespace()
        .find_map(|part| {
            part.split_once('=')
                .and_then(|(key, value)| (key == "clid").then(|| value.to_string()))
        })
        .expect("whoami should expose clid");

    let clientinfo = runtime.execute(&format!("clientinfo clid={}", clid), &mut session);
    assert!(clientinfo.contains("client_nickname=serveradmin"));
    assert!(clientinfo.contains("client_type=1"));
    assert!(clientinfo.contains("client_platform=compat-rust"));
    assert!(clientinfo.contains("connection_client_ip=127.0.0.1"));
}

#[test]
fn sendtextmessage_validates_all_three_target_modes() {
    let mut runtime = create_test_runtime("sendtextmessage-targets");
    let mut session = QuerySessionState::default();

    runtime.execute("login serveradmin serveradmin", &mut session);
    let whoami = runtime.execute("whoami", &mut session);
    let clid = whoami
        .split_whitespace()
        .find_map(|part| {
            part.split_once('=')
                .and_then(|(key, value)| (key == "clid").then(|| value.to_string()))
        })
        .expect("whoami should expose clid");

    let private_message = runtime.execute(
        &format!(
            "sendtextmessage targetmode=1 target={} msg=Hello\\sPrivate",
            clid
        ),
        &mut session,
    );
    assert!(private_message.contains("error id=0 msg=ok"));

    let channel_message = runtime.execute(
        "sendtextmessage targetmode=2 target=0 msg=Hello\\sChannel",
        &mut session,
    );
    assert!(channel_message.contains("error id=0 msg=ok"));

    let server_message = runtime.execute(
        "sendtextmessage targetmode=3 target=0 msg=Hello\\sServer",
        &mut session,
    );
    assert!(server_message.contains("error id=0 msg=ok"));
}

#[test]
fn runtime_persists_channel_descriptions_and_conversation_history() {
    let state_path = temp_state_path("runtime-conversations");
    cleanup_state_path(&state_path);

    let channel_id = {
        let mut runtime = create_baseline_runtime_with_state_path(workspace_root(), &state_path)
            .expect("runtime should build with custom state path");
        let mut session = QuerySessionState::default();

        assert!(
            runtime
                .execute("login serveradmin serveradmin", &mut session)
                .contains("error id=0 msg=ok")
        );

        let created = runtime.execute(
            "channelcreate channel_name=Persisted\\sChat cpid=0 channel_topic=Saved\\sTopic channel_description=Saved\\sDescription",
            &mut session,
        );
        assert!(created.contains("error id=0 msg=ok"));
        let channel_id = extract_field(&created, "cid")
            .and_then(|value| value.parse::<u32>().ok())
            .expect("channelcreate should return cid");

        let message_response = runtime.execute(
            &format!(
                "sendtextmessage targetmode=2 cid={} target=0 msg=Saved\\sHistory",
                channel_id
            ),
            &mut session,
        );
        assert!(message_response.contains("error id=0 msg=ok"));

        channel_id
    };

    {
        let mut runtime = create_baseline_runtime_with_state_path(workspace_root(), &state_path)
            .expect("runtime should reload with persisted state");
        let mut session = QuerySessionState::default();

        assert!(
            runtime
                .execute("login serveradmin serveradmin", &mut session)
                .contains("error id=0 msg=ok")
        );

        let channelinfo = runtime.execute(&format!("channelinfo cid={channel_id}"), &mut session);
        assert!(channelinfo.contains("channel_description=Saved\\sDescription"));

        let index_rows = runtime.web_conversation_index_rows(1, &[channel_id]);
        assert_eq!(index_rows.len(), 1);
        assert_ne!(index_rows[0]["timestamp"], "0");

        let history_rows = runtime
            .web_conversation_history_rows(1, channel_id, None, None, Some(10))
            .expect("history rows should exist for persisted channel");
        assert_eq!(history_rows.len(), 1);
        assert_eq!(history_rows[0]["msg"], "Saved History");
        assert_eq!(history_rows[0]["sender_name"], "serveradmin");
    }

    cleanup_state_path(&state_path);
}

#[test]
fn runtime_persists_virtual_server_metadata() {
    let state_path = temp_state_path("runtime-server-metadata");
    cleanup_state_path(&state_path);

    {
        let mut runtime = create_baseline_runtime_with_state_path(workspace_root(), &state_path)
            .expect("runtime should build with custom state path");
        let mut session = QuerySessionState::default();

        assert!(
            runtime
                .execute("login serveradmin serveradmin", &mut session)
                .contains("error id=0 msg=ok")
        );
        assert!(
            runtime
                .execute("use sid=1", &mut session)
                .contains("error id=0 msg=ok")
        );

        let edited = runtime.execute(
            "serveredit virtualserver_name=Persisted\\sCompat virtualserver_welcomemessage=Saved\\sGreeting virtualserver_hostmessage=Saved\\sHost virtualserver_hostmessage_mode=3 virtualserver_ask_for_privilegekey=1 virtualserver_maxclients=96",
            &mut session,
        );
        assert!(edited.contains("error id=0 msg=ok"));
    }

    {
        let mut runtime = create_baseline_runtime_with_state_path(workspace_root(), &state_path)
            .expect("runtime should reload with persisted server state");
        let mut session = QuerySessionState::default();

        assert!(
            runtime
                .execute("login serveradmin serveradmin", &mut session)
                .contains("error id=0 msg=ok")
        );
        assert!(
            runtime
                .execute("use sid=1", &mut session)
                .contains("error id=0 msg=ok")
        );

        let serverinfo = runtime.execute("serverinfo", &mut session);
        assert!(serverinfo.contains("virtualserver_name=Persisted\\sCompat"));
        assert!(serverinfo.contains("virtualserver_welcomemessage=Saved\\sGreeting"));
        assert!(serverinfo.contains("virtualserver_hostmessage=Saved\\sHost"));
        assert!(serverinfo.contains("virtualserver_hostmessage_mode=3"));
        assert!(serverinfo.contains("virtualserver_ask_for_privilegekey=1"));
        assert!(serverinfo.contains("virtualserver_maxclients=96"));

        let serverlist = runtime.execute("serverlist -uid", &mut session);
        assert!(serverlist.contains("virtualserver_name=Persisted\\sCompat"));
    }

    cleanup_state_path(&state_path);
}

#[test]
fn runtime_persists_private_conversation_history() {
    let state_path = temp_state_path("runtime-private-conversations");
    cleanup_state_path(&state_path);

    {
        let mut runtime = create_baseline_runtime_with_state_path(workspace_root(), &state_path)
            .expect("runtime should build with custom state path");
        let mut admin_session = QuerySessionState::default();
        let mut bot_session = QuerySessionState::default();

        assert!(
            runtime
                .execute("login serveradmin serveradmin", &mut admin_session)
                .contains("error id=0 msg=ok")
        );
        assert!(
            runtime
                .execute("use sid=1", &mut admin_session)
                .contains("error id=0 msg=ok")
        );
        assert!(
            runtime
                .execute(
                    "querycreate client_login_name=private_bot client_login_password=private_secret server_id=1",
                    &mut admin_session,
                )
                .contains("client_login_name=private_bot")
        );
        assert!(
            runtime
                .execute("login private_bot private_secret", &mut bot_session)
                .contains("error id=0 msg=ok")
        );
        assert!(
            runtime
                .execute("use sid=1", &mut bot_session)
                .contains("error id=0 msg=ok")
        );

        let bot_whoami = runtime.execute("whoami", &mut bot_session);
        let bot_clid = extract_field(&bot_whoami, "clid")
            .and_then(|value| value.parse::<u64>().ok())
            .expect("whoami should expose the private bot clid");

        let private_message = runtime.execute(
            &format!(
                "sendtextmessage targetmode=1 target={} msg=Persisted\\sPrivate",
                bot_clid
            ),
            &mut admin_session,
        );
        assert!(private_message.contains("error id=0 msg=ok"));
    }

    {
        let runtime = create_baseline_runtime_with_state_path(workspace_root(), &state_path)
            .expect("runtime should reload with persisted private state");

        let history_rows = runtime
            .web_private_conversation_history_rows(
                1,
                1,
                Some("private_bot"),
                None,
                None,
                None,
                Some(10),
            )
            .expect("private history rows should exist for persisted messages");
        assert_eq!(history_rows.len(), 1);
        assert_eq!(history_rows[0]["msg"], "Persisted Private");
        assert_eq!(history_rows[0]["sender_name"], "serveradmin");
        assert_eq!(history_rows[0]["sender_unique_id"], "serveradmin");
        assert_eq!(history_rows[0]["cluid"], "private_bot");
    }

    cleanup_state_path(&state_path);
}

#[test]
fn runtime_persists_query_accounts_channels_and_session_snapshots() {
    let state_path = temp_state_path("runtime-persistence");
    cleanup_state_path(&state_path);

    {
        let mut runtime = create_baseline_runtime_with_state_path(workspace_root(), &state_path)
            .expect("runtime should build with custom state path");
        let mut session = QuerySessionState::default();

        assert!(
            runtime
                .execute("login serveradmin serveradmin", &mut session)
                .contains("error id=0 msg=ok")
        );
        assert!(
            runtime
                .execute("servernotifyregister event=channel id=2", &mut session)
                .contains("error id=0 msg=ok")
        );
        assert!(
            runtime
                .execute("clientmove cid=2", &mut session)
                .contains("error id=0 msg=ok")
        );

        let created = runtime.execute(
            "channelcreate channel_name=Persisted\\sRoom cpid=0 channel_topic=Saved\\sTopic",
            &mut session,
        );
        assert!(created.contains("error id=0 msg=ok"));
        assert!(runtime.execute(
            "querycreate client_login_name=persisted_bot client_login_password=stateful_secret",
            &mut session,
        )
        .contains("client_login_name=persisted_bot"));
    }

    {
        let mut runtime = create_baseline_runtime_with_state_path(workspace_root(), &state_path)
            .expect("runtime should reload with persisted state");
        let mut session = QuerySessionState::default();

        assert!(
            runtime
                .execute("login serveradmin serveradmin", &mut session)
                .contains("error id=0 msg=ok")
        );

        let whoami = runtime.execute("whoami", &mut session);
        assert!(whoami.contains("client_channel_id=2"));
        assert!(whoami.contains("notify_subscription_count=1"));

        let channellist = runtime.execute("channellist", &mut session);
        assert!(channellist.contains("channel_name=Persisted\\sRoom"));
        assert!(channellist.contains("channel_topic=Saved\\sTopic"));

        let mut bot_session = QuerySessionState::default();
        assert!(
            runtime
                .execute("login persisted_bot stateful_secret", &mut bot_session)
                .contains("error id=0 msg=ok")
        );
    }

    cleanup_state_path(&state_path);
}

#[test]
fn servergroup_membership_requires_permissions() {
    let mut runtime = create_test_runtime("servergroup-membership-permissions");
    let mut session = QuerySessionState::default();

    assert!(
        runtime
            .execute("login serveradmin serveradmin", &mut session)
            .contains("error id=0 msg=ok")
    );
    assert!(
        runtime
            .execute("use sid=1", &mut session)
            .contains("error id=0 msg=ok")
    );
    assert!(
        runtime
            .execute(
                "clientsetserverquerylogin client_login_name=operator",
                &mut session,
            )
            .contains("client_login_password=generated-operator")
    );
    assert!(
        runtime
            .execute("servergroupdelclient sgid=6 cldbid=1", &mut session)
            .contains("error id=0 msg=ok")
    );

    let denied = runtime.execute("servergroupaddclient sgid=6 cldbid=1", &mut session);
    assert!(denied.contains("error id=2568"));
    assert!(denied.contains("failed_permid="));
}

#[test]
fn permission_edits_require_modify_power() {
    let mut runtime = create_test_runtime("permission-edit-permissions");
    let mut session = QuerySessionState::default();

    assert!(
        runtime
            .execute("login serveradmin serveradmin", &mut session)
            .contains("error id=0 msg=ok")
    );
    assert!(
        runtime
            .execute("use sid=1", &mut session)
            .contains("error id=0 msg=ok")
    );
    assert!(
        runtime
            .execute(
                "clientsetserverquerylogin client_login_name=operator_perm",
                &mut session,
            )
            .contains("client_login_password=generated-operator_perm")
    );
    assert!(
        runtime
            .execute("servergroupdelclient sgid=6 cldbid=1", &mut session)
            .contains("error id=0 msg=ok")
    );

    let denied = runtime.execute(
        "servergroupaddperm sgid=8 permsid=custom_runtime_probe permvalue=7 permnegated=0 permskip=0",
        &mut session,
    );
    assert!(denied.contains("error id=2568"));
    assert!(denied.contains("failed_permid="));

    let permissions = runtime.execute("servergrouppermlist sgid=8 -permsid", &mut session);
    assert!(!permissions.contains("permsid=custom_runtime_probe"));
}

#[test]
fn server_and_channel_edits_require_permissions() {
    let mut runtime = create_test_runtime("access-edit-permissions");
    let mut session = QuerySessionState::default();

    assert!(
        runtime
            .execute("login serveradmin serveradmin", &mut session)
            .contains("error id=0 msg=ok")
    );
    assert!(
        runtime
            .execute("use sid=1", &mut session)
            .contains("error id=0 msg=ok")
    );
    assert!(
        runtime
            .execute(
                "clientsetserverquerylogin client_login_name=operator_access",
                &mut session,
            )
            .contains("client_login_password=generated-operator_access")
    );
    let antiflood_permission_id = runtime
        .execute(
            "permidgetbyname permsid=b_virtualserver_modify_antiflood",
            &mut session,
        )
        .split_whitespace()
        .find_map(|part| part.strip_prefix("permid=")?.parse::<u32>().ok())
        .expect("permidgetbyname should expose antiflood permission id");
    assert!(
        runtime
            .execute("servergroupdelclient sgid=6 cldbid=1", &mut session)
            .contains("error id=0 msg=ok")
    );

    let denied_serveredit = runtime.execute("serveredit virtualserver_name=Denied", &mut session);
    assert!(denied_serveredit.contains("error id=2568"));
    assert!(denied_serveredit.contains("failed_permid="));

    let denied_antiflood_serveredit = runtime.execute(
        "serveredit virtualserver_antiflood_points_needed_command_block=2",
        &mut session,
    );
    assert!(denied_antiflood_serveredit.contains("error id=2568"));
    assert!(
        denied_antiflood_serveredit.contains(&format!("failed_permid={antiflood_permission_id}"))
    );

    let denied_channelcreate = runtime.execute(
        "channelcreate channel_name=Denied\\sRoom channel_topic=Nope",
        &mut session,
    );
    assert!(denied_channelcreate.contains("error id=2568"));
    assert!(denied_channelcreate.contains("failed_permid="));

    let denied_channeledit = runtime.execute("channeledit cid=1 channel_name=Denied", &mut session);
    assert!(denied_channeledit.contains("error id=2568"));
    assert!(denied_channeledit.contains("failed_permid="));
}

#[test]
fn query_antiflood_blocks_repeated_commands_unless_ignored() {
    let mut runtime = create_test_runtime("query-antiflood");
    let mut admin_session = QuerySessionState::default();

    assert!(
        runtime
            .execute("login serveradmin serveradmin", &mut admin_session)
            .contains("error id=0 msg=ok")
    );
    assert!(
        runtime
            .execute("use sid=1", &mut admin_session)
            .contains("error id=0 msg=ok")
    );
    assert!(
        runtime
            .execute(
                "serveredit virtualserver_antiflood_points_tick_reduce=0 virtualserver_antiflood_points_needed_command_block=3 virtualserver_antiflood_points_needed_ip_block=6 virtualserver_antiflood_ban_time=60",
                &mut admin_session,
            )
            .contains("error id=0 msg=ok")
    );

    let created_query_account = runtime.execute(
        "querycreate client_login_name=flood_bot client_login_password=flood_secret server_id=1",
        &mut admin_session,
    );
    let flood_bot_cldbid = extract_field(&created_query_account, "cldbid")
        .expect("querycreate should expose cldbid for flood bot")
        .to_string();

    let mut bot_session = QuerySessionState::default();
    assert!(
        runtime
            .execute("login flood_bot flood_secret", &mut bot_session)
            .contains("error id=0 msg=ok")
    );
    assert!(
        runtime
            .execute("use sid=1", &mut bot_session)
            .contains("error id=0 msg=ok")
    );
    assert!(
        runtime
            .execute("serverinfo", &mut bot_session)
            .contains("error id=0 msg=ok")
    );

    let blocked_serverinfo = runtime.execute("serverinfo", &mut bot_session);
    assert!(blocked_serverinfo.contains("error id=524"));

    assert!(
        runtime
            .execute(
                &format!(
                    "clientaddperm cldbid={} permsid=b_client_ignore_antiflood permvalue=1",
                    flood_bot_cldbid
                ),
                &mut admin_session,
            )
            .contains("error id=0 msg=ok")
    );

    let mut ignored_bot_session = QuerySessionState::default();
    assert!(
        runtime
            .execute("login flood_bot flood_secret", &mut ignored_bot_session)
            .contains("error id=0 msg=ok")
    );
    assert!(
        runtime
            .execute("use sid=1", &mut ignored_bot_session)
            .contains("error id=0 msg=ok")
    );
    assert!(
        runtime
            .execute("serverinfo", &mut ignored_bot_session)
            .contains("error id=0 msg=ok")
    );
    assert!(
        runtime
            .execute("serverinfo", &mut ignored_bot_session)
            .contains("error id=0 msg=ok")
    );
}

#[test]
fn query_antiflood_ip_block_is_shared_across_sessions() {
    let mut runtime = create_test_runtime("query-antiflood-shared-ip");
    let mut admin_session = QuerySessionState::default();

    assert!(
        runtime
            .execute("login serveradmin serveradmin", &mut admin_session)
            .contains("error id=0 msg=ok")
    );
    assert!(
        runtime
            .execute("use sid=1", &mut admin_session)
            .contains("error id=0 msg=ok")
    );
    assert!(
        runtime
            .execute(
                "serveredit virtualserver_antiflood_points_tick_reduce=0 virtualserver_antiflood_points_needed_command_block=100 virtualserver_antiflood_points_needed_ip_block=4 virtualserver_antiflood_ban_time=60",
                &mut admin_session,
            )
            .contains("error id=0 msg=ok")
    );

    for (login_name, password) in [
        ("shared_ip_a", "shared_ip_secret_a"),
        ("shared_ip_b", "shared_ip_secret_b"),
        ("isolated_ip", "isolated_ip_secret"),
    ] {
        assert!(
            runtime
                .execute(
                    &format!(
                        "querycreate client_login_name={login_name} client_login_password={password} server_id=1"
                    ),
                    &mut admin_session,
                )
                .contains("error id=0 msg=ok")
        );
    }

    let mut shared_a_session = QuerySessionState {
        connection_ip: String::from("198.51.100.10"),
        ..QuerySessionState::default()
    };
    let mut shared_b_session = QuerySessionState {
        connection_ip: String::from("198.51.100.10"),
        ..QuerySessionState::default()
    };
    let mut isolated_session = QuerySessionState {
        connection_ip: String::from("198.51.100.11"),
        ..QuerySessionState::default()
    };

    for (login_name, password, session) in [
        ("shared_ip_a", "shared_ip_secret_a", &mut shared_a_session),
        ("shared_ip_b", "shared_ip_secret_b", &mut shared_b_session),
        ("isolated_ip", "isolated_ip_secret", &mut isolated_session),
    ] {
        assert!(
            runtime
                .execute(&format!("login {login_name} {password}"), session)
                .contains("error id=0 msg=ok")
        );
        assert!(
            runtime
                .execute("use sid=1", session)
                .contains("error id=0 msg=ok")
        );
    }

    let mut shared_a_blocked = false;
    let mut shared_b_blocked = false;
    for _ in 0..10 {
        let shared_a_response = runtime.execute("serverinfo", &mut shared_a_session);
        if shared_a_response.contains("error id=524") {
            shared_a_blocked = true;
            break;
        }
        assert!(shared_a_response.contains("error id=0 msg=ok"));

        let shared_b_response = runtime.execute("serverinfo", &mut shared_b_session);
        if shared_b_response.contains("error id=524") {
            shared_b_blocked = true;
            break;
        }
        assert!(shared_b_response.contains("error id=0 msg=ok"));
    }

    assert!(
        shared_a_blocked || shared_b_blocked,
        "shared IP sessions should eventually trigger a shared IP flood block"
    );

    assert!(
        runtime
            .execute("serverinfo", &mut isolated_session)
            .contains("error id=0 msg=ok")
    );

    let partner_blocked_response = if shared_a_blocked {
        runtime.execute("serverinfo", &mut shared_b_session)
    } else {
        runtime.execute("serverinfo", &mut shared_a_session)
    };
    assert!(partner_blocked_response.contains("error id=524"));
}

#[test]
fn group_structure_and_assignment_mutations_require_permissions() {
    let mut runtime = create_test_runtime("group-structure-permissions");
    let mut session = QuerySessionState::default();

    assert!(
        runtime
            .execute("login serveradmin serveradmin", &mut session)
            .contains("error id=0 msg=ok")
    );
    assert!(
        runtime
            .execute("use sid=1", &mut session)
            .contains("error id=0 msg=ok")
    );

    let mut permission_id_for = |permission_name: &str| {
        runtime
            .execute(
                &format!("permidgetbyname permsid={permission_name}"),
                &mut session,
            )
            .split_whitespace()
            .find_map(|part| part.strip_prefix("permid=")?.parse::<u32>().ok())
            .unwrap_or_else(|| panic!("permidgetbyname should expose id for {permission_name}"))
    };
    let servergroup_create_permission_id = permission_id_for("b_virtualserver_servergroup_create");
    let servergroup_delete_permission_id = permission_id_for("b_virtualserver_servergroup_delete");
    let channelgroup_create_permission_id =
        permission_id_for("b_virtualserver_channelgroup_create");
    let channelgroup_delete_permission_id =
        permission_id_for("b_virtualserver_channelgroup_delete");
    let group_modify_permission_id = permission_id_for("i_group_modify_power");
    let group_member_add_permission_id = permission_id_for("i_group_member_add_power");
    let permission_modify_ignore_id = permission_id_for("b_permission_modify_power_ignore");

    assert!(
        runtime
            .execute(
                "clientsetserverquerylogin client_login_name=operator_groups",
                &mut session,
            )
            .contains("client_login_password=generated-operator_groups")
    );

    let server_group_created = runtime.execute(
        r"servergroupadd name=Protected\sServer\sGroup type=1",
        &mut session,
    );
    let protected_server_group_id = server_group_created
        .split_whitespace()
        .find_map(|part| part.strip_prefix("sgid=")?.parse::<u32>().ok())
        .expect("servergroupadd should expose sgid");
    assert!(
        runtime
            .execute(
                &format!(
                    "servergroupaddperm sgid={} permsid=i_server_group_needed_modify_power permvalue=75 permnegated=0 permskip=0",
                    protected_server_group_id,
                ),
                &mut session,
            )
            .contains("error id=0 msg=ok")
    );

    let channel_group_created = runtime.execute(
        r"channelgroupadd name=Protected\sChannel\sGroup type=1",
        &mut session,
    );
    let protected_channel_group_id = channel_group_created
        .split_whitespace()
        .find_map(|part| part.strip_prefix("cgid=")?.parse::<u32>().ok())
        .expect("channelgroupadd should expose cgid");
    assert!(
        runtime
            .execute(
                &format!(
                    "channelgroupaddperm cgid={} permsid=i_channel_group_needed_modify_power permvalue=75 permnegated=0 permskip=0|permsid=i_channel_group_needed_member_add_power permvalue=60 permnegated=0 permskip=0",
                    protected_channel_group_id,
                ),
                &mut session,
            )
            .contains("error id=0 msg=ok")
    );

    assert!(
        runtime
            .execute("servergroupdelclient sgid=6 cldbid=1", &mut session)
            .contains("error id=0 msg=ok")
    );

    let denied_servergroupadd = runtime.execute(
        r"servergroupadd name=Denied\sServer\sGroup type=1",
        &mut session,
    );
    assert!(denied_servergroupadd.contains("error id=2568"));
    assert!(
        denied_servergroupadd
            .contains(&format!("failed_permid={servergroup_create_permission_id}"))
    );

    let denied_servergroupcopy = runtime.execute(
        &format!(
            r"servergroupcopy ssgid={} tsgid=0 name=Denied\sServer\sCopy type=1",
            protected_server_group_id,
        ),
        &mut session,
    );
    assert!(denied_servergroupcopy.contains("error id=2568"));
    assert!(
        denied_servergroupcopy
            .contains(&format!("failed_permid={servergroup_create_permission_id}"))
    );

    let denied_servergrouprename = runtime.execute(
        &format!(
            r"servergrouprename sgid={} name=Denied\sRename",
            protected_server_group_id,
        ),
        &mut session,
    );
    assert!(denied_servergrouprename.contains("error id=2568"));
    assert!(
        denied_servergrouprename.contains(&format!("failed_permid={group_modify_permission_id}"))
    );

    let denied_servergroupdel = runtime.execute(
        &format!("servergroupdel sgid={} force=1", protected_server_group_id),
        &mut session,
    );
    assert!(denied_servergroupdel.contains("error id=2568"));
    assert!(
        denied_servergroupdel
            .contains(&format!("failed_permid={servergroup_delete_permission_id}"))
    );

    let denied_autoaddperm = runtime.execute(
        "servergroupautoaddperm sgtype=45 permsid=custom_auto_probe permvalue=1 permnegated=0 permskip=0",
        &mut session,
    );
    assert!(denied_autoaddperm.contains("error id=2568"));
    assert!(denied_autoaddperm.contains(&format!("failed_permid={permission_modify_ignore_id}")));

    let denied_channelgroupadd = runtime.execute(
        r"channelgroupadd name=Denied\sChannel\sGroup type=1",
        &mut session,
    );
    assert!(denied_channelgroupadd.contains("error id=2568"));
    assert!(denied_channelgroupadd.contains(&format!(
        "failed_permid={channelgroup_create_permission_id}"
    )));

    let denied_channelgroupcopy = runtime.execute(
        &format!(
            r"channelgroupcopy scgid={} tcgid=0 name=Denied\sChannel\sCopy type=1",
            protected_channel_group_id,
        ),
        &mut session,
    );
    assert!(denied_channelgroupcopy.contains("error id=2568"));
    assert!(denied_channelgroupcopy.contains(&format!(
        "failed_permid={channelgroup_create_permission_id}"
    )));

    let denied_channelgrouprename = runtime.execute(
        &format!(
            r"channelgrouprename cgid={} name=Denied\sRename",
            protected_channel_group_id,
        ),
        &mut session,
    );
    assert!(denied_channelgrouprename.contains("error id=2568"));
    assert!(
        denied_channelgrouprename.contains(&format!("failed_permid={group_modify_permission_id}"))
    );

    let denied_channelgroupdel = runtime.execute(
        &format!(
            "channelgroupdel cgid={} force=1",
            protected_channel_group_id
        ),
        &mut session,
    );
    assert!(denied_channelgroupdel.contains("error id=2568"));
    assert!(denied_channelgroupdel.contains(&format!(
        "failed_permid={channelgroup_delete_permission_id}"
    )));

    let denied_setclientchannelgroup = runtime.execute(
        &format!(
            "setclientchannelgroup cgid={} cid=2 cldbid=1",
            protected_channel_group_id,
        ),
        &mut session,
    );
    assert!(denied_setclientchannelgroup.contains("error id=2568"));
    assert!(
        denied_setclientchannelgroup
            .contains(&format!("failed_permid={group_member_add_permission_id}"))
    );
}

#[test]
fn query_and_token_mutations_require_permissions() {
    let mut runtime = create_test_runtime("query-token-permissions");
    let mut session = QuerySessionState::default();

    assert!(
        runtime
            .execute("login serveradmin serveradmin", &mut session)
            .contains("error id=0 msg=ok")
    );
    assert!(
        runtime
            .execute("use sid=1", &mut session)
            .contains("error id=0 msg=ok")
    );

    let mut permission_id_for = |permission_name: &str| {
        extract_field(
            &runtime.execute(
                &format!("permidgetbyname permsid={permission_name}"),
                &mut session,
            ),
            "permid",
        )
        .unwrap_or_else(|| panic!("permidgetbyname should expose id for {permission_name}"))
        .parse::<u32>()
        .unwrap_or_else(|_| panic!("permission id should parse for {permission_name}"))
    };
    let query_create_permission_id = permission_id_for("b_client_query_create");
    let query_create_own_permission_id = permission_id_for("b_client_query_create_own");
    let query_rename_permission_id = permission_id_for("b_client_query_rename");
    let query_change_own_password_permission_id =
        permission_id_for("b_client_query_change_own_password");
    let query_delete_permission_id = permission_id_for("b_client_query_delete");
    let query_login_modify_permission_id =
        permission_id_for("b_client_create_modify_serverquery_login");
    let token_add_permission_id = permission_id_for("b_virtualserver_token_add");
    let token_delete_permission_id = permission_id_for("b_virtualserver_token_delete_all");
    let token_edit_permission_id = permission_id_for("b_virtualserver_token_edit_all");
    let token_list_permission_id = permission_id_for("b_virtualserver_token_list_all");

    let created_query = runtime.execute(
        "querycreate client_login_name=query_target client_login_password=query_target_secret server_id=1",
        &mut session,
    );
    assert!(created_query.contains("client_login_name=query_target"));

    let created_token = runtime.execute(
        r"tokenadd token_description=Admin\sToken token_max_uses=1 action_type=2 action_id1=8",
        &mut session,
    );
    let token_id = extract_field(&created_token, "token_id")
        .expect("tokenadd should expose token_id")
        .parse::<u32>()
        .expect("token_id should parse");
    assert!(created_token.contains("token="));

    assert!(
        runtime
            .execute(
                "clientsetserverquerylogin client_login_name=operator_query_tokens",
                &mut session,
            )
            .contains("client_login_password=generated-operator_query_tokens")
    );
    assert!(
        runtime
            .execute("servergroupdelclient sgid=6 cldbid=1", &mut session)
            .contains("error id=0 msg=ok")
    );

    let denied_querycreate_own = runtime.execute(
        "querycreate client_login_name=own_denied client_login_password=own_secret cldbid=1 server_id=1",
        &mut session,
    );
    assert!(denied_querycreate_own.contains("error id=2568"));
    assert!(
        denied_querycreate_own.contains(&format!("failed_permid={query_create_own_permission_id}"))
    );

    let denied_querycreate = runtime.execute(
        "querycreate client_login_name=global_denied client_login_password=global_secret server_id=1",
        &mut session,
    );
    assert!(denied_querycreate.contains("error id=2568"));
    assert!(denied_querycreate.contains(&format!("failed_permid={query_create_permission_id}")));

    let denied_queryrename = runtime.execute(
        "queryrename client_login_name=query_target client_new_login_name=query_target_renamed",
        &mut session,
    );
    assert!(denied_queryrename.contains("error id=2568"));
    assert!(denied_queryrename.contains(&format!("failed_permid={query_rename_permission_id}")));

    let denied_querychangepassword = runtime.execute(
        "querychangepassword client_login_name=operator_query_tokens",
        &mut session,
    );
    assert!(denied_querychangepassword.contains("error id=2568"));
    assert!(denied_querychangepassword.contains(&format!(
        "failed_permid={query_change_own_password_permission_id}"
    )));

    let denied_querydelete =
        runtime.execute("querydelete client_login_name=query_target", &mut session);
    assert!(denied_querydelete.contains("error id=2568"));
    assert!(denied_querydelete.contains(&format!("failed_permid={query_delete_permission_id}")));

    let denied_queryloginrename = runtime.execute(
        "clientsetserverquerylogin client_login_name=operator_query_tokens_denied",
        &mut session,
    );
    assert!(denied_queryloginrename.contains("error id=2568"));
    assert!(
        denied_queryloginrename
            .contains(&format!("failed_permid={query_login_modify_permission_id}"))
    );

    let denied_tokenadd = runtime.execute(
        r"tokenadd token_description=Denied\sToken token_max_uses=1 action_type=2 action_id1=8",
        &mut session,
    );
    assert!(denied_tokenadd.contains("error id=2568"));
    assert!(denied_tokenadd.contains(&format!("failed_permid={token_add_permission_id}")));

    let listed_tokens = runtime.execute("tokenlist -new", &mut session);
    assert!(listed_tokens.contains("error id=0 msg=ok"));
    assert!(!listed_tokens.contains(&format!("token_id={token_id}")));

    let denied_tokenactionlist = runtime.execute(
        &format!("tokenactionlist token_id={token_id}"),
        &mut session,
    );
    assert!(denied_tokenactionlist.contains("error id=2568"));
    assert!(denied_tokenactionlist.contains(&format!("failed_permid={token_list_permission_id}")));

    let denied_tokendelete =
        runtime.execute(&format!("tokendelete token_id={token_id}"), &mut session);
    assert!(denied_tokendelete.contains("error id=2568"));
    assert!(denied_tokendelete.contains(&format!("failed_permid={token_delete_permission_id}")));

    let denied_tokenedit = runtime.execute(
        &format!(
            "tokenedit token_id={} token_description=Denied\\sEdit",
            token_id
        ),
        &mut session,
    );
    assert!(denied_tokenedit.contains("error id=2568"));
    assert!(denied_tokenedit.contains(&format!("failed_permid={token_edit_permission_id}")));
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
