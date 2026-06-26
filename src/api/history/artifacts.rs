use crate::workflow::WorkflowArtifact;
use serde_json::Value;
use std::path::Path;

use super::{
    ApiResult,
    query::list_runs,
    storage,
    types::{ArtifactCatalog, ArtifactListOptions, RunArtifact},
};

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
    collect_artifact_array(
        run_id,
        stage_index,
        None,
        workflow_id.as_deref(),
        None,
        execution.get("artifacts"),
        artifacts,
    );
    for (node_index, node) in execution
        .get("nodes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
    {
        let node_id = node.get("node_id").and_then(Value::as_str);
        collect_artifact_array(
            run_id,
            stage_index,
            Some(node_index),
            workflow_id.as_deref(),
            node_id,
            node.get("artifacts"),
            artifacts,
        );
    }
}

fn collect_artifact_array(
    run_id: &str,
    stage_index: Option<usize>,
    node_index: Option<usize>,
    workflow_id: Option<&str>,
    node_id: Option<&str>,
    value: Option<&Value>,
    artifacts: &mut Vec<RunArtifact>,
) {
    for artifact in value.and_then(Value::as_array).into_iter().flatten() {
        let Ok(artifact) = serde_json::from_value::<WorkflowArtifact>(artifact.clone()) else {
            continue;
        };
        artifacts.push(RunArtifact {
            run_id: run_id.to_owned(),
            stage_index,
            node_index,
            workflow_id: workflow_id.map(str::to_owned),
            node_id: node_id.map(str::to_owned),
            artifact,
        });
    }
}
