use super::util::validate_id_segment;
use super::{ApiError, ApiResult};
use crate::workflow::WorkflowArtifact;
use serde::Serialize;
use serde_json::Value;
#[cfg(test)]
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};

const RUNS_DIR: &str = ".lightflow/runs";

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RunCatalog {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last: Option<String>,
    pub runs: Vec<RunSummary>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RunSummary {
    pub run_id: String,
    pub status: String,
    pub started_at_ms: u128,
    pub completed_at_ms: u128,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workflow_id: Option<String>,
    pub stages: usize,
    pub run_dir: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RunTrace {
    pub run_id: String,
    pub run_dir: PathBuf,
    pub manifest: Value,
    pub execution: Value,
    pub events: Vec<Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RunEvents {
    pub run_id: String,
    pub events: Vec<Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ArtifactCatalog {
    pub artifacts: Vec<RunArtifact>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RunArtifact {
    pub run_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workflow_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_id: Option<String>,
    pub artifact: WorkflowArtifact,
}

pub(super) fn list_runs(root: &Path) -> ApiResult<RunCatalog> {
    let runs_root = runs_root(root);
    let last = read_last(&runs_root);
    let mut runs = Vec::new();
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
            let manifest = read_json(&manifest_path)?;
            let Some(run_id) = manifest.get("run_id").and_then(Value::as_str) else {
                continue;
            };
            runs.push(RunSummary {
                run_id: run_id.to_owned(),
                status: manifest
                    .get("status")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown")
                    .to_owned(),
                started_at_ms: manifest
                    .get("started_at_ms")
                    .and_then(Value::as_u64)
                    .map(u128::from)
                    .unwrap_or_default(),
                completed_at_ms: manifest
                    .get("completed_at_ms")
                    .and_then(Value::as_u64)
                    .map(u128::from)
                    .unwrap_or_default(),
                workflow_id: first_workflow_id(&manifest),
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
    Ok(RunCatalog { last, runs })
}

pub(super) fn get_run(root: &Path, selector: &str) -> ApiResult<RunTrace> {
    let run_id = resolve_run_id(root, selector)?;
    let run_dir = run_dir(root, &run_id);
    Ok(RunTrace {
        run_id,
        run_dir: run_dir.clone(),
        manifest: read_json(run_dir.join("manifest.json"))?,
        execution: read_json(run_dir.join("execution.json"))?,
        events: read_events(&run_dir)?,
    })
}

pub(super) fn get_run_events(root: &Path, selector: &str) -> ApiResult<RunEvents> {
    let run_id = resolve_run_id(root, selector)?;
    let run_dir = run_dir(root, &run_id);
    Ok(RunEvents {
        run_id,
        events: read_events(&run_dir)?,
    })
}

pub(super) fn list_artifacts(root: &Path) -> ApiResult<ArtifactCatalog> {
    let mut artifacts = Vec::new();
    for run in list_runs(root)?.runs {
        let execution_path = run.run_dir.join("execution.json");
        if !execution_path.exists() {
            continue;
        }
        let execution = read_json(&execution_path)?;
        collect_execution_artifacts(&run.run_id, &execution, &mut artifacts);
    }
    Ok(ArtifactCatalog { artifacts })
}

fn collect_execution_artifacts(run_id: &str, execution: &Value, artifacts: &mut Vec<RunArtifact>) {
    if execution.get("pipeline").and_then(Value::as_bool) == Some(true) {
        for stage in execution
            .get("stages")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            collect_execution_artifacts(run_id, stage, artifacts);
        }
        return;
    }

    let workflow_id = execution
        .get("workflow_id")
        .and_then(Value::as_str)
        .map(str::to_owned);
    collect_artifact_array(
        run_id,
        workflow_id.as_deref(),
        None,
        execution.get("artifacts"),
        artifacts,
    );
    for node in execution
        .get("nodes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let node_id = node.get("node_id").and_then(Value::as_str);
        collect_artifact_array(
            run_id,
            workflow_id.as_deref(),
            node_id,
            node.get("artifacts"),
            artifacts,
        );
    }
}

fn collect_artifact_array(
    run_id: &str,
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
            workflow_id: workflow_id.map(str::to_owned),
            node_id: node_id.map(str::to_owned),
            artifact,
        });
    }
}

fn resolve_run_id(root: &Path, selector: &str) -> ApiResult<String> {
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

fn read_last(root: &Path) -> Option<String> {
    fs::read_to_string(root.join("last"))
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn first_workflow_id(manifest: &Value) -> Option<String> {
    manifest
        .get("stages")
        .and_then(Value::as_array)
        .and_then(|stages| stages.first())
        .and_then(|stage| stage.get("workflow_id"))
        .and_then(Value::as_str)
        .map(str::to_owned)
}

fn read_events(run_dir: &Path) -> ApiResult<Vec<Value>> {
    let path = run_dir.join("events.jsonl");
    if !path.exists() {
        return Ok(Vec::new());
    }
    let source = fs::read_to_string(path)?;
    let mut events = Vec::new();
    for line in source.lines().filter(|line| !line.trim().is_empty()) {
        events.push(serde_json::from_str(line).map_err(|error| {
            ApiError::InvalidRequest(format!("invalid run event JSON: {error}"))
        })?);
    }
    Ok(events)
}

fn read_json(path: impl AsRef<Path>) -> ApiResult<Value> {
    serde_json::from_slice(&fs::read(path)?)
        .map_err(|error| ApiError::InvalidRequest(format!("invalid run JSON: {error}")))
}

fn runs_root(root: &Path) -> PathBuf {
    root.join(RUNS_DIR)
}

fn run_dir(root: &Path, run_id: &str) -> PathBuf {
    runs_root(root).join(run_id)
}

#[cfg(test)]
pub(crate) fn write_history_fixture(root: &Path) -> ApiResult<()> {
    let run_dir = run_dir(root, "run-test");
    fs::create_dir_all(&run_dir)?;
    fs::write(runs_root(root).join("last"), "run-test")?;
    fs::write(
        run_dir.join("manifest.json"),
        json_bytes(&json!({
            "kind": "workflow_run",
            "run_id": "run-test",
            "status": "completed",
            "started_at_ms": 1,
            "completed_at_ms": 2,
            "stages": [{"workflow_id": "lightflow.fixture"}]
        }))?,
    )?;
    fs::write(
        run_dir.join("execution.json"),
        json_bytes(&json!({
            "workflow_id": "lightflow.fixture",
            "version": "0.1.0",
            "inputs": {},
            "outputs": {},
            "artifacts": [{
                "id": "image",
                "kind": "image",
                "path": "/tmp/image.png",
                "mime_type": "image/png",
                "metadata": {}
            }],
            "nodes": []
        }))?,
    )?;
    fs::write(
        run_dir.join("events.jsonl"),
        "{\"event\":\"run_started\",\"run_id\":\"run-test\",\"at_ms\":1}\n",
    )?;
    Ok(())
}

#[cfg(test)]
fn json_bytes(value: &Value) -> ApiResult<Vec<u8>> {
    serde_json::to_vec_pretty(value)
        .map_err(|error| ApiError::InvalidRequest(format!("invalid fixture JSON: {error}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn get_run_rejects_path_traversal_selectors() -> Result<(), Box<dyn std::error::Error>> {
        let root = temp_root();
        let outside = root.join("outside");
        fs::create_dir_all(&outside)?;
        fs::create_dir_all(runs_root(&root))?;
        fs::write(outside.join("manifest.json"), "{}")?;
        fs::write(outside.join("execution.json"), "{}")?;
        fs::write(outside.join("events.jsonl"), "")?;

        let direct_error = get_run(&root, "../../outside")
            .expect_err("direct traversal selectors should be rejected")
            .to_string();
        assert!(direct_error.contains("invalid run id path segment"));

        fs::write(runs_root(&root).join("last"), "../../outside")?;
        let last_error = get_run(&root, "last")
            .expect_err("last should not be allowed to point outside runs")
            .to_string();
        assert!(last_error.contains("invalid run id path segment"));
        assert!(outside.exists());

        let _ = fs::remove_dir_all(root);
        Ok(())
    }

    fn temp_root() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock must be after unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "lightflow-api-history-test-{}-{nanos}",
            std::process::id()
        ))
    }
}
