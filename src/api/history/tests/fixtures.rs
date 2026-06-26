use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

use crate::api::ApiError;
use crate::api::ApiResult;

pub(crate) fn write_history_fixture(root: &Path) -> ApiResult<()> {
    let run_dir = super::super::storage::run_dir(root, "run-test");
    fs::create_dir_all(&run_dir)?;
    fs::write(
        super::super::storage::runs_root(root).join("last"),
        "run-test",
    )?;
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

pub(crate) fn temp_root() -> std::path::PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock must be after unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "lightflow-api-history-test-{}-{nanos}",
        std::process::id()
    ))
}

pub(super) fn json_bytes(value: &Value) -> ApiResult<Vec<u8>> {
    serde_json::to_vec_pretty(value)
        .map_err(|error| ApiError::InvalidRequest(format!("invalid fixture JSON: {error}")))
}
