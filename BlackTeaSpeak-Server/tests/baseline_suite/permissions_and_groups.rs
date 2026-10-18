use super::*;
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

