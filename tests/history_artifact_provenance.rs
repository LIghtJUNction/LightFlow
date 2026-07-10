use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use lightflow::api::ApiService;
use lightflow::workflow::WorkflowExecutionOptions;
use serde_json::{Value, json};

#[test]
fn sibling_artifacts_keep_distinct_provenance_while_ancestors_collapse()
-> Result<(), Box<dyn std::error::Error>> {
    let root = TestRoot::new();
    let execution = json!({
        "workflow_id":"lightflow.root",
        "status":"completed",
        "artifacts":[artifact()],
        "nodes":[{
            "node_id":"parent",
            "workflow_id":"lightflow.middle",
            "selected_workflow_id":"lightflow.middle",
            "status":"completed",
            "artifacts":[artifact()],
            "nodes":[
                {
                    "node_id":"left",
                    "workflow_id":"lightflow.leaf",
                    "selected_workflow_id":"lightflow.leaf",
                    "status":"completed",
                    "artifacts":[artifact()]
                },
                {
                    "node_id":"right",
                    "workflow_id":"lightflow.leaf",
                    "selected_workflow_id":"lightflow.leaf",
                    "status":"completed",
                    "artifacts":[artifact()]
                }
            ]
        }]
    });
    let service = ApiService::new(root.path());
    service.record_completed_workflow_run(
        "lightflow.root",
        &WorkflowExecutionOptions::default(),
        &execution,
        1,
        2,
    )?;

    let catalog = service.list_artifacts()?;
    let paths = catalog
        .artifacts
        .iter()
        .map(|artifact| artifact.node_path.as_deref())
        .collect::<Vec<_>>();
    assert_eq!(paths, vec![Some("parent/left"), Some("parent/right")]);
    assert!(
        catalog
            .artifacts
            .iter()
            .all(|artifact| artifact.depth == Some(1))
    );
    Ok(())
}

fn artifact() -> Value {
    json!({
        "id":"shared-artifact",
        "kind":"image",
        "path":"/intended/shared.png",
        "mime_type":"image/png",
        "metadata":{}
    })
}

struct TestRoot {
    path: PathBuf,
}

impl TestRoot {
    fn new() -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "lightflow-artifact-provenance-{}-{nanos}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("test root");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TestRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}
