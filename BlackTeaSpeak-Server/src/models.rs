use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::Deserialize;
use std::collections::HashSet;

pub const WHISPER_TARGET_CLIENT: u32 = 0;
pub const WHISPER_TARGET_CHANNEL: u32 = 1;
pub const WHISPER_TARGET_SERVER_GROUP: u32 = 2;
pub const WHISPER_TARGET_SELF: u32 = 4;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WhisperTargetSelection {
    pub client_ids: HashSet<u64>,
    pub channel_ids: HashSet<u32>,
    pub server_group_ids: HashSet<u32>,
    pub echo_self: bool,
}

impl WhisperTargetSelection {
    pub fn is_empty(&self) -> bool {
        self.client_ids.is_empty() && self.channel_ids.is_empty() && self.server_group_ids.is_empty() && !self.echo_self
    }
}

#[derive(Debug, Clone)]
pub struct BuildVersion {
    pub build_name: String,
    pub build_version: String,
    pub build_index: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BinaryManifest {
    #[serde(rename = "runtimeDependencies")]
    pub runtime_dependencies: Vec<String>,
    pub binary: BinaryIdentity,
    #[serde(rename = "commandOffsets")]
    pub command_offsets: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BinaryIdentity {
    pub path: PathBuf,
    pub sha256: String,
    #[serde(rename = "sizeBytes")]
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CommandSpec {
    pub name: String,
    pub category: String,
    #[serde(rename = "docPath")]
    pub doc_path: String,
    #[serde(rename = "binaryOffset")]
    pub binary_offset: Option<String>,
    pub usage: Vec<String>,
    pub permissions: Vec<String>,
    pub description: String,
    pub result: Option<String>,
    pub notes: Option<String>,
    pub examples: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PermissionDefinition {
    pub name: String,
    pub value: i64,
    #[serde(rename = "grantedBy")]
    pub granted_by: i64,
    pub skipped: i64,
    pub negated: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PermissionGroupSpec {
    pub name: String,
    pub target: i64,
    #[serde(rename = "targetName")]
    pub target_name: String,
    #[serde(rename = "property")]
    pub property_name: Option<String>,
    pub permissions: Vec<PermissionDefinition>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PermissionMappingGroup {
    #[serde(rename = "groupId")]
    pub group_id: i64,
    #[serde(rename = "groupName")]
    pub group_name: String,
    pub mappings: Vec<PermissionMappingEntry>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PermissionMappingEntry {
    #[serde(rename = "originalName")]
    pub original_name: String,
    #[serde(rename = "mappedValue")]
    pub mapped_value: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PermissionCatalogEntry {
    pub name: String,
    pub id: u32,
    pub description: String,
    #[serde(rename = "idSource")]
    pub id_source: String,
    #[serde(rename = "descriptionSource")]
    pub description_source: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SourcePathAnchor {
    pub path: String,
    #[serde(rename = "fileName")]
    pub file_name: String,
    #[serde(rename = "unitName")]
    pub unit_name: String,
    pub offset: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BaselineProfile {
    pub profile: String,
    pub goal: String,
    #[serde(rename = "essentialCommands")]
    pub essential_commands: Vec<CommandSpec>,
    #[serde(rename = "essentialPermissions")]
    pub essential_permissions: Vec<String>,
    #[serde(rename = "essentialSourcePaths")]
    pub essential_source_paths: Vec<SourcePathAnchor>,
    #[serde(rename = "renameSeeds")]
    pub rename_seeds: Vec<String>,
}
