use std::fs;

use serde_json::json;

use super::super::list_artifacts;
use super::fixtures::json_bytes;
use super::temp_root;

#[test]
fn list_artifacts_includes_stage_and_node_indexes_for_pipelines()
-> Result<(), Box<dyn std::error::Error>> {
    let root = temp_root();
    let run_dir = super::super::storage::run_dir(&root, "run-pipeline-artifacts");
    fs::create_dir_all(&run_dir)?;
    fs::write(
        super::super::storage::runs_root(&root).join("last"),
        "run-pipeline-artifacts",
    )?;
    fs::write(
        run_dir.join("manifest.json"),
        json_bytes(&json!({
            "kind": "workflow_run",
            "run_id": "run-pipeline-artifacts",
            "status": "completed",
            "stage_input_resolution": "resolved",
            "started_at_ms": 1,
            "completed_at_ms": 2,
            "stages": [
                {"workflow_id": "lightflow.first", "execution": {"inputs": {}}},
                {"workflow_id": "lightflow.second", "execution": {"inputs": {}}}
            ]
        }))?,
    )?;
    fs::write(
        run_dir.join("execution.json"),
        json_bytes(&json!({
            "pipeline": true,
            "stages": [
                {
                    "workflow_id": "lightflow.first",
                    "artifacts": [],
                    "nodes": []
                },
                {
                    "workflow_id": "lightflow.second",
                    "artifacts": [{
                        "id": "stage-image",
                        "kind": "image",
                        "path": "/tmp/stage.png",
                        "mime_type": "image/png",
                        "metadata": {}
                    }],
                    "nodes": [{
                        "node_id": "render",
                        "artifacts": [{
                            "id": "node-image",
                            "kind": "image",
                            "path": "/tmp/node.png",
                            "mime_type": "image/png",
                            "metadata": {}
                        }]
                    }]
                }
            ]
        }))?,
    )?;

    let catalog = list_artifacts(&root)?;

    assert_eq!(catalog.artifacts.len(), 2);
    assert_eq!(catalog.artifacts[0].stage_index, Some(1));
    assert_eq!(catalog.artifacts[0].node_index, None);
    assert_eq!(
        catalog.artifacts[0].workflow_id.as_deref(),
        Some("lightflow.second")
    );
    assert_eq!(catalog.artifacts[1].stage_index, Some(1));
    assert_eq!(catalog.artifacts[1].node_index, Some(0));
    assert_eq!(catalog.artifacts[1].node_id.as_deref(), Some("render"));

    let _ = fs::remove_dir_all(root);
    Ok(())
}
