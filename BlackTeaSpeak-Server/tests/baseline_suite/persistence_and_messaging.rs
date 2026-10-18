use super::*;
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

