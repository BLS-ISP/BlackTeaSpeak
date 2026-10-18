use super::*;
#[test]
fn paket_a_moderated_channels_talk_power_and_clientedit() {
    let mut runtime = create_test_runtime("paket-a-talk-power");
    let mut admin_session = QuerySessionState::default();
    assert_eq!(
        runtime.execute("login serveradmin serveradmin", &mut admin_session),
        "error id=0 msg=ok"
    );

    // Create a moderated channel with channel_needed_talk_power=60
    let create_resp = runtime.execute(
        "channelcreate channel_name=Moderated\\sStage channel_needed_talk_power=60",
        &mut admin_session,
    );
    assert!(create_resp.contains("error id=0 msg=ok"), "{create_resp}");
    let stage_cid: u32 = extract_field(&create_resp, "cid")
        .expect("channelcreate should return cid")
        .parse()
        .unwrap();

    let chinfo = runtime.execute(&format!("channelinfo cid={stage_cid}"), &mut admin_session);
    assert!(
        chinfo.contains("channel_needed_talk_power=60"),
        "channelinfo should expose channel_needed_talk_power=60: {chinfo}"
    );
    let chlist = runtime.execute("channellist -voice", &mut admin_session);
    assert!(
        chlist.contains("channel_needed_talk_power=60"),
        "channellist -voice should expose channel_needed_talk_power=60: {chlist}"
    );

    // Move online client clid=11 (DJ Mix, cldbid=41, Normal group with talk_power=25) into the moderated channel
    assert_eq!(
        runtime.execute(
            &format!("clientmove clid=11 cid={stage_cid}"),
            &mut admin_session
        ),
        "error id=0 msg=ok"
    );

    // clid=11 has talk power 25 < 60, so cannot speak yet
    assert!(
        !runtime.client_can_speak_in_channel(1, 11),
        "client with talk_power 25 should not be able to speak in channel requiring 60"
    );

    // Client requests talk power via clientupdate
    let mut dj_session = QuerySessionState {
        client_id: 11,
        selected_virtual_server_id: Some(1),
        current_channel_id: Some(stage_cid),
        actor_client_database_id_override: Some(41),
        is_desktop_client: true,
        ..QuerySessionState::default()
    };
    assert_eq!(
        runtime.execute(
            "clientupdate client_talk_request=1 client_talk_request_msg=Can\\sI\\sspeak\\splease",
            &mut dj_session
        ),
        "error id=0 msg=ok"
    );

    let clinfo_before = runtime.execute("clientinfo clid=11", &mut admin_session);
    assert!(
        clinfo_before.contains("client_talk_request=1")
            && clinfo_before.contains("client_talk_request_msg=Can\\sI\\sspeak\\splease"),
        "clientinfo should show talk request: {clinfo_before}"
    );

    // Admin grants client_is_talker=1 and sets client_description via clientedit
    assert_eq!(
        runtime.execute(
            "clientedit clid=11 client_is_talker=1 client_description=Stage\\sGuest client_is_channel_commander=1",
            &mut admin_session
        ),
        "error id=0 msg=ok"
    );

    // Now clid=11 can speak in the moderated channel, and talk_request was cleared
    assert!(
        runtime.client_can_speak_in_channel(1, 11),
        "client with client_is_talker=1 must be allowed to speak in moderated channel"
    );
    let clinfo_after = runtime.execute("clientinfo clid=11", &mut admin_session);
    assert!(
        clinfo_after.contains("client_is_talker=1")
            && clinfo_after.contains("client_talk_request=0")
            && clinfo_after.contains("client_description=Stage\\sGuest")
            && clinfo_after.contains("client_is_channel_commander=1"),
        "clientinfo should reflect clientedit changes: {clinfo_after}"
    );
}

#[test]
fn paket_a_channel_passwords_and_client_limits() {
    let mut runtime = create_test_runtime("paket-a-channel-limits");
    let mut admin_session = QuerySessionState::default();
    assert_eq!(
        runtime.execute("login serveradmin serveradmin", &mut admin_session),
        "error id=0 msg=ok"
    );

    // Create a password-protected channel with maxclients=1 and maxfamilyclients=1
    let create_resp = runtime.execute(
        "channelcreate channel_name=VIP\\sVault channel_password=vaultpw channel_maxclients=1 channel_maxfamilyclients=1",
        &mut admin_session,
    );
    assert!(create_resp.contains("error id=0 msg=ok"), "{create_resp}");
    let vip_cid: u32 = extract_field(&create_resp, "cid")
        .expect("channelcreate should return cid")
        .parse()
        .unwrap();

    // Create a subchannel under VIP Vault that inherits family maxclients
    let sub_resp = runtime.execute(
        &format!("channelcreate channel_name=VIP\\sSub cpid={vip_cid} channel_maxclients=5 channel_maxfamilyclients=-1"),
        &mut admin_session,
    );
    assert!(sub_resp.contains("error id=0 msg=ok"), "{sub_resp}");
    let sub_cid: u32 = extract_field(&sub_resp, "cid")
        .expect("subchannel should return cid")
        .parse()
        .unwrap();

    // Remove ignore permissions from Normal group (sgid=7) if present so normal clients must respect password/limits
    let _ = runtime.execute(
        "servergroupdelperm sgid=7 permsid=b_channel_join_ignore_password|permsid=b_channel_join_ignore_maxclients",
        &mut admin_session,
    );

    // Session for clid=11 (cldbid=41, Normal group sgid=7)
    let mut client11_session = QuerySessionState {
        client_id: 11,
        selected_virtual_server_id: Some(1),
        current_channel_id: Some(1),
        actor_client_database_id_override: Some(41),
        is_desktop_client: true,
        ..QuerySessionState::default()
    };

    // 1. Joining without password fails with 781
    let no_pw = runtime.execute(&format!("clientmove cid={vip_cid}"), &mut client11_session);
    assert!(
        no_pw.contains("error id=781 msg=invalid\\schannel\\spassword"),
        "joining password channel without cpw should return 781: {no_pw}"
    );

    // 2. Joining with correct password succeeds
    let ok_pw = runtime.execute(
        &format!("clientmove cid={vip_cid} cpw=vaultpw"),
        &mut client11_session,
    );
    assert_eq!(ok_pw, "error id=0 msg=ok");

    // 3. Another non-admin client (clid=10, cldbid=40)
    let _ = runtime.execute("servergroupdelclient sgid=6 cldbid=40", &mut admin_session);
    let _ = runtime.execute("servergroupdelclient sgid=9 cldbid=40", &mut admin_session);
    let mut client10_session = QuerySessionState {
        client_id: 10,
        selected_virtual_server_id: Some(1),
        current_channel_id: Some(1),
        actor_client_database_id_override: Some(40),
        is_desktop_client: true,
        ..QuerySessionState::default()
    };

    // Trying to join vip_cid (maxclients=1, already has clid=11) fails with 777
    let full_err = runtime.execute(
        &format!("clientmove cid={vip_cid} cpw=vaultpw"),
        &mut client10_session,
    );
    assert!(
        full_err.contains("error id=777 msg=channel\\smaxclient\\sreached"),
        "joining full channel should return 777: {full_err}"
    );

    // Trying to join sub_cid (maxclients=5, but parent family maxfamilyclients=1 already reached) fails with 778
    let family_err = runtime.execute(
        &format!("clientmove cid={sub_cid}"),
        &mut client10_session,
    );
    assert!(
        family_err.contains("error id=778 msg=channel\\smaxfamily\\sreached"),
        "joining subchannel when family maxclients is reached should return 778: {family_err}"
    );

    // Admin can move client 10 into vip_cid without password and ignoring maxclients
    let admin_move = runtime.execute(
        &format!("clientmove clid=10 cid={vip_cid}"),
        &mut admin_session,
    );
    assert_eq!(admin_move, "error id=0 msg=ok");
}

#[test]
fn paket_a_server_password_and_temp_passwords() {
    let mut runtime = create_test_runtime("paket-a-server-passwords");
    let mut admin_session = QuerySessionState::default();
    assert_eq!(
        runtime.execute("login serveradmin serveradmin", &mut admin_session),
        "error id=0 msg=ok"
    );

    // Set server password
    assert_eq!(
        runtime.execute(
            "serveredit virtualserver_password=mainsecret",
            &mut admin_session
        ),
        "error id=0 msg=ok"
    );
    let sinfo = runtime.execute("serverinfo", &mut admin_session);
    assert!(
        sinfo.contains("virtualserver_flag_password=1"),
        "serverinfo should indicate virtualserver_flag_password=1: {sinfo}"
    );

    assert!(runtime.verify_server_password(1, "wrong", None).is_err());
    assert_eq!(runtime.verify_server_password(1, "mainsecret", None), Ok(None));

    // Add a temporary server password pointing to channel 2
    assert_eq!(
        runtime.execute(
            "servertemppasswordadd pw=event123 desc=Event\\sAccess duration=600 tcid=2 tcpw=chansecret",
            &mut admin_session
        ),
        "error id=0 msg=ok"
    );

    let tplist = runtime.execute("servertemppasswordlist", &mut admin_session);
    assert!(
        tplist.contains("pw_clear=event123")
            && tplist.contains("desc=Event\\sAccess")
            && tplist.contains("tcid=2"),
        "servertemppasswordlist should list temporary password: {tplist}"
    );

    // Verify temp password returns target channel 2 and channel password
    assert_eq!(
        runtime.verify_server_password(1, "event123", None),
        Ok(Some((2, "chansecret".to_string())))
    );

    // Delete temporary server password
    assert_eq!(
        runtime.execute("servertemppassworddel pw=event123", &mut admin_session),
        "error id=0 msg=ok"
    );
    assert!(runtime.verify_server_password(1, "event123", None).is_err());
}

#[test]
fn paket_a_channel_subscriptions() {
    let mut runtime = create_test_runtime("paket-a-channel-subs");
    let mut session = QuerySessionState {
        client_id: 10,
        selected_virtual_server_id: Some(1),
        current_channel_id: Some(1),
        actor_client_database_id_override: Some(40),
        is_desktop_client: true,
        ..QuerySessionState::default()
    };

    assert_eq!(
        runtime.execute("channelsubscribe cid=1|cid=2", &mut session),
        "error id=0 msg=ok"
    );
    assert!(session.subscribed_channels.contains(&1));
    assert!(session.subscribed_channels.contains(&2));

    assert_eq!(
        runtime.execute("channelunsubscribe cid=1", &mut session),
        "error id=0 msg=ok"
    );
    assert!(!session.subscribed_channels.contains(&1));
    assert!(session.subscribed_channels.contains(&2));

    assert_eq!(
        runtime.execute("channelsubscribeall", &mut session),
        "error id=0 msg=ok"
    );
    assert!(session.subscribed_channels.len() >= 2);

    assert_eq!(
        runtime.execute("channelunsubscribeall", &mut session),
        "error id=0 msg=ok"
    );
    assert!(session.subscribed_channels.is_empty());
}

#[test]
fn paket_b_offline_messages_and_persistence() {
    let state_path = temp_state_path("paket-b-offline-messages");
    {
        let mut runtime = create_baseline_runtime_with_state_path(workspace_root(), &state_path).expect("runtime should initialize");
        let mut admin_session = QuerySessionState::default();
        assert!(
            runtime
                .execute("login serveradmin serveradmin", &mut admin_session)
                .contains("error id=0 msg=ok")
        );

        let created = runtime.execute(
            "querycreate client_login_name=inbox_user client_login_password=inbox_pass",
            &mut admin_session,
        );
        assert!(created.contains("error id=0 msg=ok"));
        let receiver_cldbid = extract_field(&created, "cldbid")
            .expect("querycreate should return cldbid")
            .parse::<u64>()
            .expect("cldbid should parse");

        // Grant basic read permissions to inbox_user, but disallow sending offline messages initially
        assert_eq!(
            runtime.execute(
                &format!(
                    "clientaddperm cldbid={} permsid=b_client_offline_textmessage_send permvalue=0 permskip=0",
                    receiver_cldbid
                ),
                &mut admin_session
            ),
            "error id=0 msg=ok"
        );

        // Admin sends an offline message to inbox_user
        let added = runtime.execute(
            "messageadd cluid=query:inbox_user subject=Welcome\\sNote message=Hello\\sfrom\\sadmin!",
            &mut admin_session,
        );
        assert!(added.contains("msgid=1"));
        assert!(added.contains("error id=0 msg=ok"));

        // Log in as inbox_user and check messagelist
        let mut user_session = QuerySessionState::default();
        assert!(
            runtime
                .execute("login inbox_user inbox_pass", &mut user_session)
                .contains("error id=0 msg=ok")
        );

        // inbox_user cannot send offline messages due to b_client_offline_textmessage_send=0
        let denied_send = runtime.execute(
            "messageadd cluid=serveradmin subject=Reply message=Blocked",
            &mut user_session,
        );
        assert!(denied_send.contains("error id=2568 msg=insufficient\\sclient\\spermissions"));

        let list = runtime.execute("messagelist", &mut user_session);
        assert!(list.contains("msgid=1"));
        assert!(list.contains("cluid=serveradmin"));
        assert!(list.contains("subject=Welcome\\sNote"));
        assert!(list.contains("flag_read=0"));

        // Reading the message via messageget marks flag_read=1
        let msg = runtime.execute("messageget msgid=1", &mut user_session);
        assert!(msg.contains("msgid=1"));
        assert!(msg.contains("message=Hello\\sfrom\\sadmin!"));

        let list_after_get = runtime.execute("messagelist", &mut user_session);
        assert!(list_after_get.contains("flag_read=1"));

        // Mark unread again via messageupdateflag
        assert_eq!(
            runtime.execute("messageupdateflag msgid=1 flag=0", &mut user_session),
            "error id=0 msg=ok"
        );
    }

    // Reload from persisted state and verify the offline message is still present with flag_read=0
    {
        let mut runtime = create_baseline_runtime_with_state_path(workspace_root(), &state_path).expect("runtime should initialize");
        let mut user_session = QuerySessionState::default();
        assert!(
            runtime
                .execute("login inbox_user inbox_pass", &mut user_session)
                .contains("error id=0 msg=ok")
        );

        let list = runtime.execute("messagelist", &mut user_session);
        assert!(list.contains("msgid=1"));
        assert!(list.contains("subject=Welcome\\sNote"));
        assert!(list.contains("flag_read=0"));

        assert_eq!(
            runtime.execute("messagedel msgid=1", &mut user_session),
            "error id=0 msg=ok"
        );
        let empty_list = runtime.execute("messagelist", &mut user_session);
        assert!(empty_list.contains("error id=1281 msg=database\\sempty\\sresult\\sset"));
    }

    cleanup_state_path(&state_path);
}

#[test]
fn paket_b_complaint_system_and_permissions() {
    let state_path = temp_state_path("paket-b-complaints");
    let (alice_cldbid, bob_cldbid);
    {
        let mut runtime = create_baseline_runtime_with_state_path(workspace_root(), &state_path).expect("runtime should initialize");
        let mut admin = QuerySessionState::default();
        assert!(
            runtime
                .execute("login serveradmin serveradmin", &mut admin)
                .contains("error id=0 msg=ok")
        );

        let alice_created = runtime.execute(
            "querycreate client_login_name=alice_user client_login_password=alice_pass",
            &mut admin,
        );
        alice_cldbid = extract_field(&alice_created, "cldbid")
            .unwrap()
            .parse::<u64>()
            .unwrap();

        let bob_created = runtime.execute(
            "querycreate client_login_name=bob_user client_login_password=bob_pass",
            &mut admin,
        );
        bob_cldbid = extract_field(&bob_created, "cldbid")
            .unwrap()
            .parse::<u64>()
            .unwrap();

        // Set bob's needed complain power to 60 and alice's complain power to 40 initially
        assert_eq!(
            runtime.execute(
                &format!(
                    "clientaddperm cldbid={} permsid=i_client_needed_complain_power permvalue=60 permskip=0",
                    bob_cldbid
                ),
                &mut admin
            ),
            "error id=0 msg=ok"
        );
        assert_eq!(
            runtime.execute(
                &format!(
                    "clientaddperm cldbid={} permsid=i_client_complain_power permvalue=40 permskip=0",
                    alice_cldbid
                ),
                &mut admin
            ),
            "error id=0 msg=ok"
        );
        assert_eq!(
            runtime.execute(
                &format!(
                    "clientaddperm cldbid={} permsid=b_client_complain_delete_own permvalue=1 permskip=0",
                    alice_cldbid
                ),
                &mut admin
            ),
            "error id=0 msg=ok"
        );

        let mut alice = QuerySessionState::default();
        assert!(
            runtime
                .execute("login alice_user alice_pass", &mut alice)
                .contains("error id=0 msg=ok")
        );

        // Insufficient complain power (40 < 60)
        let denied = runtime.execute(
            &format!("complainadd tcldbid={} message=Loud\\smic", bob_cldbid),
            &mut alice,
        );
        assert!(denied.contains("error id=2568 msg=insufficient\\sclient\\spermissions"));

        // Raise alice's complain power to 60
        assert_eq!(
            runtime.execute(
                &format!(
                    "clientaddperm cldbid={} permsid=i_client_complain_power permvalue=60 permskip=0",
                    alice_cldbid
                ),
                &mut admin
            ),
            "error id=0 msg=ok"
        );

        assert_eq!(
            runtime.execute(
                &format!("complainadd tcldbid={} message=Loud\\smic", bob_cldbid),
                &mut alice
            ),
            "error id=0 msg=ok"
        );

        // Duplicate complaint from same sender to same target is rejected
        let duplicate = runtime.execute(
            &format!("complainadd tcldbid={} message=Again", bob_cldbid),
            &mut alice,
        );
        assert!(duplicate.contains("msg=complaint\\salready\\sexists"));
    }

    // Reload from persisted state and verify complaint is retained
    {
        let mut runtime = create_baseline_runtime_with_state_path(workspace_root(), &state_path).expect("runtime should initialize");
        let mut admin = QuerySessionState::default();
        assert!(
            runtime
                .execute("login serveradmin serveradmin", &mut admin)
                .contains("error id=0 msg=ok")
        );

        let list = runtime.execute(&format!("complainlist tcldbid={}", bob_cldbid), &mut admin);
        assert!(list.contains(&format!("tcldbid={}", bob_cldbid)));
        assert!(list.contains(&format!("fcldbid={}", alice_cldbid)));
        assert!(list.contains("message=Loud\\smic"));

        // Alice deletes her own complaint using b_client_complain_delete_own
        let mut alice = QuerySessionState::default();
        assert!(
            runtime
                .execute("login alice_user alice_pass", &mut alice)
                .contains("error id=0 msg=ok")
        );
        assert_eq!(
            runtime.execute(
                &format!("complaindel tcldbid={} fcldbid={}", bob_cldbid, alice_cldbid),
                &mut alice
            ),
            "error id=0 msg=ok"
        );

        // Re-add complaint and test complaindelall via admin
        assert_eq!(
            runtime.execute(
                &format!("complainadd tcldbid={} message=Second\\sreport", bob_cldbid),
                &mut alice
            ),
            "error id=0 msg=ok"
        );
        assert_eq!(
            runtime.execute(&format!("complaindelall tcldbid={}", bob_cldbid), &mut admin),
            "error id=0 msg=ok"
        );
        let empty = runtime.execute(&format!("complainlist tcldbid={}", bob_cldbid), &mut admin);
        assert!(empty.contains("error id=1281 msg=database\\sempty\\sresult\\sset"));
    }

    cleanup_state_path(&state_path);
}

#[test]
fn paket_b_custom_properties_clientdb_and_channelfind() {
    let state_path = temp_state_path("paket-b-custom-clientdb");
    let charlie_cldbid;
    {
        let mut runtime = create_baseline_runtime_with_state_path(workspace_root(), &state_path).expect("runtime should initialize");
        let mut admin = QuerySessionState::default();
        assert!(
            runtime
                .execute("login serveradmin serveradmin", &mut admin)
                .contains("error id=0 msg=ok")
        );

        let created = runtime.execute(
            "querycreate client_login_name=charlie_bot client_login_password=charlie_pass",
            &mut admin,
        );
        charlie_cldbid = extract_field(&created, "cldbid")
            .unwrap()
            .parse::<u64>()
            .unwrap();

        // Set custom client properties
        assert_eq!(
            runtime.execute(
                &format!(
                    "customset cldbid={} ident=discord_tag value=charlie#4242",
                    charlie_cldbid
                ),
                &mut admin
            ),
            "error id=0 msg=ok"
        );
        assert_eq!(
            runtime.execute(
                &format!(
                    "customset cldbid={} ident=squad_role value=pilot",
                    charlie_cldbid
                ),
                &mut admin
            ),
            "error id=0 msg=ok"
        );

        let info = runtime.execute(&format!("custominfo cldbid={}", charlie_cldbid), &mut admin);
        assert!(info.contains("ident=discord_tag value=charlie#4242"));
        assert!(info.contains("ident=squad_role value=pilot"));

        let search = runtime.execute(
            "customsearch ident=discord_tag pattern=%charlie%",
            &mut admin,
        );
        assert!(search.contains(&format!("cldbid={}", charlie_cldbid)));
        assert!(search.contains("value=charlie#4242"));
    }

    // Verify persistence of custom properties and test clientdb* + channelfind commands
    {
        let mut runtime = create_baseline_runtime_with_state_path(workspace_root(), &state_path).expect("runtime should initialize");
        let mut admin = QuerySessionState::default();
        assert!(
            runtime
                .execute("login serveradmin serveradmin", &mut admin)
                .contains("error id=0 msg=ok")
        );

        let info = runtime.execute(&format!("custominfo cldbid={}", charlie_cldbid), &mut admin);
        assert!(info.contains("ident=discord_tag value=charlie#4242"));

        assert_eq!(
            runtime.execute(
                &format!("customdelete cldbid={} ident=squad_role", charlie_cldbid),
                &mut admin
            ),
            "error id=0 msg=ok"
        );
        let info_after_del =
            runtime.execute(&format!("custominfo cldbid={}", charlie_cldbid), &mut admin);
        assert!(!info_after_del.contains("squad_role"));

        // clientdblist, clientdbinfo, clientdbedit, clientdbfind, clientdbdelete
        let dblist = runtime.execute("clientdblist -count", &mut admin);
        assert!(dblist.contains(&format!("cldbid={}", charlie_cldbid)));
        assert!(dblist.contains("count="));

        assert_eq!(
            runtime.execute(
                &format!(
                    "clientdbedit cldbid={} client_description=Veteran\\sPilot",
                    charlie_cldbid
                ),
                &mut admin
            ),
            "error id=0 msg=ok"
        );

        let dbinfo =
            runtime.execute(&format!("clientdbinfo cldbid={}", charlie_cldbid), &mut admin);
        assert!(dbinfo.contains("client_description=Veteran\\sPilot"));
        assert!(dbinfo.contains("client_unique_identifier=charlie_bot"));

        let dbfind_name = runtime.execute("clientdbfind pattern=%charlie%", &mut admin);
        assert!(dbfind_name.contains(&format!("cldbid={}", charlie_cldbid)));

        let dbfind_uid =
            runtime.execute("clientdbfind -uid pattern=charlie_bot", &mut admin);
        assert!(dbfind_uid.contains(&format!("cldbid={}", charlie_cldbid)));

        assert_eq!(
            runtime.execute(
                &format!("clientdbdelete cldbid={}", charlie_cldbid),
                &mut admin
            ),
            "error id=0 msg=ok"
        );
        let dbinfo_deleted =
            runtime.execute(&format!("clientdbinfo cldbid={}", charlie_cldbid), &mut admin);
        assert!(
            dbinfo_deleted.contains("msg=client\\snot\\sfound")
                || dbinfo_deleted.contains("msg=invalid\\sclientID")
        );

        // channelfind
        let channelfind = runtime.execute("channelfind pattern=Default", &mut admin);
        assert!(channelfind.contains("cid=1"));
        assert!(channelfind.contains("channel_name=Default\\sChannel"));
    }

    cleanup_state_path(&state_path);
}

#[test]
fn paket_b_file_transfer_icons_and_avatars() {
    let state_path = temp_state_path("paket-b-file-transfer");
    {
        let mut runtime = create_baseline_runtime_with_state_path(workspace_root(), &state_path).expect("runtime should initialize");
        let mut admin = QuerySessionState::default();
        assert!(
            runtime
                .execute("login serveradmin serveradmin", &mut admin)
                .contains("error id=0 msg=ok")
        );

        // 1. Create directory, upload file, list active transfers, list directory, get file info, download, rename, stop, delete
        let _ = runtime.execute("ftdeletefile cid=1 cpw= name=/docs", &mut admin);
        assert_eq!(
            runtime.execute("ftcreatedir cid=1 cpw= dirname=/docs", &mut admin),
            "error id=0 msg=ok"
        );

        let upload = runtime.execute(
            "ftinitupload clientftfid=101 name=/docs/manual.txt cid=1 cpw= size=256 overwrite=1 resume=0",
            &mut admin,
        );
        assert!(upload.contains("error id=0 msg=ok"));
        assert!(upload.contains("clientftfid=101"));
        let serverftfid = extract_field(&upload, "serverftfid")
            .expect("ftinitupload should return serverftfid");

        // ftlist without cid/path lists active transfers
        let active_transfers = runtime.execute("ftlist", &mut admin);
        assert!(active_transfers.contains(&format!("serverftfid={}", serverftfid)));
        assert!(active_transfers.contains("name=manual.txt"));
        assert!(active_transfers.contains("path=docs"));

        // ftgetfilelist lists the directory entries
        let filelist = runtime.execute("ftgetfilelist cid=1 cpw= path=/docs", &mut admin);
        assert!(filelist.contains("name=manual.txt"));
        assert!(filelist.contains("size=256"));
        assert!(filelist.contains("type=1"));

        // ftgetfileinfo returns file metadata
        let fileinfo =
            runtime.execute("ftgetfileinfo cid=1 cpw= name=/docs/manual.txt", &mut admin);
        assert!(fileinfo.contains("name=manual.txt"));
        assert!(fileinfo.contains("size=256"));

        // ftinitdownload prepares a download ticket
        let download = runtime.execute(
            "ftinitdownload clientftfid=102 name=/docs/manual.txt cid=1 cpw= seekpos=0",
            &mut admin,
        );
        assert!(download.contains("error id=0 msg=ok"));
        assert!(download.contains("size=256"));
        assert!(download.contains("ftkey="));

        // ftrenamefile renames the file in place
        assert_eq!(
            runtime.execute(
                "ftrenamefile cid=1 cpw= oldname=/docs/manual.txt newname=/docs/handbook.txt",
                &mut admin
            ),
            "error id=0 msg=ok"
        );
        let renamed_info = runtime.execute(
            "ftgetfileinfo cid=1 cpw= name=/docs/handbook.txt",
            &mut admin,
        );
        assert!(renamed_info.contains("size=256"));

        // ftstop stops the active upload ticket
        assert_eq!(
            runtime.execute(
                &format!("ftstop serverftfid={} delete=0", serverftfid),
                &mut admin
            ),
            "error id=0 msg=ok"
        );

        // ftdeletefile removes the file
        assert_eq!(
            runtime.execute("ftdeletefile cid=1 cpw= name=/docs/handbook.txt", &mut admin),
            "error id=0 msg=ok"
        );
        let deleted_info = runtime.execute(
            "ftgetfileinfo cid=1 cpw= name=/docs/handbook.txt",
            &mut admin,
        );
        assert!(deleted_info.contains("error id=2051 msg=file\\snot\\sfound"));
        let _ = runtime.execute("ftdeletefile cid=1 cpw= name=/docs", &mut admin);

        // 2. Channel password & upload power checks + Icon & Avatar management
        assert_eq!(
            runtime.execute("channeledit cid=2 channel_password=ftsecret", &mut admin),
            "error id=0 msg=ok"
        );
        assert_eq!(
            runtime.execute(
                "channeladdperm cid=2 permsid=i_ft_needed_file_upload_power permvalue=70",
                &mut admin
            ),
            "error id=0 msg=ok"
        );

        let mut user_session = QuerySessionState {
            client_id: 11,
            selected_virtual_server_id: Some(1),
            current_channel_id: Some(1),
            actor_client_database_id_override: Some(41),
            is_desktop_client: true,
            ..QuerySessionState::default()
        };
        assert_eq!(
            runtime.execute(
                "clientaddperm cldbid=41 permsid=i_ft_file_upload_power permvalue=50 permskip=0",
                &mut admin
            ),
            "error id=0 msg=ok"
        );
        assert_eq!(
            runtime.execute(
                "clientaddperm cldbid=41 permsid=i_client_max_avatar_filesize permvalue=4096 permskip=0",
                &mut admin
            ),
            "error id=0 msg=ok"
        );

        // Wrong channel password -> 781
        let wrong_pw = runtime.execute(
            "ftinitupload clientftfid=201 name=/test.bin cid=2 cpw=wrong size=64 overwrite=1 resume=0",
            &mut user_session,
        );
        assert!(wrong_pw.contains("error id=781 msg=invalid\\schannel\\spassword"));

        // Correct password but insufficient upload power (50 < 70) -> 2568
        let low_power = runtime.execute(
            "ftinitupload clientftfid=202 name=/test.bin cid=2 cpw=ftsecret size=64 overwrite=1 resume=0",
            &mut user_session,
        );
        assert!(low_power.contains("error id=2568 msg=insufficient\\sclient\\spermissions"));

        // Icon upload without b_icon_manage -> 2568
        let icon_denied = runtime.execute(
            "ftinitupload clientftfid=203 name=/icon_999 cid=0 cpw= size=64 overwrite=1 resume=0",
            &mut user_session,
        );
        assert!(icon_denied.contains("error id=2568 msg=insufficient\\sclient\\spermissions"));

        // Admin uploads icon and lists /icons
        let icon_ok = runtime.execute(
            "ftinitupload clientftfid=204 name=/icon_999 cid=0 cpw= size=64 overwrite=1 resume=0",
            &mut admin,
        );
        assert!(icon_ok.contains("error id=0 msg=ok"));
        let icons_list = runtime.execute("ftgetfilelist cid=0 cpw= path=/icons", &mut admin);
        assert!(icons_list.contains("name=icon_999"));
        let _ = runtime.execute("ftdeletefile cid=0 cpw= name=/icon_999", &mut admin);

        // User uploads avatar (/avatar on cid=0) and updates client_flag_avatar
        let avatar_too_big = runtime.execute(
            "ftinitupload clientftfid=205 name=/avatar cid=0 cpw= size=8192 overwrite=1 resume=0",
            &mut user_session,
        );
        assert!(avatar_too_big.contains("error id=2568 msg=insufficient\\sclient\\spermissions"));

        let avatar_ok = runtime.execute(
            "ftinitupload clientftfid=206 name=/avatar cid=0 cpw= size=512 overwrite=1 resume=0",
            &mut user_session,
        );
        assert!(avatar_ok.contains("error id=0 msg=ok"));

        let client_info_with_avatar = runtime.execute("clientinfo clid=11", &mut admin);
        let flag_avatar = extract_field(&client_info_with_avatar, "client_flag_avatar")
            .expect("clientinfo should include client_flag_avatar");
        assert!(!flag_avatar.is_empty());

        // Deleting /avatar clears client_flag_avatar
        assert_eq!(
            runtime.execute("ftdeletefile cid=0 cpw= name=/avatar", &mut user_session),
            "error id=0 msg=ok"
        );
        let client_info_after_del = runtime.execute("clientinfo clid=11", &mut admin);
        assert!(client_info_after_del.contains("client_flag_avatar placeholder=") || {
            extract_field(&client_info_after_del, "client_flag_avatar")
                .unwrap_or_default()
                .is_empty()
        });
    }

    cleanup_state_path(&state_path);
}

#[test]
fn paket_c_virtual_server_lifecycle_create_stop_start_delete() {
    let state_path = temp_state_path("paket-c-vserver-lifecycle");
    {
        let mut runtime = create_baseline_runtime_with_state_path(workspace_root(), &state_path)
            .expect("runtime should initialize");
        let mut admin = QuerySessionState::default();
        assert!(
            runtime
                .execute("login serveradmin serveradmin", &mut admin)
                .contains("error id=0 msg=ok")
        );

        let created = runtime.execute(
            "servercreate virtualserver_name=Second\\sServer virtualserver_port=9988 virtualserver_maxclients=64",
            &mut admin,
        );
        assert!(created.contains("error id=0 msg=ok"), "created: {}", created);
        assert!(created.contains("sid=2"));
        assert!(created.contains("virtualserver_port=9988"));
        assert!(created.contains("token="));

        let list = runtime.execute("serverlist", &mut admin);
        assert!(list.contains("virtualserver_id=1"));
        assert!(list.contains("virtualserver_id=2"));
        assert!(list.contains("virtualserver_name=Second\\sServer"));

        let hostinfo = runtime.execute("hostinfo", &mut admin);
        assert!(hostinfo.contains("virtualservers_running_total=2"));

        let by_port = runtime.execute("serveridgetbyport virtualserver_port=9988", &mut admin);
        assert!(by_port.contains("server_id=2"));

        assert_eq!(runtime.execute("use sid=2", &mut admin), "error id=0 msg=ok");
        let create_ch = runtime.execute(
            "channelcreate channel_name=Server2\\sLobby channel_flag_permanent=1",
            &mut admin,
        );
        assert!(create_ch.contains("error id=0 msg=ok"), "create_ch: {}", create_ch);

        // Cannot delete a running virtual server
        let del_running = runtime.execute("serverdelete sid=2", &mut admin);
        assert!(del_running.contains("error id=1025 msg=server\\sis\\srunning"));

        // Stop server 2
        assert_eq!(
            runtime.execute("serverstop sid=2 reasonmsg=Maintenance", &mut admin),
            "error id=0 msg=ok"
        );
        let offline_list = runtime.execute("serverlist -onlyoffline", &mut admin);
        assert!(offline_list.contains("virtualserver_id=2"));
        assert!(offline_list.contains("virtualserver_status=offline"));
        assert!(!offline_list.contains("virtualserver_id=1"));

        let hostinfo_after_stop = runtime.execute("hostinfo", &mut admin);
        assert!(hostinfo_after_stop.contains("virtualservers_running_total=1"));

        // Normal `use sid=2` fails while offline, but `use sid=2 -virtual` succeeds
        let use_offline = runtime.execute("use sid=2", &mut admin);
        assert!(use_offline.contains("error id=1033 msg=server\\sis\\snot\\srunning"));
        assert_eq!(
            runtime.execute("use sid=2 -virtual", &mut admin),
            "error id=0 msg=ok"
        );
        let s2_info = runtime.execute("serverinfo", &mut admin);
        assert!(s2_info.contains("virtualserver_status=virtual\\sonline"));

        // Start server 2 again
        assert_eq!(
            runtime.execute("serverstart sid=2", &mut admin),
            "error id=0 msg=ok"
        );
    }

    // Reload from persisted state and verify server 2 and its channel survived
    {
        let mut runtime = create_baseline_runtime_with_state_path(workspace_root(), &state_path)
            .expect("runtime should initialize");
        let mut admin = QuerySessionState::default();
        assert!(
            runtime
                .execute("login serveradmin serveradmin", &mut admin)
                .contains("error id=0 msg=ok")
        );

        assert_eq!(runtime.execute("use sid=2", &mut admin), "error id=0 msg=ok");
        let chlist = runtime.execute("channellist", &mut admin);
        assert!(chlist.contains("channel_name=Server2\\sLobby"));

        // Stop and delete server 2
        assert_eq!(
            runtime.execute("serverstop sid=2", &mut admin),
            "error id=0 msg=ok"
        );
        assert_eq!(
            runtime.execute("serverdelete sid=2", &mut admin),
            "error id=0 msg=ok"
        );
        let list_after_del = runtime.execute("serverlist", &mut admin);
        assert!(!list_after_del.contains("virtualserver_id=2"));

        assert_eq!(
            runtime.execute("serverprocessstop reasonmsg=Planned\\sRestart", &mut admin),
            "error id=0 msg=ok"
        );
    }

    cleanup_state_path(&state_path);
}

#[test]
fn paket_c_server_snapshot_create_and_deploy() {
    let state_path = temp_state_path("paket-c-server-snapshot");
    {
        let mut runtime = create_baseline_runtime_with_state_path(workspace_root(), &state_path)
            .expect("runtime should initialize");
        let mut admin = QuerySessionState::default();
        assert!(
            runtime
                .execute("login serveradmin serveradmin", &mut admin)
                .contains("error id=0 msg=ok")
        );

        assert_eq!(
            runtime.execute(
                "serveredit virtualserver_name=Snapshot\\sSource virtualserver_welcomemessage=SnapWelcome",
                &mut admin
            ),
            "error id=0 msg=ok"
        );
        let raid_ch = runtime.execute(
            "channelcreate channel_name=Raid\\sRoom channel_flag_permanent=1 channel_needed_talk_power=25",
            &mut admin,
        );
        let raid_cid = extract_field(&raid_ch, "cid").expect("cid should be returned");

        let snap = runtime.execute("serversnapshotcreate", &mut admin);
        assert!(snap.contains("error id=0 msg=ok"), "snap: {}", snap);
        let data = extract_field(&snap, "data").expect("snapshot data should be present");

        // Mutate server and delete Raid Room
        assert_eq!(
            runtime.execute("serveredit virtualserver_name=Mutated\\sServer", &mut admin),
            "error id=0 msg=ok"
        );
        assert_eq!(
            runtime.execute(&format!("channeldelete cid={} force=1", raid_cid), &mut admin),
            "error id=0 msg=ok"
        );
        let chlist_before = runtime.execute("channellist", &mut admin);
        assert!(!chlist_before.contains("channel_name=Raid\\sRoom"));

        // Deploy snapshot with -mapping
        let deploy = runtime.execute(
            &format!("serversnapshotdeploy -mapping version=2 data={}", data),
            &mut admin,
        );
        assert!(deploy.contains("error id=0 msg=ok"), "deploy: {}", deploy);
        assert!(deploy.contains("ocid="));
        assert!(deploy.contains("ncid="));

        let sinfo = runtime.execute("serverinfo", &mut admin);
        assert!(sinfo.contains("virtualserver_name=Snapshot\\sSource"));
        assert!(sinfo.contains("virtualserver_welcomemessage=SnapWelcome"));

        let chlist_after = runtime.execute("channellist -voice", &mut admin);
        assert!(chlist_after.contains("channel_name=Raid\\sRoom"));
        assert!(chlist_after.contains("channel_needed_talk_power=25"));
    }

    cleanup_state_path(&state_path);
}

#[test]
fn paket_c_logs_instanceedit_gm_and_conversations() {
    let state_path = temp_state_path("paket-c-logs-instance-conv");
    {
        let mut runtime = create_baseline_runtime_with_state_path(workspace_root(), &state_path)
            .expect("runtime should initialize");
        let mut admin = QuerySessionState::default();
        assert!(
            runtime
                .execute("login serveradmin serveradmin", &mut admin)
                .contains("error id=0 msg=ok")
        );

        assert_eq!(
            runtime.execute(
                "logadd loglevel=2 logmsg=Custom\\sAudit\\sEvent",
                &mut admin
            ),
            "error id=0 msg=ok"
        );
        let logview = runtime.execute("logview lines=10", &mut admin);
        assert!(logview.contains("error id=0 msg=ok"));
        assert!(logview.contains("Custom\\sAudit\\sEvent"));

        let logquery = runtime.execute("logquery lines=5", &mut admin);
        assert!(logquery.contains("error id=0 msg=ok"));
        assert!(logquery.contains("Custom\\sAudit\\sEvent"));

        assert_eq!(
            runtime.execute(
                "instanceedit serverinstance_filetransfer_port=30034 serverinstance_max_download_total_bandwidth=123456",
                &mut admin
            ),
            "error id=0 msg=ok"
        );
        let inst = runtime.execute("instanceinfo", &mut admin);
        assert!(inst.contains("serverinstance_filetransfer_port=30034"));
        assert!(inst.contains("serverinstance_max_download_total_bandwidth=123456"));

        assert_eq!(
            runtime.execute("gm msg=Global\\sMaintenance\\sNotice", &mut admin),
            "error id=0 msg=ok"
        );

        let added_ban = runtime.execute(
            "banadd ip=203.0.113.55 time=300 banreason=Trigger\\sTest",
            &mut admin,
        );
        assert!(added_ban.contains("error id=0 msg=ok"));
        let triggers = runtime.execute("bantriggerlist", &mut admin);
        assert!(triggers.contains("error id=0 msg=ok"), "triggers: {}", triggers);

        // Send a channel text message and verify conversationfetch / conversationhistory
        assert_eq!(
            runtime.execute(
                "sendtextmessage targetmode=2 target=1 msg=Hello\\sChannel\\sHistory",
                &mut admin
            ),
            "error id=0 msg=ok"
        );
        let conv_fetch = runtime.execute("conversationfetch cid=1", &mut admin);
        assert!(conv_fetch.contains("error id=0 msg=ok"));
        assert!(conv_fetch.contains("cid=1"));

        let conv_hist = runtime.execute("conversationhistory cid=1", &mut admin);
        assert!(conv_hist.contains("error id=0 msg=ok"));
        assert!(conv_hist.contains("msg=Hello\\sChannel\\sHistory"));
    }

    // Verify instanceedit and logs persist across reload
    {
        let mut runtime = create_baseline_runtime_with_state_path(workspace_root(), &state_path)
            .expect("runtime should initialize");
        let mut admin = QuerySessionState::default();
        assert!(
            runtime
                .execute("login serveradmin serveradmin", &mut admin)
                .contains("error id=0 msg=ok")
        );

        let inst = runtime.execute("instanceinfo", &mut admin);
        assert!(inst.contains("serverinstance_filetransfer_port=30034"));

        let logview = runtime.execute("logview lines=20", &mut admin);
        assert!(logview.contains("Custom\\sAudit\\sEvent"));
    }

    cleanup_state_path(&state_path);
}

#[test]
fn paket_d_playlists_apikeys_complain_autoban_and_temp_channels_work() {
    let state_path = temp_state_path("paket-d");
    cleanup_state_path(&state_path);

    let mut runtime = create_baseline_runtime_with_state_path(workspace_root(), &state_path)
        .expect("runtime should initialize");
    let mut admin = QuerySessionState::default();
    assert!(
        runtime
            .execute("login serveradmin serveradmin", &mut admin)
            .contains("error id=0 msg=ok")
    );
    assert_eq!(runtime.execute("use 1", &mut admin), "error id=0 msg=ok");

    // 1. Playlist create, permissions, assign to music bot, delete
    let created_pl = runtime.execute(
        "playlistcreate playlist_title=Party\\sMix playlist_description=Evening\\sSet",
        &mut admin,
    );
    assert!(
        created_pl.contains("playlist_id=") && created_pl.contains("error id=0 msg=ok"),
        "playlistcreate failed: {created_pl}"
    );
    let playlist_id: u32 = created_pl
        .lines()
        .next()
        .unwrap()
        .split_whitespace()
        .find_map(|token| token.strip_prefix("playlist_id="))
        .unwrap()
        .parse()
        .unwrap();

    // Standalone playlist should not appear in musicbotlist
    let bots_before = runtime.execute("musicbotlist", &mut admin);
    assert!(!bots_before.contains("Party\\sMix"));

    // Add and delete playlist permission & client permission
    assert!(
        runtime
            .execute(
                &format!(
                    "playlistaddperm playlist_id={playlist_id} permsid=i_playlist_needed_modify_power permvalue=50 permskip=0 permnegated=0"
                ),
                &mut admin,
            )
            .contains("error id=0 msg=ok")
    );
    assert_eq!(
        runtime.execute(
            &format!(
                "playlistdelperm playlist_id={playlist_id} permsid=i_playlist_needed_modify_power"
            ),
            &mut admin,
        ),
        "error id=0 msg=ok"
    );

    assert!(
        runtime
            .execute(
                &format!(
                    "playlistclientaddperm playlist_id={playlist_id} cldbid=1 permsid=i_playlist_modify_power permvalue=75 permskip=0 permnegated=0"
                ),
                &mut admin,
            )
            .contains("error id=0 msg=ok")
    );
    assert_eq!(
        runtime.execute(
            &format!(
                "playlistdelclientperm playlist_id={playlist_id} cldbid=1 permsid=i_playlist_modify_power"
            ),
            &mut admin,
        ),
        "error id=0 msg=ok"
    );

    // Add a song to the standalone playlist, create a musicbot, and assign the playlist to the bot
    assert!(
        runtime
            .execute(
                &format!(
                    "playlistsongadd playlist_id={playlist_id} url=https://example.com/song.mp3"
                ),
                &mut admin,
            )
            .contains("error id=0 msg=ok")
    );
    let created_bot = runtime.execute("musicbotcreate cid=1 botname=DJBot", &mut admin);
    assert!(created_bot.contains("error id=0 msg=ok"));
    let bot_id: u32 = created_bot
        .lines()
        .next()
        .unwrap()
        .split_whitespace()
        .find_map(|token| token.strip_prefix("botid="))
        .unwrap()
        .parse()
        .unwrap();

    assert_eq!(
        runtime.execute(
            &format!("musicbotplaylistassign botid={bot_id} playlist_id={playlist_id}"),
            &mut admin,
        ),
        "error id=0 msg=ok"
    );
    let bot_queue = runtime.execute(&format!("musicbotqueuelist botid={bot_id}"), &mut admin);
    assert!(
        bot_queue.contains("example.com") && bot_queue.contains("song.mp3"),
        "expected assigned playlist queue in bot: {bot_queue}"
    );

    // Create another standalone playlist and delete it
    let created_pl2 = runtime.execute("playlistcreate playlist_title=TempPlaylist", &mut admin);
    let playlist_id2: u32 = created_pl2
        .lines()
        .next()
        .unwrap()
        .split_whitespace()
        .find_map(|token| token.strip_prefix("playlist_id="))
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(
        runtime.execute(
            &format!("playlistdelete playlist_id={playlist_id2}"),
            &mut admin
        ),
        "error id=0 msg=ok"
    );

    // 2. API keys: apikeyadd, apikeylist, apikeydel
    let added_key = runtime.execute("apikeyadd scope=manage lifetime=7", &mut admin);
    assert!(
        added_key.contains("apikey=BTEA-APIKEY-") && added_key.contains("error id=0 msg=ok"),
        "apikeyadd failed: {added_key}"
    );
    let key_id: u64 = added_key
        .lines()
        .next()
        .unwrap()
        .split_whitespace()
        .find_map(|token| token.strip_prefix("id="))
        .unwrap()
        .parse()
        .unwrap();

    let key_list = runtime.execute("apikeylist cldbid=*", &mut admin);
    assert!(
        key_list.contains(&format!("id={key_id}")) && key_list.contains("scope=manage"),
        "apikeylist failed: {key_list}"
    );
    assert_eq!(
        runtime.execute(&format!("apikeydel id={key_id}"), &mut admin),
        "error id=0 msg=ok"
    );

    // 3. Complaint Auto-Ban when virtualserver_complain_autoban_count is reached
    assert_eq!(
        runtime.execute(
            "serveredit virtualserver_complain_autoban_count=1 virtualserver_complain_autoban_time=600",
            &mut admin,
        ),
        "error id=0 msg=ok"
    );
    let sinfo = runtime.execute("serverinfo", &mut admin);
    assert!(sinfo.contains("virtualserver_complain_autoban_count=1"));
    assert!(sinfo.contains("virtualserver_complain_autoban_time=600"));

    // Create a query account to have a second client database ID to complain about
    let qcreate = runtime.execute("querycreate client_login_name=troublemaker", &mut admin);
    assert!(qcreate.contains("error id=0 msg=ok"));
    let target_cldbid: u64 = qcreate
        .lines()
        .next()
        .unwrap()
        .split_whitespace()
        .find_map(|token| token.strip_prefix("cldbid="))
        .unwrap()
        .parse()
        .unwrap();

    assert_eq!(
        runtime.execute(
            &format!("complainadd tcldbid={target_cldbid} message=Spamming\\schat"),
            &mut admin,
        ),
        "error id=0 msg=ok"
    );
    let bans = runtime.execute("banlist", &mut admin);
    assert!(
        bans.contains("Too\\smany\\scomplaints"),
        "expected auto-ban in banlist: {bans}"
    );

    // 4. Temporary channel auto-cleanup when client moves out
    let temp_ch = runtime.execute(
        "channelcreate channel_name=EphemeralRoom channel_flag_permanent=0",
        &mut admin,
    );
    assert!(temp_ch.contains("error id=0 msg=ok"));
    let temp_cid: u32 = temp_ch
        .lines()
        .next()
        .unwrap()
        .split_whitespace()
        .find_map(|token| token.strip_prefix("cid="))
        .unwrap()
        .parse()
        .unwrap();

    // Move admin session into temporary channel and then back to default channel 1
    assert_eq!(
        runtime.execute(&format!("clientmove clid={} cid={temp_cid}", admin.client_id), &mut admin),
        "error id=0 msg=ok"
    );
    assert!(runtime.execute("channellist", &mut admin).contains("EphemeralRoom"));

    assert_eq!(
        runtime.execute(&format!("clientmove clid={} cid=1", admin.client_id), &mut admin),
        "error id=0 msg=ok"
    );
    assert!(
        !runtime.execute("channellist", &mut admin).contains("EphemeralRoom"),
        "expected temporary channel EphemeralRoom to be automatically deleted after leaving"
    );

    cleanup_state_path(&state_path);
}

#[test]
fn paket_f_tsdns_resolver_and_tcp_server_work() {
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpStream;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};

    let state_path = temp_state_path("paket-f-tsdns");
    let runtime = Arc::new(Mutex::new(
        create_baseline_runtime_with_state_path(workspace_root(), &state_path)
            .expect("runtime should initialize"),
    ));

    {
        let mut rt = runtime.lock().unwrap();
        let mut admin = QuerySessionState::default();
        assert!(
            rt.execute("login serveradmin serveradmin", &mut admin)
                .contains("error id=0 msg=ok")
        );

        // Add exact and wildcard TSDNS rules
        assert_eq!(
            rt.execute("tsdnsadd domain=voice.clan.de target=198.51.100.42:9987", &mut admin),
            "error id=0 msg=ok"
        );
        assert_eq!(
            rt.execute("tsdnsadd domain=*.clan.de target=198.51.100.99:9988", &mut admin),
            "error id=0 msg=ok"
        );

        let list = rt.execute("tsdnslist", &mut admin);
        assert!(list.contains("domain=voice.clan.de target=198.51.100.42:9987"));
        assert!(list.contains("domain=*.clan.de target=198.51.100.99:9988"));

        let res_exact = rt.execute("tsdnsresolve domain=voice.clan.de", &mut admin);
        assert!(res_exact.contains("target=198.51.100.42:9987"));

        let res_wild = rt.execute("tsdnsresolve domain=eu.clan.de", &mut admin);
        assert!(res_wild.contains("target=198.51.100.99:9988"));
    }

    // Bind live TsdnsTcpServer on ephemeral port and test wire protocol (<hostname>\r\n -> <ip>:<port>\r\n or 404\r\n)
    let server = blackteaspeak_server::tsdns::TsdnsTcpServer::bind_with_shared_runtime(
        Arc::clone(&runtime),
        "127.0.0.1:0",
    )
    .expect("tsdns tcp server should bind");
    let addr = server.local_addr().unwrap();
    let should_stop = Arc::new(AtomicBool::new(false));
    let stop_clone = Arc::clone(&should_stop);
    let handle = std::thread::spawn(move || {
        let _ = server.run(stop_clone);
    });

    let query_tcp = |hostname: &str| -> String {
        let mut stream = TcpStream::connect(addr).expect("connect to tsdns server");
        stream
            .write_all(format!("{hostname}\r\n").as_bytes())
            .unwrap();
        stream.flush().unwrap();
        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        line.trim().to_string()
    };

    assert_eq!(query_tcp("voice.clan.de"), "198.51.100.42:9987");
    assert_eq!(query_tcp("sub.clan.de"), "198.51.100.99:9988");
    assert_eq!(query_tcp("unknown.example.org"), "404");

    // Delete rule and verify fallback to wildcard
    {
        let mut rt = runtime.lock().unwrap();
        let mut admin = QuerySessionState::default();
        let _ = rt.execute("login serveradmin serveradmin", &mut admin);
        assert_eq!(
            rt.execute("tsdnsdel domain=voice.clan.de", &mut admin),
            "error id=0 msg=ok"
        );
    }
    assert_eq!(query_tcp("voice.clan.de"), "198.51.100.99:9988");

    should_stop.store(true, Ordering::SeqCst);
    let _ = handle.join();
    cleanup_state_path(&state_path);
}

#[test]
fn teaspeak_client_protocol_and_webrtc_broadcast_parity_works() {
    let state_path = temp_state_path("teaspeak-client-parity");
    let mut runtime = create_baseline_runtime_with_state_path(workspace_root(), &state_path)
        .expect("runtime should initialize");
    let mut session = QuerySessionState::default();

    // 1. TeaSpeak Handshake commands
    let hb = runtime.execute(
        "handshakebegin intention=0 authentication_method=0 public_key=dGVhc3BlYWstdGVzdC1rZXk=",
        &mut session,
    );
    assert!(hb.contains("error id=0 msg=ok"), "hb: {hb}");
    assert!(hb.contains("digest=SHA-256"));
    assert!(hb.contains("message="));

    let proof = runtime.execute("handshakeidentityproof proof=dummy-sig", &mut session);
    assert_eq!(proof, "error id=0 msg=ok");
    let proof_typo = runtime.execute("handshakeindentityproof proof=dummy-sig", &mut session);
    assert_eq!(proof_typo, "error id=0 msg=ok");

    assert!(
        runtime
            .execute("login serveradmin serveradmin", &mut session)
            .contains("error id=0 msg=ok")
    );
    assert_eq!(runtime.execute("use 1", &mut session), "error id=0 msg=ok");

    // 2. Feature discovery via listfeaturesupport and TeaClient alias featurelist
    let features = runtime.execute("featurelist", &mut session);
    for expected in [
        "name=error-bulks",
        "name=advanced-channel-chat",
        "name=whisper-echo",
        "name=video",
        "name=screen-sharing",
        "name=music-bots",
        "name=playlists",
        "name=token-actions",
        "name=query-management",
        "name=web-rtc",
    ] {
        assert!(
            features.contains(expected),
            "expected feature {expected} in featurelist: {features}"
        );
    }

    // 3. TeaClient variable & connection info aliases
    let srv_vars = runtime.execute("servergetvariables", &mut session);
    assert!(srv_vars.contains("virtualserver_name="), "srv_vars: {srv_vars}");

    let cli_vars = runtime.execute(
        &format!("clientgetvariables clid={}", session.client_id),
        &mut session,
    );
    assert!(
        cli_vars.contains(&format!("clid={}", session.client_id)),
        "cli_vars: {cli_vars}"
    );

    let conn_info = runtime.execute("serverconnectioninfo", &mut session);
    assert!(
        conn_info.contains("connection_bytes_sent_total="),
        "conn_info: {conn_info}"
    );

    // 4. WebRTC signaling & Video / Screen-Sharing / Audio Broadcasts
    let rtc_desc = runtime.execute(
        "rtcsessiondescribe mode=offer sdp=v=0\\r\\no=-\\s0\\s0\\sIN\\sIP4\\s127.0.0.1",
        &mut session,
    );
    assert!(rtc_desc.contains("mode=answer"), "rtc_desc: {rtc_desc}");
    assert!(rtc_desc.contains("sdp=v=0"));

    assert_eq!(
        runtime.execute(
            "rtcicecandidate media_line=0 sdp_mid=0 candidate=candidate:1\\s1\\sUDP\\s2130706431\\s127.0.0.1\\s50000\\styp\\shost",
            &mut session,
        ),
        "error id=0 msg=ok"
    );

    // Start Camera (bt=0) and Screen-Share (bt=1) + Audio broadcast
    let bvid0 = runtime.execute(
        "broadcastvideo bt=0 ssrc=4100 max_bandwidth=2500000 max_framerate=60 width=1920 height=1080",
        &mut session,
    );
    assert!(bvid0.contains("error id=0 msg=ok") && bvid0.contains("ssrc=4100"), "bvid0: {bvid0}");
    let bvid1 = runtime.execute("broadcastvideo bt=1 ssrc=4101", &mut session);
    assert!(bvid1.contains("error id=0 msg=ok") && bvid1.contains("ssrc=4101"), "bvid1: {bvid1}");
    let baud = runtime.execute("broadcastaudio ssrc=4200", &mut session);
    assert!(baud.contains("error id=0 msg=ok") && baud.contains("ssrc=4200"), "baud: {baud}");

    let bcast_list = runtime.execute("broadcastlist", &mut session);
    assert!(
        bcast_list.contains(&format!("scl={}", session.client_id)),
        "bcast_list: {bcast_list}"
    );
    assert!(bcast_list.contains("ssrc=4100"));
    assert!(bcast_list.contains("ssrc=4101"));
    assert!(bcast_list.contains("ssrc=4200"));


    // Join video & audio broadcasts
    let vjoin = runtime.execute(
        &format!("broadcastvideojoin scl={} bt=0", session.client_id),
        &mut session,
    );
    assert!(vjoin.contains("ssrc=4100"), "vjoin: {vjoin}");

    let ajoin = runtime.execute(
        &format!("broadcastaudiojoin scl={}", session.client_id),
        &mut session,
    );
    assert!(ajoin.contains("ssrc=4200"), "ajoin: {ajoin}");

    // Leave broadcast and reset RTC session
    assert_eq!(
        runtime.execute(
            &format!("broadcastleave scl={} bt=0", session.client_id),
            &mut session,
        ),
        "error id=0 msg=ok"
    );
    assert_eq!(
        runtime.execute("rtcsessionreset", &mut session),
        "error id=0 msg=ok"
    );

    // 5. TeaSpeak Whisper Session Initialize & Reset
    assert_eq!(
        runtime.execute(
            "whispersessioninitialize ssrc=9001 type=2 target=0 id=1",
            &mut session,
        ),
        "error id=0 msg=ok"
    );
    assert!(session.whisper_targets.is_some());
    assert_eq!(
        runtime.execute("whispersessionreset", &mut session),
        "error id=0 msg=ok"
    );
    assert!(session.whisper_targets.is_none());

    // 6. TeaSpeak Conversation Message Delete
    assert_eq!(
        runtime.execute(
            "sendtextmessage targetmode=2 target=1 msg=TeaSpeak\\sConversation\\sMsg",
            &mut session,
        ),
        "error id=0 msg=ok"
    );
    let del_msg = runtime.execute(
        "conversationmessagedelete cid=1 timestamp_begin=0 timestamp_end=9999999999999 limit=10",
        &mut session,
    );
    assert!(del_msg.contains("error id=0 msg=ok"), "del_msg: {del_msg}");
    assert!(del_msg.contains("deleted=1"), "del_msg: {del_msg}");

    cleanup_state_path(&state_path);
}










