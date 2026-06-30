mod support;

use std::fs;
use std::path::Path;
use support::*;

#[test]
fn lfw_run_records_trace_and_replays_history() -> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    write_project_specs(&root)?;

    let run = lfw(&root, ["run", "lightflow.parent", "--input", "in=hello"])?;
    let run_id = run["run_id"]
        .as_str()
        .expect("run output includes run_id")
        .to_owned();
    let run_dir = root.join(".lightflow/runs").join(&run_id);
    assert!(run_dir.join("manifest.json").exists());
    assert!(run_dir.join("execution.json").exists());
    assert!(run_dir.join("events.jsonl").exists());
    assert_eq!(run["workflow_id"], "lightflow.parent");
    assert_eq!(run["outputs"]["out"], "hello");
    assert_eq!(run["model_locks"], serde_json::json!([]));

    let trace = lfw(&root, ["trace", "last"])?;
    assert_eq!(trace["run_id"], run_id);
    assert_eq!(
        trace["manifest"]["stages"][0]["workflow_id"],
        "lightflow.parent"
    );
    assert_eq!(trace["execution"]["workflow_id"], "lightflow.parent");
    assert_eq!(trace["execution"]["outputs"]["out"], "hello");
    assert_eq!(trace["events"][0]["event"], "run_started");
    assert_eq!(trace["events"][0]["surface"], "cli");
    assert_eq!(trace["events"][1]["event"], "node_completed");
    assert_eq!(trace["events"][1]["node_id"], "nested");
    assert_eq!(trace["events"][1]["workflow_id"], "lightflow.parent");
    assert_eq!(trace["events"][1]["attempts"], 1);
    assert!(trace["events"][1]["duration_ms"].is_number());
    assert_eq!(
        trace["execution"]["nodes"][0]["runtime"]["executor_id"],
        "passthrough"
    );
    assert_eq!(
        trace["execution"]["nodes"][0]["runtime"]["data_policy"],
        "json_values"
    );
    assert_eq!(
        trace["execution"]["nodes"][0]["runtime"]["capabilities"],
        serde_json::json!(["lightflow.data.copy"])
    );
    assert_eq!(trace["events"][1]["runtime"]["executor_id"], "passthrough");
    assert_eq!(trace["events"][1]["runtime"]["data_policy"], "json_values");
    assert_eq!(trace["events"][2]["event"], "node_completed");
    assert_eq!(trace["events"][2]["node_id"], "sink");
    assert_eq!(trace["events"][3]["event"], "run_finished");
    assert_eq!(trace["events"][3]["surface"], "cli");

    let replay = lfw(&root, ["replay"])?;
    assert_eq!(replay["workflow_id"], "lightflow.parent");
    assert_eq!(replay["outputs"]["out"], "hello");
    assert_ne!(replay["run_id"], run_id);
    assert_eq!(replay["replayed_from"], "last");
    assert_eq!(replay["replay"]["runtime_changed"], false);
    assert_eq!(replay["replay"]["model_lock_changed"], false);
    assert_eq!(
        replay["replay"]["original_runtime"],
        replay["replay"]["replayed_runtime"]
    );
    assert_eq!(
        replay["replay"]["original_model_locks"],
        replay["replay"]["replayed_model_locks"]
    );
    assert_eq!(
        replay["replay"]["original_runtime"][0]["runtime"]["executor_id"],
        "passthrough"
    );

    let replay_trace = lfw(&root, ["trace", replay["run_id"].as_str().unwrap()])?;
    assert_eq!(replay_trace["execution"]["outputs"]["out"], "hello");
    assert_eq!(
        replay_trace["execution"]["replay"]["runtime_changed"],
        false
    );
    assert_eq!(replay_trace["events"][0]["surface"], "cli");

    let runs = lfw(&root, ["runs", "list"])?;
    assert_eq!(runs["last"], replay["run_id"]);
    assert_eq!(runs["total"], 2);
    assert_eq!(runs["completed_count"], 2);
    assert_eq!(runs["failed_count"], 0);
    assert_eq!(runs["unknown_count"], 0);
    assert_eq!(runs["unknown_run_ids"], serde_json::json!([]));
    let runs_array = runs["runs"].as_array().unwrap();
    assert_eq!(runs_array.len(), 2);
    assert_eq!(runs_array[0]["run_id"], replay["run_id"]);
    assert_eq!(runs_array[0]["status"], "completed");
    assert!(runs_array[0]["duration_ms"].is_number());
    assert_eq!(runs_array[0]["surface"], "cli");
    assert_eq!(runs_array[0]["workflow_id"], "lightflow.parent");
    assert_eq!(
        runs_array[0]["workflow_ids"],
        serde_json::json!(["lightflow.parent"])
    );
    assert_eq!(runs_array[1]["run_id"], run_id);

    let run_detail = lfw(&root, ["runs", "get", run_id.as_str()])?;
    assert_eq!(run_detail["run_id"], run_id);
    assert_eq!(run_detail["execution"]["outputs"]["out"], "hello");

    let namespaced_replay = lfw(&root, ["runs", "replay", run_id.as_str()])?;
    assert_eq!(namespaced_replay["workflow_id"], "lightflow.parent");
    assert_eq!(namespaced_replay["outputs"]["out"], "hello");
    assert_eq!(namespaced_replay["replayed_from"], run_id);
    assert_ne!(namespaced_replay["run_id"], run_id);
    assert_eq!(namespaced_replay["replay"]["runtime_changed"], false);
    let namespaced_replay_trace = lfw(
        &root,
        [
            "runs",
            "get",
            namespaced_replay["run_id"].as_str().expect("replay run id"),
        ],
    )?;
    assert_eq!(namespaced_replay_trace["events"][0]["surface"], "cli");

    let removed = lfw(&root, ["runs", "rm", run_id.as_str()])?;
    assert_eq!(removed["removed"], true);
    assert!(!root.join(".lightflow/runs").join(&run_id).exists());
    let runs_after_remove = lfw(&root, ["runs", "list"])?;
    assert_eq!(runs_after_remove["runs"].as_array().unwrap().len(), 2);

    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn lfw_replay_reports_model_lock_drift() -> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    write_project_specs(&root)?;
    write_workflow_crate(
        &root,
        "lightflow.model_passthrough",
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow("lightflow.model_passthrough")
        .version("0.1.0")
        .name("Model Passthrough")
        .input("value", "json")
        .output("value", "json")
        .model("weights", "text-to-image")
        .build()
}
"#,
    )?;
    write_workflow_crate(
        &root,
        "lightflow.echo_value",
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow("lightflow.echo_value")
        .version("0.1.0")
        .name("Echo Value")
        .input("value", "json")
        .output("value", "json")
        .build()
}
"#,
    )?;
    let model_path = root.join("models/tiny.gguf");
    fs::create_dir_all(model_path.parent().unwrap())?;
    fs::write(&model_path, b"tiny")?;
    write_model_lock(&root, &model_path, "abc123")?;

    let run = lfw(
        &root,
        [
            "run",
            "lightflow.model_passthrough",
            "--input",
            "value=hello",
        ],
    )?;
    let run_id = run["run_id"].as_str().expect("run id").to_owned();
    assert_eq!(run["outputs"]["value"], "hello");
    assert_eq!(
        run["model_locks"][0]["workflow_id"],
        "lightflow.model_passthrough"
    );
    assert_eq!(run["model_locks"][0]["requirement_id"], "weights");
    assert_eq!(run["model_locks"][0]["lock"]["status"], "available");
    assert_eq!(run["model_locks"][0]["lock"]["sha256"], "abc123");

    write_model_lock(&root, &model_path, "def456")?;
    let replay = lfw(&root, ["replay", run_id.as_str()])?;
    assert_eq!(replay["outputs"]["value"], "hello");
    assert_eq!(replay["replay"]["runtime_changed"], false);
    assert_eq!(replay["replay"]["model_lock_changed"], true);
    assert_eq!(
        replay["replay"]["original_model_locks"][0]["lock"]["sha256"],
        "abc123"
    );
    assert_eq!(
        replay["replay"]["replayed_model_locks"][0]["lock"]["sha256"],
        "def456"
    );

    let replay_trace = lfw(&root, ["trace", replay["run_id"].as_str().unwrap()])?;
    assert_eq!(
        replay_trace["execution"]["replay"]["model_lock_changed"],
        true
    );

    write_model_lock(&root, &model_path, "abc123")?;
    let pipeline = lfw(
        &root,
        [
            "run",
            "lightflow.echo_value",
            "--input",
            "value=hello",
            "|",
            "lightflow.model_passthrough",
        ],
    )?;
    let pipeline_run_id = pipeline["run_id"].as_str().expect("pipeline run id");
    assert_eq!(pipeline["model_locks"][0]["stage_index"], 1);
    assert_eq!(
        pipeline["model_locks"][0]["workflow_id"],
        "lightflow.model_passthrough"
    );

    write_model_lock(&root, &model_path, "def456")?;
    let pipeline_replay = lfw(&root, ["replay", pipeline_run_id])?;
    assert_eq!(pipeline_replay["replay"]["model_lock_changed"], true);
    assert_eq!(
        pipeline_replay["replay"]["original_model_locks"][0]["stage_index"],
        1
    );
    assert_eq!(
        pipeline_replay["replay"]["replayed_model_locks"][0]["stage_index"],
        1
    );

    let _ = fs::remove_dir_all(root);
    Ok(())
}

fn write_model_lock(
    root: &Path,
    model_path: &Path,
    sha256: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    fs::write(
        root.join("lfw.lock"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "version": 2,
            "models": {
                "lightflow.model_passthrough::weights": {
                    "requirement_id": "weights",
                    "variant_id": "tiny",
                    "repo": "example/tiny",
                    "file": "tiny.gguf",
                    "format": "gguf",
                    "sha256": sha256,
                    "hash_algorithm": "sha256",
                    "size_bytes": 4,
                    "snapshot_revision": "rev1",
                    "local_paths": [model_path],
                }
            }
        }))?,
    )?;
    Ok(())
}
