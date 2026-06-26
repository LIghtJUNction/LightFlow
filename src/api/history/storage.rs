use crate::api::util::validate_id_segment;
use crate::api::{ApiError, ApiResult};
use serde::Serialize;
use serde_json::{self, Value};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub(super) const RUNS_DIR: &str = ".lightflow/runs";

pub(super) fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

pub(super) fn unique_run_id(root: &Path, now: u128) -> ApiResult<String> {
    fs::create_dir_all(runs_root(root))?;
    for suffix in 0..1000 {
        let run_id = if suffix == 0 {
            format!("run-{now}")
        } else {
            format!("run-{now}-{suffix}")
        };
        if !run_dir(root, &run_id).exists() {
            return Ok(run_id);
        }
    }
    Err(ApiError::InvalidRequest(
        "could not allocate unique run id".to_owned(),
    ))
}

pub(super) fn resolve_run_id(root: &Path, selector: &str) -> ApiResult<String> {
    if selector == "last" {
        let run_id =
            read_last(&runs_root(root)).ok_or_else(|| ApiError::NotFound("last run".to_owned()))?;
        validate_id_segment(&run_id, "run id")?;
        Ok(run_id)
    } else {
        let run_id = selector.to_owned();
        validate_id_segment(&run_id, "run id")?;
        let manifest = run_dir(root, &run_id).join("manifest.json");
        if manifest.exists() {
            Ok(run_id)
        } else {
            Err(ApiError::NotFound(format!("run {selector}")))
        }
    }
}

pub(super) fn read_last(root: &Path) -> Option<String> {
    fs::read_to_string(root.join("last"))
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

pub(super) fn run_status(manifest: &serde_json::Value, run_dir: &Path) -> ApiResult<String> {
    if let Some(status) = manifest.get("status").and_then(Value::as_str) {
        return Ok(status.to_owned());
    }
    if let Some(status) = infer_status_from_events(run_dir)? {
        return Ok(status);
    }
    if let Some(status) = infer_status_from_execution(run_dir)? {
        return Ok(status);
    }
    Ok("unknown".to_owned())
}

pub(super) fn infer_status_from_events(run_dir: &Path) -> ApiResult<Option<String>> {
    let path = run_dir.join("events.jsonl");
    if !path.exists() {
        return Ok(None);
    }
    let source = fs::read_to_string(path)?;
    let mut inferred = None;
    for line in event_lines(&source) {
        let Ok(event) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        match event.get("event").and_then(Value::as_str) {
            Some("run_failed") => inferred = Some("failed".to_owned()),
            Some("run_finished") => inferred = Some("completed".to_owned()),
            _ => {}
        }
    }
    Ok(inferred)
}

pub(super) fn infer_status_from_execution(run_dir: &Path) -> ApiResult<Option<String>> {
    let path = run_dir.join("execution.json");
    if !path.exists() {
        return Ok(None);
    }
    let execution = read_json(path)?;
    Ok(execution
        .get("status")
        .and_then(Value::as_str)
        .filter(|status| *status == "completed" || *status == "failed")
        .map(str::to_owned))
}

pub(super) fn first_run_surface(run_dir: &Path) -> ApiResult<Option<String>> {
    let path = run_dir.join("events.jsonl");
    if !path.exists() {
        return Ok(None);
    }
    let source = fs::read_to_string(path)?;
    for line in event_lines(&source) {
        let Ok(event) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if event.get("event").and_then(Value::as_str) == Some("run_started") {
            return Ok(event
                .get("surface")
                .and_then(Value::as_str)
                .map(str::to_owned));
        }
    }
    Ok(None)
}

pub(super) fn write_run_manifest(
    run_dir: &Path,
    run_id: &str,
    status: &str,
    stages: &[super::types::RunStageRecord],
    started_at_ms: u128,
    completed_at_ms: u128,
) -> ApiResult<()> {
    write_json_pretty(
        &run_dir.join("manifest.json"),
        &super::types::RecordedRunManifest {
            kind: "workflow_run".to_owned(),
            run_id: run_id.to_owned(),
            status: status.to_owned(),
            stage_input_resolution: "resolved".to_owned(),
            started_at_ms,
            completed_at_ms,
            stages: stages.to_vec(),
        },
    )
}

pub(super) fn write_json_pretty(path: &Path, value: &impl Serialize) -> ApiResult<()> {
    write_text(
        path,
        &format!(
            "{}\n",
            serde_json::to_string_pretty(value).map_err(|error| {
                ApiError::InvalidRequest(format!("invalid run JSON: {error}"))
            })?
        ),
    )
}

pub(super) fn write_text(path: &Path, value: &str) -> ApiResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, value)?;
    Ok(())
}

pub(super) fn append_event(run_dir: &Path, value: Value) -> ApiResult<()> {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(run_dir.join("events.jsonl"))?;
    serde_json::to_writer(&mut file, &value)
        .map_err(|error| ApiError::InvalidRequest(format!("invalid run event JSON: {error}")))?;
    file.write_all(b"\n")?;
    Ok(())
}

pub(super) fn read_events(run_dir: &Path) -> ApiResult<Vec<Value>> {
    let path = run_dir.join("events.jsonl");
    if !path.exists() {
        return Ok(Vec::new());
    }
    let source = fs::read_to_string(path)?;
    let mut events = Vec::new();
    for line in event_lines(&source) {
        events.push(serde_json::from_str(line).map_err(|error| {
            ApiError::InvalidRequest(format!("invalid run event JSON: {error}"))
        })?);
    }
    Ok(events)
}

fn event_lines(source: &str) -> Vec<&str> {
    let mut lines = Vec::new();
    for line in source.lines() {
        for part in line.split("\\n") {
            let value = part.trim();
            if !value.is_empty() {
                lines.push(value);
            }
        }
    }
    lines
}

pub(super) fn read_json(path: impl AsRef<Path>) -> ApiResult<Value> {
    serde_json::from_slice(&fs::read(path)?)
        .map_err(|error| ApiError::InvalidRequest(format!("invalid run JSON: {error}")))
}

pub(super) fn runs_root(root: &Path) -> PathBuf {
    root.join(RUNS_DIR)
}

pub(super) fn run_dir(root: &Path, run_id: &str) -> PathBuf {
    runs_root(root).join(run_id)
}
