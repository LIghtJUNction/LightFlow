use crate::workflow::ModelRequirement;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

mod catalog;
mod fingerprints;
pub(in crate::api) use catalog::model_catalog;
pub(in crate::api) use fingerprints::model_lock_fingerprints;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ModelCatalog {
    pub total: usize,
    pub available_count: usize,
    pub blocked_count: usize,
    pub issues: Vec<String>,
    pub models: Vec<NodeModelCard>,
}

#[derive(Debug, Clone, Default, Eq, PartialEq)]
pub struct ModelListOptions {
    pub workflow_id: Option<String>,
    pub status: ModelStatusFilter,
}

#[derive(Debug, Clone, Copy, Default, Eq, PartialEq)]
pub enum ModelStatusFilter {
    #[default]
    All,
    Available,
    Blocked,
}

impl ModelStatusFilter {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "all" => Some(Self::All),
            "available" | "ready" => Some(Self::Available),
            "blocked" | "missing" | "not-ready" => Some(Self::Blocked),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NodeModelCard {
    pub workflow_id: String,
    pub workflow_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    pub requirement: ModelRequirement,
    pub bindings: Vec<NodeModelBinding>,
    pub lock: ModelLockStatus,
    pub sync_command: Vec<String>,
    pub verify_command: Vec<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct NodeModelBinding {
    pub direction: PortDirection,
    pub port: String,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PortDirection {
    Input,
    Output,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelLockStatus {
    pub status: ModelLockState,
    pub key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hash_algorithm: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot_revision: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub local_paths: Vec<PathBuf>,
    pub missing_paths: Vec<PathBuf>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelLockState {
    MissingLock,
    MissingEntry,
    MissingPath,
    Available,
    InvalidLock,
}

impl ModelLockState {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::MissingLock => "missing_lock",
            Self::MissingEntry => "missing_entry",
            Self::MissingPath => "missing_path",
            Self::Available => "available",
            Self::InvalidLock => "invalid_lock",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ModelLockFingerprint {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stage_index: Option<usize>,
    pub workflow_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_id: Option<String>,
    pub requirement_id: String,
    pub lock: ModelLockStatus,
}
