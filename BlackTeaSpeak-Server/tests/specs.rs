use std::path::PathBuf;

use blackteaspeak_server::specs::FoundationSpecs;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate should live inside workspace root")
        .to_path_buf()
}

#[test]
fn loads_foundation_specs() {
    let specs = FoundationSpecs::load(workspace_root()).expect("foundation should load");

    assert_eq!(specs.commands.len(), 170);
    assert_eq!(specs.permission_groups.len(), 8);
    assert!(specs.permission_catalog.len() >= 300);
    assert!(specs.get_command("login").is_some());
    assert!(
        specs
            .get_permission_group("serverinstance_admin_serverquery_group")
            .is_some()
    );
    let help_view = specs
        .get_permission_catalog_entry("b_serverinstance_help_view")
        .expect("permission catalog should contain b_serverinstance_help_view");
    assert_eq!(help_view.id, 4353);
    assert_eq!(help_view.description, "View server instance help");
    assert_eq!(specs.build_version.build_version, "1.5.6");
    assert!(
        specs
            .baseline_command_names()
            .contains("musicbotplayeraction")
    );
    assert!(specs.get_command("musicbotqueueadd").is_some());
    assert!(specs.get_command("musicbotsetsubscription").is_some());
    assert!(specs.get_command("playlistsetsubscription").is_some());
}
