use super::run::{RunOptions, RunStage};
use super::{CliError, CliResult, required_arg, validate_path_segment};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub(super) const RUNS_DIR: &str = ".lightflow/runs";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(super) struct RunManifest {
    pub(super) kind: String,
    pub(super) run_id: String,
    #[serde(default = "default_run_status")]
    pub(super) status: String,
    pub(super) started_at_ms: u128,
    pub(super) completed_at_ms: u128,
    pub(super) stages: Vec<RunStage>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(super) struct RunHistory {
    pub(super) run_id: String,
    pub(super) run_dir: PathBuf,
}

pub(super) fn record_run(
    root: &Path,
    stages: &RunOptions,
    execution: &impl Serialize,
    started_at_ms: u128,
    completed_at_ms: u128,
) -> CliResult<RunHistory> {
    let run_id = unique_run_id(root, completed_at_ms)?;
    let run_dir = run_dir(root, &run_id);
    let execution = serde_json::to_value(execution)?;
    fs::create_dir_all(&run_dir)?;
    let manifest = RunManifest {
        kind: "workflow_run".to_owned(),
        run_id: run_id.clone(),
        status: "completed".to_owned(),
        started_at_ms,
        completed_at_ms,
        stages: stages.stages.clone(),
    };
    write_json_pretty(&run_dir.join("manifest.json"), &manifest)?;
    write_json_pretty(&run_dir.join("execution.json"), &execution)?;
    append_event(
        &run_dir,
        json!({
            "event": "run_started",
            "run_id": run_id,
            "at_ms": started_at_ms,
        }),
    )?;
    append_execution_events(&run_dir, &run_id, &execution)?;
    append_event(
        &run_dir,
        json!({
            "event": "run_finished",
            "run_id": run_id,
            "at_ms": completed_at_ms,
        }),
    )?;
    write_text(&runs_root(root).join("last"), &run_id)?;
    Ok(RunHistory { run_id, run_dir })
}

pub(super) fn record_failed_run(
    root: &Path,
    stages: &RunOptions,
    error: &serde_json::Value,
    started_at_ms: u128,
    completed_at_ms: u128,
) -> CliResult<RunHistory> {
    let run_id = unique_run_id(root, completed_at_ms)?;
    let run_dir = run_dir(root, &run_id);
    let execution = json!({
        "status": "failed",
        "error": error,
        "stages": stages.stages.clone(),
    });
    fs::create_dir_all(&run_dir)?;
    let manifest = RunManifest {
        kind: "workflow_run".to_owned(),
        run_id: run_id.clone(),
        status: "failed".to_owned(),
        started_at_ms,
        completed_at_ms,
        stages: stages.stages.clone(),
    };
    write_json_pretty(&run_dir.join("manifest.json"), &manifest)?;
    write_json_pretty(&run_dir.join("execution.json"), &execution)?;
    append_event(
        &run_dir,
        json!({
            "event": "run_started",
            "run_id": run_id,
            "at_ms": started_at_ms,
        }),
    )?;
    append_event(
        &run_dir,
        json!({
            "event": "run_failed",
            "run_id": run_id,
            "at_ms": completed_at_ms,
            "error": error,
        }),
    )?;
    write_text(&runs_root(root).join("last"), &run_id)?;
    Ok(RunHistory { run_id, run_dir })
}

fn append_execution_events(
    run_dir: &Path,
    run_id: &str,
    execution: &serde_json::Value,
) -> CliResult<()> {
    if execution
        .get("pipeline")
        .and_then(serde_json::Value::as_bool)
        == Some(true)
    {
        for (stage_index, stage) in execution
            .get("stages")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .enumerate()
        {
            append_stage_node_events(run_dir, run_id, stage_index, stage)?;
        }
        return Ok(());
    }

    append_stage_node_events(run_dir, run_id, 0, execution)
}

fn append_stage_node_events(
    run_dir: &Path,
    run_id: &str,
    stage_index: usize,
    execution: &serde_json::Value,
) -> CliResult<()> {
    let workflow_id = execution
        .get("workflow_id")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    let Some(nodes) = execution.get("nodes").and_then(serde_json::Value::as_array) else {
        return Ok(());
    };
    for (node_index, node) in nodes.iter().enumerate() {
        let status = node
            .get("status")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("unknown");
        let event = if status == "skipped" {
            "node_skipped"
        } else {
            "node_completed"
        };
        append_event(
            run_dir,
            json!({
                "event": event,
                "run_id": run_id,
                "stage_index": stage_index,
                "node_index": node_index,
                "workflow_id": workflow_id,
                "node_id": node.get("node_id").cloned().unwrap_or_default(),
                "node_workflow_id": node.get("workflow_id").cloned().unwrap_or_default(),
                "selected_workflow_id": node.get("selected_workflow_id").cloned().unwrap_or(serde_json::Value::Null),
                "status": status,
                "duration_ms": node.get("duration_ms").cloned().unwrap_or(0.into()),
                "attempts": node.get("attempts").cloned().unwrap_or(0.into()),
                "inputs": node.get("inputs").cloned().unwrap_or_else(|| json!({})),
                "outputs": node.get("outputs").cloned().unwrap_or_else(|| json!({})),
                "artifacts": node.get("artifacts").cloned().unwrap_or_else(|| json!([])),
            }),
        )?;
    }
    Ok(())
}

pub(super) fn trace_run(root: &Path, args: &[String]) -> CliResult<serde_json::Value> {
    let selector = args.first().map(String::as_str).unwrap_or("last");
    if args.len() > 1 {
        return Err(CliError::Usage(format!(
            "unexpected argument for trace: {}",
            args[1]
        )));
    }
    let run_id = resolve_run_id(root, selector)?;
    let run_dir = run_dir(root, &run_id);
    let manifest = read_manifest(root, &run_id)?;
    let execution = read_json(run_dir.join("execution.json"))?;
    let events = read_events(&run_dir)?;
    Ok(json!({
        "run_id": run_id,
        "run_dir": run_dir,
        "manifest": manifest,
        "execution": execution,
        "events": events,
    }))
}

pub(super) fn manage_runs(root: &Path, args: &[String]) -> CliResult<serde_json::Value> {
    let action = args.first().map(String::as_str).unwrap_or("list");
    match action {
        "list" | "ls" => {
            ensure_no_history_extra_args(args, 1, "runs list")?;
            list_runs(root)
        }
        "get" | "show" | "trace" => {
            let run_id = args.get(1).map(String::as_str).unwrap_or("last");
            ensure_no_history_extra_args(args, 2, "runs get")?;
            trace_run(root, &[run_id.to_owned()])
        }
        "rm" | "remove" | "delete" => {
            let run_id = required_arg(args, 1, "run id")?;
            ensure_no_history_extra_args(args, 2, "runs rm")?;
            remove_run(root, run_id)
        }
        "-h" | "--help" | "help" => Err(CliError::Usage(runs_usage())),
        _ => Err(CliError::Usage(runs_usage())),
    }
}

fn list_runs(root: &Path) -> CliResult<serde_json::Value> {
    let root = runs_root(root);
    let last = fs::read_to_string(root.join("last"))
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());
    let mut runs = Vec::new();
    if root.exists() {
        for entry in fs::read_dir(&root)? {
            let entry = entry?;
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let manifest_path = path.join("manifest.json");
            if !manifest_path.exists() {
                continue;
            }
            let manifest: RunManifest = serde_json::from_slice(&fs::read(&manifest_path)?)?;
            let first_stage = manifest.stages.first();
            runs.push(json!({
                "run_id": manifest.run_id,
                "status": manifest.status,
                "started_at_ms": manifest.started_at_ms,
                "completed_at_ms": manifest.completed_at_ms,
                "workflow_id": first_stage.map(|stage| stage.workflow_id.as_str()).unwrap_or_default(),
                "stages": manifest.stages.len(),
                "run_dir": path,
            }));
        }
    }
    runs.sort_by(|left, right| {
        right["completed_at_ms"]
            .as_u64()
            .cmp(&left["completed_at_ms"].as_u64())
            .then_with(|| left["run_id"].as_str().cmp(&right["run_id"].as_str()))
    });
    Ok(json!({
        "root": root,
        "last": last,
        "runs": runs,
    }))
}

fn remove_run(root: &Path, selector: &str) -> CliResult<serde_json::Value> {
    let run_id = resolve_run_id(root, selector)?;
    let path = run_dir(root, &run_id);
    let removed = if path.exists() {
        fs::remove_dir_all(&path)?;
        true
    } else {
        false
    };
    let last_path = runs_root(root).join("last");
    if fs::read_to_string(&last_path)
        .ok()
        .is_some_and(|last| last.trim() == run_id)
    {
        let _ = fs::remove_file(&last_path);
    }
    Ok(json!({
        "removed": removed,
        "run_id": run_id,
        "run_dir": path,
    }))
}

pub(super) fn read_manifest(root: &Path, selector: &str) -> CliResult<RunManifest> {
    let run_id = resolve_run_id(root, selector)?;
    let path = run_dir(root, &run_id).join("manifest.json");
    let manifest = serde_json::from_slice(&fs::read(path)?)?;
    Ok(manifest)
}

fn resolve_run_id(root: &Path, selector: &str) -> CliResult<String> {
    if selector == "last" {
        let path = runs_root(root).join("last");
        let run_id = fs::read_to_string(path)?.trim().to_owned();
        validate_path_segment(&run_id, "run id")?;
        return Ok(run_id);
    }
    validate_path_segment(selector, "run id")?;
    Ok(selector.to_owned())
}

fn run_dir(root: &Path, run_id: &str) -> PathBuf {
    runs_root(root).join(run_id)
}

fn runs_root(root: &Path) -> PathBuf {
    root.join(RUNS_DIR)
}

fn unique_run_id(root: &Path, now: u128) -> CliResult<String> {
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
    Err(CliError::Usage(
        "could not allocate unique run id".to_owned(),
    ))
}

fn default_run_status() -> String {
    "completed".to_owned()
}

pub(super) fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

fn write_json_pretty(path: &Path, value: &impl Serialize) -> CliResult<()> {
    write_text(path, &format!("{}\n", serde_json::to_string_pretty(value)?))
}

fn write_text(path: &Path, value: &str) -> CliResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, value)?;
    Ok(())
}

fn read_json(path: impl AsRef<Path>) -> CliResult<serde_json::Value> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

fn append_event(run_dir: &Path, value: serde_json::Value) -> CliResult<()> {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(run_dir.join("events.jsonl"))?;
    serde_json::to_writer(&mut file, &value)?;
    file.write_all(b"\n")?;
    Ok(())
}

fn read_events(run_dir: &Path) -> CliResult<Vec<serde_json::Value>> {
    let path = run_dir.join("events.jsonl");
    if !path.exists() {
        return Ok(Vec::new());
    }
    fs::read_to_string(path)?
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).map_err(CliError::from))
        .collect()
}

pub(super) fn parse_replay_run_id(args: &[String]) -> CliResult<&str> {
    let run_id = required_arg(args, 0, "run id")?;
    if let Some(extra) = args.get(1) {
        return Err(CliError::Usage(format!(
            "unexpected argument for replay: {extra}"
        )));
    }
    Ok(run_id)
}

fn ensure_no_history_extra_args(args: &[String], max_len: usize, command: &str) -> CliResult<()> {
    if let Some(extra) = args.get(max_len) {
        return Err(CliError::Usage(format!(
            "unexpected argument for {command}: {extra}"
        )));
    }
    Ok(())
}

fn runs_usage() -> String {
    [
        "usage:",
        "  lfw runs list",
        "  lfw runs get [last|run_id]",
        "  lfw runs rm <last|run_id>",
    ]
    .join("\n")
}
