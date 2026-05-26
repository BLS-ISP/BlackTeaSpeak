use super::*;
use super::*;
use std::fs;

use crate::models::PermissionGroupSpec;

pub(crate) const ERROR_INSUFFICIENT_PERMISSIONS: u32 = 0xA08;

pub(crate) const TEAWEB_PERMISSION_GROUP_ENDS: &[u32] = &[
    0, 7, 13, 18, 21, 21, 34, 48, 82, 82, 89, 113, 133, 140, 157, 157, 173, 175, 199, 201, 201,
    275, 303, 323, 342, 360,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PermissionActorContext {
    pub(crate) server_id: u32,
    pub(crate) channel_id: u32,
    pub(crate) client_database_id: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PermissionAssignment {
    pub(crate) value: i64,
    pub(crate) negated: bool,
    pub(crate) skipped: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct ParsedPermissionAssignment {
    pub(crate) name: String,
    pub(crate) assignment: PermissionAssignment,
}

#[derive(Debug, Clone)]
pub(crate) struct WebPermissionLayout {
    pub(crate) ids_by_name: BTreeMap<String, u32>,
    pub(crate) group_markers: Vec<u32>,
}

