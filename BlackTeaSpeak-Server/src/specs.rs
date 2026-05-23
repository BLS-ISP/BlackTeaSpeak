use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::models::{
    BaselineProfile, BinaryManifest, BuildVersion, CommandSpec, PermissionCatalogEntry,
    PermissionGroupSpec, PermissionMappingGroup,
};

#[derive(Debug, Clone)]
pub struct FoundationSpecs {
    pub workspace_root: PathBuf,
    pub binary_manifest: BinaryManifest,
    pub commands: BTreeMap<String, CommandSpec>,
    pub permission_groups: Vec<PermissionGroupSpec>,
    pub permission_groups_by_property: HashMap<String, PermissionGroupSpec>,
    pub permission_mapping_groups: Vec<PermissionMappingGroup>,
    pub permission_catalog: Vec<PermissionCatalogEntry>,
    pub permission_catalog_by_name: HashMap<String, PermissionCatalogEntry>,
    pub subsystems: Vec<Value>,
    pub baseline_profile: BaselineProfile,
    pub build_version: BuildVersion,
}

impl FoundationSpecs {
    pub fn load(workspace_root: impl AsRef<Path>) -> Result<Self> {
        let workspace_root = workspace_root.as_ref().to_path_buf();
        let foundation_dir = workspace_root
            .join("data")
            .join("foundation");

        let binary_manifest =
            read_json::<BinaryManifest>(&foundation_dir.join("binary-manifest.json"))?;
        let commands_list =
            read_json::<Vec<CommandSpec>>(&foundation_dir.join("commands-manifest.json"))?;
        let permission_groups =
            read_json::<Vec<PermissionGroupSpec>>(&foundation_dir.join("permission-groups.json"))?;
        let permission_mapping_groups = read_json::<Vec<PermissionMappingGroup>>(
            &foundation_dir.join("permission-mapping.json"),
        )?;
        let permission_catalog = read_json::<Vec<PermissionCatalogEntry>>(
            &foundation_dir.join("permission-catalog.json"),
        )?;
        let subsystems = read_json::<Vec<Value>>(&foundation_dir.join("subsystems.json"))?;
        let baseline_profile =
            read_json::<BaselineProfile>(&foundation_dir.join("query-baseline.json"))?;
        let build_version = read_build_version(
            &workspace_root
                .join("BlackTeaSpeak-1.5.6")
                .join("buildVersion.txt"),
        )?;

        let commands = commands_list
            .into_iter()
            .map(|command| (command.name.clone(), command))
            .collect::<BTreeMap<_, _>>();

        let permission_groups_by_property = permission_groups
            .iter()
            .filter_map(|group| {
                group
                    .property_name
                    .as_ref()
                    .map(|property| (property.clone(), group.clone()))
            })
            .collect::<HashMap<_, _>>();

        let permission_catalog_by_name = permission_catalog
            .iter()
            .map(|entry| (entry.name.clone(), entry.clone()))
            .collect::<HashMap<_, _>>();

        Ok(Self {
            workspace_root,
            binary_manifest,
            commands,
            permission_groups,
            permission_groups_by_property,
            permission_mapping_groups,
            permission_catalog,
            permission_catalog_by_name,
            subsystems,
            baseline_profile,
            build_version,
        })
    }

    pub fn discover_workspace_root(start: impl AsRef<Path>) -> Result<PathBuf> {
        let start = start
            .as_ref()
            .canonicalize()
            .unwrap_or_else(|_| start.as_ref().to_path_buf());
        for candidate in start.ancestors() {
            let probe = candidate
                .join("data")
                .join("foundation")
                .join("query-baseline.json");
            if probe.exists() {
                return Ok(candidate.to_path_buf());
            }
        }
        bail!("workspace root with data/foundation/query-baseline.json not found")
    }

    pub fn baseline_command_names(&self) -> BTreeSet<String> {
        self.baseline_profile
            .essential_commands
            .iter()
            .map(|command| command.name.clone())
            .collect()
    }

    pub fn get_command(&self, name: &str) -> Option<&CommandSpec> {
        self.commands.get(name)
    }

    pub fn get_permission_group(&self, property_name: &str) -> Option<&PermissionGroupSpec> {
        self.permission_groups_by_property.get(property_name)
    }

    pub fn get_permission_catalog_entry(&self, name: &str) -> Option<&PermissionCatalogEntry> {
        self.permission_catalog_by_name.get(name)
    }
}

fn read_json<T>(path: &Path) -> Result<T>
where
    T: DeserializeOwned,
{
    let content = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    let content = strip_utf8_bom(&content);
    serde_json::from_slice(content).with_context(|| format!("failed to parse {}", path.display()))
}

fn read_build_version(path: &Path) -> Result<BuildVersion> {
    let content =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let mut build_name = String::from("BlackTeaSpeak");
    let mut build_version = String::from("unknown");
    let mut build_index = 0_i64;

    for line in content.lines() {
        let trimmed = line.trim();

        if let Some(value) = trimmed.strip_prefix("BlackTeaSpeak version ") {
            build_version = value.trim().to_string();
            continue;
        }

        if trimmed.starts_with('{') {
            let parsed = serde_json::from_str::<Value>(trimmed).with_context(|| {
                format!("failed to parse build version JSON from {}", path.display())
            })?;
            if let Some(value) = parsed.get("build_name").and_then(Value::as_str) {
                build_name = value.to_string();
            }
            if let Some(value) = parsed.get("build_version").and_then(Value::as_str) {
                build_version = value.to_string();
            }
            if let Some(value) = parsed.get("build_index").and_then(Value::as_i64) {
                build_index = value;
            }
        }
    }

    Ok(BuildVersion {
        build_name,
        build_version,
        build_index,
    })
}

fn strip_utf8_bom(content: &[u8]) -> &[u8] {
    if content.starts_with(&[0xEF, 0xBB, 0xBF]) {
        &content[3..]
    } else {
        content
    }
}
