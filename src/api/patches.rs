use super::util::{node_inputs, node_outputs};
use super::{ApiError, ApiResult};
use crate::workflow::{PortSpec, WorkflowNode, WorkflowPatch, WorkflowSpec};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

const PATCHES_DIR: &str = ".lightflow/patches";

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PatchCatalog {
    pub patches: Vec<PatchSummary>,
    pub root: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PatchSummary {
    pub name: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RegisteredPatch {
    pub name: String,
    pub path: PathBuf,
    pub patch: WorkflowPatch,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SavedPatch {
    pub saved: bool,
    pub name: String,
    pub path: PathBuf,
    pub patch: WorkflowPatch,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RemovedPatch {
    pub removed: bool,
    pub name: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PatchValidation {
    pub valid: bool,
    pub issues: Vec<String>,
    pub patch: WorkflowPatch,
}

pub(super) fn list_patches(root: &Path) -> ApiResult<PatchCatalog> {
    let patches_root = patches_root(root);
    let mut patches = Vec::new();
    if patches_root.exists() {
        for entry in fs::read_dir(&patches_root)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
                continue;
            };
            patches.push(PatchSummary {
                name: stem.to_owned(),
                path,
            });
        }
    }
    patches.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(PatchCatalog {
        patches,
        root: patches_root,
    })
}

pub(super) fn get_patch(root: &Path, name: &str) -> ApiResult<RegisteredPatch> {
    let name = normalized_patch_name(name)?;
    let path = patch_path(root, &name)?;
    let patch = serde_json::from_slice(&fs::read(&path)?)
        .map_err(|error| ApiError::InvalidRequest(format!("invalid patch JSON: {error}")))?;
    Ok(RegisteredPatch { name, path, patch })
}

pub(super) fn save_patch(root: &Path, name: &str, patch: &WorkflowPatch) -> ApiResult<SavedPatch> {
    let name = normalized_patch_name(name)?;
    let path = patch_path(root, &name)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(
        &path,
        format!(
            "{}\n",
            serde_json::to_string_pretty(patch).map_err(|error| {
                ApiError::InvalidRequest(format!("invalid patch JSON: {error}"))
            })?
        ),
    )?;
    Ok(SavedPatch {
        saved: true,
        name,
        path,
        patch: patch.clone(),
    })
}

pub(super) fn remove_patch(root: &Path, name: &str) -> ApiResult<RemovedPatch> {
    let name = normalized_patch_name(name)?;
    let path = patch_path(root, &name)?;
    let removed = if path.exists() {
        fs::remove_file(&path)?;
        true
    } else {
        false
    };
    Ok(RemovedPatch {
        removed,
        name,
        path,
    })
}

pub(super) fn validate_patch(
    patch: WorkflowPatch,
    workflows: Option<&BTreeMap<String, WorkflowSpec>>,
) -> PatchValidation {
    let mut issues = Vec::new();
    if patch.nodes.is_empty() {
        issues.push("patch.nodes is empty".to_owned());
    }

    if let Some(workflows) = workflows {
        let workflow_ids = workflows.keys().cloned().collect::<BTreeSet<_>>();
        let node_ids = workflows
            .values()
            .flat_map(|workflow| workflow.nodes.iter().map(|node| node.id.clone()))
            .collect::<BTreeSet<_>>();
        for (node_id, node_patch) in &patch.nodes {
            if !node_ids.contains(node_id) {
                issues.push(format!(
                    "patch node {node_id} does not match any available workflow node"
                ));
            }
            if node_patch.enable && node_patch.disable {
                issues.push(format!(
                    "patch node {node_id} cannot set both enable and disable"
                ));
            }
            if let Some(replacement) = &node_patch.replace_with
                && !workflow_ids.contains(replacement)
            {
                issues.push(format!(
                    "patch node {node_id} replacement workflow {replacement} is not available"
                ));
            }
            if let Some(fallback) = &node_patch.fallback_workflow_id
                && !workflow_ids.contains(fallback)
            {
                issues.push(format!(
                    "patch node {node_id} fallback workflow {fallback} is not available"
                ));
            }
            if node_patch.retry == Some(0) {
                issues.push(format!(
                    "patch node {node_id} retry must be greater than zero"
                ));
            }
        }
    }

    PatchValidation {
        valid: issues.is_empty(),
        issues,
        patch,
    }
}

pub(super) fn validate_patch_for_workflow(
    patch: &WorkflowPatch,
    workflow: &WorkflowSpec,
    workflows: &BTreeMap<String, WorkflowSpec>,
) -> PatchValidation {
    let mut issues = Vec::new();
    if patch.nodes.is_empty() {
        issues.push("patch.nodes is empty".to_owned());
    }

    let workflow_ids = workflows.keys().cloned().collect::<BTreeSet<_>>();
    let node_ids = workflow
        .nodes
        .iter()
        .map(|node| node.id.clone())
        .collect::<BTreeSet<_>>();
    let nodes_by_id = workflow
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect::<BTreeMap<_, _>>();
    for (node_id, node_patch) in &patch.nodes {
        if !node_ids.contains(node_id) {
            issues.push(format!(
                "patch node {node_id} does not match any node in workflow {}",
                workflow.id
            ));
        }
        if node_patch.enable && node_patch.disable {
            issues.push(format!(
                "patch node {node_id} cannot set both enable and disable"
            ));
        }
        if let Some(replacement) = &node_patch.replace_with
            && !workflow_ids.contains(replacement)
        {
            issues.push(format!(
                "patch node {node_id} replacement workflow {replacement} is not available"
            ));
        }
        if let (Some(node), Some(replacement)) =
            (nodes_by_id.get(node_id.as_str()), &node_patch.replace_with)
        {
            validate_candidate_contract(
                node_id,
                "replacement",
                replacement,
                node,
                workflows,
                &mut issues,
            );
        }
        if let Some(fallback) = &node_patch.fallback_workflow_id
            && !workflow_ids.contains(fallback)
        {
            issues.push(format!(
                "patch node {node_id} fallback workflow {fallback} is not available"
            ));
        }
        if let (Some(node), Some(fallback)) = (
            nodes_by_id.get(node_id.as_str()),
            &node_patch.fallback_workflow_id,
        ) {
            validate_candidate_contract(
                node_id,
                "fallback",
                fallback,
                node,
                workflows,
                &mut issues,
            );
        }
        if node_patch.retry == Some(0) {
            issues.push(format!(
                "patch node {node_id} retry must be greater than zero"
            ));
        }
    }

    PatchValidation {
        valid: issues.is_empty(),
        issues,
        patch: patch.clone(),
    }
}

fn validate_candidate_contract(
    node_id: &str,
    role: &str,
    candidate_id: &str,
    node: &WorkflowNode,
    workflows: &BTreeMap<String, WorkflowSpec>,
    issues: &mut Vec<String>,
) {
    let Some(candidate) = workflows.get(candidate_id) else {
        return;
    };
    push_missing_ports(
        node_id,
        role,
        candidate_id,
        "input",
        &node_inputs(node, workflows),
        &candidate.inputs,
        issues,
    );
    push_unsatisfied_extra_required_inputs(
        node_id,
        role,
        candidate_id,
        &node_inputs(node, workflows),
        &candidate.inputs,
        issues,
    );
    push_missing_ports(
        node_id,
        role,
        candidate_id,
        "output",
        &node_outputs(node, workflows),
        &candidate.outputs,
        issues,
    );
}

fn push_missing_ports(
    node_id: &str,
    role: &str,
    candidate_id: &str,
    direction: &str,
    required: &[PortSpec],
    available: &[PortSpec],
    issues: &mut Vec<String>,
) {
    for port in required {
        match available.iter().find(|candidate| candidate.name == port.name) {
            Some(candidate) if candidate.ty == port.ty => {}
            Some(candidate) => issues.push(format!(
                "patch node {node_id} {role} workflow {candidate_id} {direction} port {} has type {}, expected {}",
                port.name, candidate.ty, port.ty
            )),
            None => issues.push(format!(
                "patch node {node_id} {role} workflow {candidate_id} is missing {direction} port {}",
                port.name
            )),
        }
    }
}

fn push_unsatisfied_extra_required_inputs(
    node_id: &str,
    role: &str,
    candidate_id: &str,
    original_inputs: &[PortSpec],
    candidate_inputs: &[PortSpec],
    issues: &mut Vec<String>,
) {
    let original_names = original_inputs
        .iter()
        .map(|port| port.name.as_str())
        .collect::<BTreeSet<_>>();
    for port in candidate_inputs {
        if original_names.contains(port.name.as_str()) {
            continue;
        }
        if port.required == Some(true) && port.default.is_none() {
            issues.push(format!(
                "patch node {node_id} {role} workflow {candidate_id} has unsatisfied required input port {}",
                port.name
            ));
        }
    }
}

fn patch_path(root: &Path, name: &str) -> ApiResult<PathBuf> {
    Ok(patches_root(root).join(format!("{name}.json")))
}

fn patches_root(root: &Path) -> PathBuf {
    root.join(PATCHES_DIR)
}

fn normalized_patch_name(name: &str) -> ApiResult<String> {
    let name = name.strip_suffix(".json").unwrap_or(name);
    if name.is_empty()
        || name == "."
        || name == ".."
        || name.contains('/')
        || name.contains('\\')
        || name.chars().any(char::is_whitespace)
    {
        return Err(ApiError::InvalidRequest(
            "patch name must be a single non-empty file name".to_owned(),
        ));
    }
    Ok(name.to_owned())
}
