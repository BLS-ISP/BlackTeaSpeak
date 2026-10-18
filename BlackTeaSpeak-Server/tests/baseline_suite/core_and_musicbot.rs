use super::*;
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

