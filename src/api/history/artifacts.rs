use crate::workflow::WorkflowArtifact;
use serde_json::Value;
use std::path::Path;

use super::{
    ApiResult,
    query::list_runs,
    storage,
    types::{ArtifactCatalog, ArtifactListOptions, RunArtifact},
};

struct ArtifactCollection<'a> {
    run_id: &'a str,
    stage_index: Option<usize>,
    artifacts: &'a mut Vec<RunArtifact>,
}

struct ArtifactLocation<'a> {
    node_index: Option<usize>,
    workflow_id: Option<&'a str>,
    node_id: Option<&'a str>,
    node_path: Option<&'a str>,
    depth: Option<usize>,
}

pub(super) fn list_artifacts(root: &Path) -> ApiResult<ArtifactCatalog> {
    list_artifacts_with_options(root, &ArtifactListOptions::default())
}

pub(super) fn list_artifacts_with_options(
    root: &Path,
    options: &ArtifactListOptions,
) -> ApiResult<ArtifactCatalog> {
    let mut artifacts = Vec::new();
    for run in list_runs(root)?.runs {
        let execution_path = run.run_dir.join("execution.json");
        if !execution_path.exists() {
            continue;
        }
        let execution = storage::read_json(&execution_path)?;
        collect_execution_artifacts(&run.run_id, &execution, &mut artifacts);
    }
    apply_artifact_filters(root, &mut artifacts, options)?;
    Ok(ArtifactCatalog { artifacts })
}

fn apply_artifact_filters(
    root: &Path,
    artifacts: &mut Vec<RunArtifact>,
    options: &ArtifactListOptions,
) -> ApiResult<()> {
    if let Some(run_id) = options.run_id.as_deref() {
        let resolved_run_id = if run_id == "last" {
            storage::resolve_run_id(root, "last")?
        } else {
            run_id.to_owned()
        };
        artifacts.retain(|artifact| artifact.run_id == resolved_run_id);
    }
    if let Some(workflow_id) = options.workflow_id.as_deref() {
        artifacts.retain(|artifact| artifact.workflow_id.as_deref() == Some(workflow_id));
    }
    if let Some(kind) = options.kind.as_deref() {
        artifacts.retain(|artifact| artifact.artifact.kind == kind);
    }
    if let Some(limit) = options.limit {
        artifacts.truncate(limit);
    }
    Ok(())
}

fn collect_execution_artifacts(run_id: &str, execution: &Value, artifacts: &mut Vec<RunArtifact>) {
    if execution.get("pipeline").and_then(Value::as_bool) == Some(true) {
        for (stage_index, stage) in execution
            .get("stages")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .enumerate()
        {
            collect_stage_artifacts(run_id, Some(stage_index), stage, artifacts);
        }
        return;
    }

    collect_stage_artifacts(run_id, Some(0), execution, artifacts)
}

fn collect_stage_artifacts(
    run_id: &str,
    stage_index: Option<usize>,
    execution: &Value,
    artifacts: &mut Vec<RunArtifact>,
) {
    let workflow_id = execution
        .get("workflow_id")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let mut collection = ArtifactCollection {
        run_id,
        stage_index,
        artifacts,
    };
    collect_artifact_array(
        &mut collection,
        ArtifactLocation {
            node_index: None,
            workflow_id: workflow_id.as_deref(),
            node_id: None,
            node_path: None,
            depth: None,
        },
        execution.get("artifacts"),
    );
    for (node_index, node) in execution
        .get("nodes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
    {
        collect_node_artifacts(
            &mut collection,
            workflow_id.as_deref(),
            node_index,
            node,
            "",
            0,
        );
    }
}

fn collect_node_artifacts(
    collection: &mut ArtifactCollection<'_>,
    parent_workflow_id: Option<&str>,
    node_index: usize,
    node: &Value,
    parent_path: &str,
    depth: usize,
) {
    let node_id = node
        .get("node_id")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let node_path = if parent_path.is_empty() {
        node_id.to_owned()
    } else {
        format!("{parent_path}/{node_id}")
    };
    let workflow_id = node
        .get("selected_workflow_id")
        .and_then(Value::as_str)
        .or_else(|| node.get("workflow_id").and_then(Value::as_str))
        .or(parent_workflow_id);
    collect_artifact_array(
        collection,
        ArtifactLocation {
            node_index: Some(node_index),
            workflow_id,
            node_id: Some(node_id),
            node_path: Some(&node_path),
            depth: Some(depth),
        },
        node.get("artifacts"),
    );
    for (child_index, child) in node
        .get("nodes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
    {
        collect_node_artifacts(
            collection,
            workflow_id,
            child_index,
            child,
            &node_path,
            depth + 1,
        );
    }
}

fn collect_artifact_array(
    collection: &mut ArtifactCollection<'_>,
    location: ArtifactLocation<'_>,
    value: Option<&Value>,
) {
    for artifact in value.and_then(Value::as_array).into_iter().flatten() {
        let Ok(artifact) = serde_json::from_value::<WorkflowArtifact>(artifact.clone()) else {
            continue;
        };
        let candidate = RunArtifact {
            run_id: collection.run_id.to_owned(),
            stage_index: collection.stage_index,
            node_index: location.node_index,
            workflow_id: location.workflow_id.map(str::to_owned),
            node_id: location.node_id.map(str::to_owned),
            node_path: location.node_path.map(str::to_owned),
            depth: location.depth,
            artifact,
        };
        push_deepest_artifact(collection.artifacts, candidate);
    }
}

fn push_deepest_artifact(artifacts: &mut Vec<RunArtifact>, candidate: RunArtifact) {
    let duplicate = artifacts.iter().position(|existing| {
        existing.run_id == candidate.run_id
            && existing.stage_index == candidate.stage_index
            && existing.artifact.id == candidate.artifact.id
            && existing.artifact.path == candidate.artifact.path
            && provenance_overlaps(
                existing.node_path.as_deref(),
                candidate.node_path.as_deref(),
            )
    });
    let Some(index) = duplicate else {
        artifacts.push(candidate);
        return;
    };
    let existing_depth = artifacts[index].depth.map_or(-1, |depth| depth as isize);
    let candidate_depth = candidate.depth.map_or(-1, |depth| depth as isize);
    if candidate_depth > existing_depth {
        artifacts[index] = candidate;
    }
}

fn provenance_overlaps(existing: Option<&str>, candidate: Option<&str>) -> bool {
    match (existing, candidate) {
        (None, _) | (_, None) => true,
        (Some(existing), Some(candidate)) => {
            existing == candidate
                || is_ancestor_path(existing, candidate)
                || is_ancestor_path(candidate, existing)
        }
    }
}

fn is_ancestor_path(ancestor: &str, descendant: &str) -> bool {
    descendant
        .strip_prefix(ancestor)
        .is_some_and(|suffix| suffix.starts_with('/'))
}
