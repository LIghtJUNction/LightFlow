use super::{ApiError, ApiResult};
use crate::workflow::{PortSpec, WorkflowCondition, WorkflowNodeKind, WorkflowSpec};
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

pub(super) fn node_inputs(
    node: &crate::workflow::WorkflowNode,
    workflows: &BTreeMap<String, WorkflowSpec>,
) -> Vec<PortSpec> {
    match node.kind {
        WorkflowNodeKind::Workflow => workflows
            .get(&node.workflow_id)
            .map(|workflow| workflow.inputs.clone())
            .unwrap_or_default(),
        WorkflowNodeKind::If => {
            let mut ports = Vec::new();
            if let Some(WorkflowCondition::InputEquals { input, .. }) = &node.condition {
                push_unique_port(&mut ports, PortSpec::new(input.clone(), "json"));
            }
            for workflow_id in branch_workflow_ids(node) {
                if let Some(workflow) = workflows.get(workflow_id) {
                    for port in &workflow.inputs {
                        push_unique_port(&mut ports, port.clone());
                    }
                }
            }
            ports
        }
    }
}

pub(super) fn node_outputs(
    node: &crate::workflow::WorkflowNode,
    workflows: &BTreeMap<String, WorkflowSpec>,
) -> Vec<PortSpec> {
    match node.kind {
        WorkflowNodeKind::Workflow => workflows
            .get(&node.workflow_id)
            .map(|workflow| workflow.outputs.clone())
            .unwrap_or_default(),
        WorkflowNodeKind::If => {
            let mut ports = Vec::new();
            for workflow_id in branch_workflow_ids(node) {
                if let Some(workflow) = workflows.get(workflow_id) {
                    for port in &workflow.outputs {
                        push_unique_port(&mut ports, port.clone());
                    }
                }
            }
            ports
        }
    }
}

pub(super) fn referenced_workflow_ids(node: &crate::workflow::WorkflowNode) -> Vec<&str> {
    match node.kind {
        WorkflowNodeKind::Workflow => vec![node.workflow_id.as_str()],
        WorkflowNodeKind::If => branch_workflow_ids(node),
    }
}

fn branch_workflow_ids(node: &crate::workflow::WorkflowNode) -> Vec<&str> {
    [
        node.then_workflow_id.as_deref(),
        node.else_workflow_id.as_deref(),
    ]
    .into_iter()
    .flatten()
    .collect()
}

fn push_unique_port(ports: &mut Vec<PortSpec>, port: PortSpec) {
    if !ports.iter().any(|existing| existing.name == port.name) {
        ports.push(port);
    }
}

pub(super) fn push_id_issue(issues: &mut Vec<String>, value: &str, label: &str) {
    if let Err(error) = validate_id_segment(value, label) {
        issues.push(error.to_string());
    }
}

pub(super) fn push_duplicate_port_issues(
    issues: &mut Vec<String>,
    direction: &str,
    owner_id: &str,
    ports: &[PortSpec],
) {
    let mut names = BTreeSet::new();
    for port in ports {
        if port.name.trim().is_empty() {
            issues.push(format!("{owner_id} has an empty {direction} port name"));
        }
        if port.ty.trim().is_empty() {
            issues.push(format!("{owner_id} port {} has an empty type", port.name));
        }
        if !names.insert(port.name.as_str()) {
            issues.push(format!(
                "{owner_id} has duplicate {direction} port {}",
                port.name
            ));
        }
    }
}

pub(super) fn validate_id_segment(value: &str, label: &str) -> ApiResult<()> {
    if value.is_empty()
        || value == "."
        || value == ".."
        || value.contains('/')
        || value.contains('\\')
    {
        return Err(ApiError::InvalidRequest(format!(
            "invalid {label} path segment: {value}"
        )));
    }
    Ok(())
}

pub(super) fn path_file_name(path: &Path, label: &str) -> ApiResult<String> {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(str::to_owned)
        .ok_or_else(|| ApiError::InvalidRequest(format!("{label} path has no file name: {path:?}")))
}

pub(super) fn workflow_crate_dir_name(workflow_id: &str) -> String {
    workflow_id
        .strip_prefix("lightflow.")
        .unwrap_or(workflow_id)
        .replace('.', "_")
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(super) enum XdgUserDirectory {
    Pictures,
    Videos,
    Music,
}

impl XdgUserDirectory {
    fn env_key(self) -> &'static str {
        match self {
            Self::Pictures => "XDG_PICTURES_DIR",
            Self::Videos => "XDG_VIDEOS_DIR",
            Self::Music => "XDG_MUSIC_DIR",
        }
    }

    fn fallback_name(self) -> &'static str {
        match self {
            Self::Pictures => "Pictures",
            Self::Videos => "Videos",
            Self::Music => "Music",
        }
    }
}

pub(super) fn lightflow_xdg_user_dir(root: &Path, directory: XdgUserDirectory) -> PathBuf {
    xdg_user_dir(directory)
        .unwrap_or_else(|| root.join(directory.fallback_name()))
        .join("lightflow")
}

fn xdg_user_dir(directory: XdgUserDirectory) -> Option<PathBuf> {
    if let Some(path) = env::var_os(directory.env_key())
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
    {
        return Some(path);
    }

    let home = env::var_os("HOME").map(PathBuf::from)?;
    let config_home = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".config"));
    if let Ok(source) = fs::read_to_string(config_home.join("user-dirs.dirs"))
        && let Some(path) = parse_xdg_user_dir(&source, directory.env_key(), &home)
    {
        return Some(path);
    }
    Some(home.join(directory.fallback_name()))
}

fn parse_xdg_user_dir(source: &str, key: &str, home: &Path) -> Option<PathBuf> {
    for line in source.lines().map(str::trim) {
        if line.starts_with('#') {
            continue;
        }
        let Some(value) = line
            .strip_prefix(key)
            .and_then(|line| line.strip_prefix('='))
        else {
            continue;
        };
        let value = value.trim().trim_matches('"');
        if value.is_empty() {
            return None;
        }
        if let Some(suffix) = value.strip_prefix("$HOME/") {
            return Some(home.join(suffix));
        }
        if value == "$HOME" {
            return Some(home.to_path_buf());
        }
        if let Some(suffix) = value.strip_prefix("${HOME}/") {
            return Some(home.join(suffix));
        }
        if value == "${HOME}" {
            return Some(home.to_path_buf());
        }
        return Some(PathBuf::from(value));
    }
    None
}
