use crate::api::util::validate_id_segment;
use serde_json::Value;
use std::fs;
use std::path::Path;

use super::storage;
use super::types::{
    RemovedRun, ReplayStages, RunCatalog, RunEvents, RunListOptions, RunSummary, RunTrace,
};
use crate::api::{ApiError, ApiResult};

pub(super) fn replay_stages(root: &Path, selector: &str) -> ApiResult<ReplayStages> {
    let run_id = storage::resolve_run_id(root, selector)?;
    let path = storage::run_dir(root, &run_id).join("manifest.json");
    let manifest: super::types::RecordedRunManifest = serde_json::from_slice(&fs::read(path)?)
        .map_err(|error| ApiError::InvalidRequest(format!("invalid run manifest JSON: {error}")))?;
    if manifest.stages.is_empty() {
        return Err(ApiError::InvalidRequest(format!(
            "run {run_id} has no replayable stages"
        )));
    }
    Ok(ReplayStages {
        stages: manifest.stages,
        stage_inputs_resolved: manifest.stage_input_resolution == "resolved",
    })
}

pub(super) fn list_runs(root: &Path) -> ApiResult<RunCatalog> {
    list_runs_with_options(root, &RunListOptions::default())
}

pub(super) fn list_runs_with_options(
    root: &Path,
    options: &RunListOptions,
) -> ApiResult<RunCatalog> {
    let runs_root = storage::runs_root(root);
    let last = storage::read_last(&runs_root);
    let mut runs = Vec::new();
    let mut issues = Vec::new();
    if runs_root.exists() {
        for entry in fs::read_dir(&runs_root)? {
            let entry = entry?;
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let manifest_path = path.join("manifest.json");
            if !manifest_path.exists() {
                continue;
            }
            let manifest = match storage::read_json(&manifest_path) {
                Ok(manifest) => manifest,
                Err(error) => {
                    issues.push(format!(
                        "{}: could not read run manifest: {error}",
                        path.display()
                    ));
                    continue;
                }
            };
            let Some(run_id) = manifest.get("run_id").and_then(Value::as_str) else {
                issues.push(format!(
                    "{}: run manifest is missing run_id",
                    manifest_path.display()
                ));
                continue;
            };
            let workflow_ids = workflow_ids(&manifest);
            let started_at_ms = manifest
                .get("started_at_ms")
                .and_then(Value::as_u64)
                .map(u128::from)
                .unwrap_or_default();
            let completed_at_ms = manifest
                .get("completed_at_ms")
                .and_then(Value::as_u64)
                .map(u128::from)
                .unwrap_or_default();
            let status = storage::run_status(&manifest, &path)?;
            runs.push(RunSummary {
                run_id: run_id.to_owned(),
                status,
                started_at_ms,
                completed_at_ms,
                duration_ms: completed_at_ms.saturating_sub(started_at_ms),
                surface: storage::first_run_surface(&path)?,
                workflow_id: workflow_ids.first().cloned(),
                workflow_ids,
                stages: manifest
                    .get("stages")
                    .and_then(Value::as_array)
                    .map_or(0, Vec::len),
                run_dir: path,
            });
        }
    }
    runs.sort_by(|a, b| {
        b.completed_at_ms
            .cmp(&a.completed_at_ms)
            .then_with(|| b.run_id.cmp(&a.run_id))
    });
    apply_run_filters(&mut runs, options);
    let total = runs.len();
    let completed_count = runs.iter().filter(|run| run.status == "completed").count();
    let failed_count = runs.iter().filter(|run| run.status == "failed").count();
    let unknown_run_ids = runs
        .iter()
        .filter(|run| run.status != "completed" && run.status != "failed")
        .map(|run| run.run_id.clone())
        .collect::<Vec<_>>();
    let unknown_count = unknown_run_ids.len();
    Ok(RunCatalog {
        last,
        total,
        completed_count,
        failed_count,
        unknown_count,
        unknown_run_ids,
        issues,
        runs,
    })
}

fn apply_run_filters(runs: &mut Vec<RunSummary>, options: &RunListOptions) {
    if let Some(workflow_id) = options.workflow_id.as_deref() {
        runs.retain(|run| run.workflow_ids.iter().any(|id| id == workflow_id));
    }
    if let Some(status) = options.status.as_deref() {
        runs.retain(|run| run.status == status);
    }
    if let Some(limit) = options.limit {
        runs.truncate(limit);
    }
}

pub(super) fn get_run(root: &Path, selector: &str) -> ApiResult<RunTrace> {
    let run_id = storage::resolve_run_id(root, selector)?;
    let run_dir = storage::run_dir(root, &run_id);
    Ok(RunTrace {
        run_id,
        run_dir: run_dir.clone(),
        manifest: storage::read_json(run_dir.join("manifest.json"))?,
        execution: storage::read_json(run_dir.join("execution.json"))?,
        events: storage::read_events(&run_dir)?,
    })
}

pub(super) fn get_run_events(root: &Path, selector: &str) -> ApiResult<RunEvents> {
    let run_id = storage::resolve_run_id(root, selector)?;
    let run_dir = storage::run_dir(root, &run_id);
    Ok(RunEvents {
        run_id,
        events: storage::read_events(&run_dir)?,
    })
}

pub(super) fn remove_run(root: &Path, selector: &str) -> ApiResult<RemovedRun> {
    let run_id = if selector == "last" {
        storage::resolve_run_id(root, selector)?
    } else {
        validate_id_segment(selector, "run id")?;
        selector.to_owned()
    };
    let path = storage::run_dir(root, &run_id);
    let removed = if path.exists() {
        fs::remove_dir_all(&path)?;
        true
    } else {
        false
    };
    let last_path = storage::runs_root(root).join("last");
    if fs::read_to_string(&last_path)
        .ok()
        .is_some_and(|last| last.trim() == run_id)
    {
        let _ = fs::remove_file(&last_path);
    }
    Ok(RemovedRun {
        removed,
        run_id,
        run_dir: path,
    })
}

fn workflow_ids(manifest: &Value) -> Vec<String> {
    manifest
        .get("stages")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|stage| stage.get("workflow_id"))
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect()
}
