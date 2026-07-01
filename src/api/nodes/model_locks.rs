use super::model_lock_read::{model_lock_status, read_model_lock};
use crate::workflow::{ModelRequirement, WorkflowSpec};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

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

pub(in crate::api) fn model_catalog(
    root: &Path,
    workflows: &BTreeMap<String, WorkflowSpec>,
    options: &ModelListOptions,
) -> ModelCatalog {
    let lock = read_model_lock(root);
    let mut models = Vec::new();
    let mut issues = Vec::new();
    for workflow in workflows.values() {
        if options
            .workflow_id
            .as_deref()
            .is_some_and(|workflow_id| workflow.id != workflow_id)
        {
            continue;
        }
        for requirement in &workflow.models {
            let lock = model_lock_status(&lock, &workflow.id, &requirement.id);
            let blocked = lock.status != ModelLockState::Available;
            match options.status {
                ModelStatusFilter::All => {}
                ModelStatusFilter::Available if blocked => continue,
                ModelStatusFilter::Blocked if !blocked => continue,
                ModelStatusFilter::Available | ModelStatusFilter::Blocked => {}
            }
            if lock.status != ModelLockState::Available {
                issues.push(format!(
                    "{}: model lock is {}",
                    lock.key,
                    lock.status.as_str()
                ));
            }
            models.push(NodeModelCard {
                workflow_id: workflow.id.clone(),
                workflow_name: workflow.name.clone(),
                category: workflow.category.clone(),
                requirement: requirement.clone(),
                bindings: model_bindings(workflow, &requirement.id),
                lock,
                sync_command: model_sync_command(&workflow.id),
                verify_command: model_verify_command(&workflow.id),
            });
        }
    }
    let total = models.len();
    let blocked_count = issues.len();
    ModelCatalog {
        total,
        available_count: total.saturating_sub(blocked_count),
        blocked_count,
        issues,
        models,
    }
}

fn model_sync_command(workflow_id: &str) -> Vec<String> {
    vec![
        "lfw".to_owned(),
        "sync".to_owned(),
        workflow_id.to_owned(),
        "--auto-model".to_owned(),
        "--apply".to_owned(),
    ]
}

fn model_verify_command(workflow_id: &str) -> Vec<String> {
    vec![
        "lfw".to_owned(),
        "sync".to_owned(),
        workflow_id.to_owned(),
        "--locked".to_owned(),
        "--apply".to_owned(),
    ]
}

pub(in crate::api) fn model_lock_fingerprints(
    root: &Path,
    workflows: &BTreeMap<String, WorkflowSpec>,
    execution: &serde_json::Value,
) -> Vec<ModelLockFingerprint> {
    let lock = read_model_lock(root);
    let mut fingerprints = Vec::new();
    let mut contexts = Vec::new();
    collect_model_contexts(execution, None, &mut contexts);
    for context in contexts {
        let Some(workflow) = workflows.get(&context.workflow_id) else {
            continue;
        };
        for requirement in &workflow.models {
            fingerprints.push(ModelLockFingerprint {
                stage_index: context.stage_index,
                workflow_id: workflow.id.clone(),
                node_id: context.node_id.clone(),
                requirement_id: requirement.id.clone(),
                lock: model_lock_status(&lock, &workflow.id, &requirement.id),
            });
        }
    }
    fingerprints
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct ModelContext {
    stage_index: Option<usize>,
    workflow_id: String,
    node_id: Option<String>,
}

fn collect_model_contexts(
    execution: &serde_json::Value,
    stage_index: Option<usize>,
    contexts: &mut Vec<ModelContext>,
) {
    if execution
        .get("pipeline")
        .and_then(serde_json::Value::as_bool)
        == Some(true)
    {
        for (index, stage) in execution
            .get("stages")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .enumerate()
        {
            collect_model_contexts(stage, Some(index), contexts);
        }
        return;
    }

    if let Some(workflow_id) = execution
        .get("workflow_id")
        .and_then(serde_json::Value::as_str)
    {
        contexts.push(ModelContext {
            stage_index,
            workflow_id: workflow_id.to_owned(),
            node_id: None,
        });
    }

    for node in execution
        .get("nodes")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
    {
        let workflow_id = node
            .get("selected_workflow_id")
            .and_then(serde_json::Value::as_str)
            .or_else(|| node.get("workflow_id").and_then(serde_json::Value::as_str));
        let Some(workflow_id) = workflow_id else {
            continue;
        };
        contexts.push(ModelContext {
            stage_index,
            workflow_id: workflow_id.to_owned(),
            node_id: node
                .get("node_id")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned),
        });
    }
}

fn model_bindings(workflow: &WorkflowSpec, requirement_id: &str) -> Vec<NodeModelBinding> {
    let mut bindings = Vec::new();
    for port in &workflow.inputs {
        if port.model_requirement.as_deref() == Some(requirement_id) {
            bindings.push(NodeModelBinding {
                direction: PortDirection::Input,
                port: port.name.clone(),
            });
        }
    }
    for port in &workflow.outputs {
        if port.model_requirement.as_deref() == Some(requirement_id) {
            bindings.push(NodeModelBinding {
                direction: PortDirection::Output,
                port: port.name.clone(),
            });
        }
    }
    bindings
}
