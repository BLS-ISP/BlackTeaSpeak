use super::*;
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

