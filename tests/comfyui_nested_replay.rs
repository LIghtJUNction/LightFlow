mod cli_project_support;
mod comfyui_runtime_support;
mod support;

use std::fs;

use cli_project_support::use_local_lightflow_dependency;
use comfyui_runtime_support::{MockComfyUi, MockResponse};
use serde_json::{Value, json};
use support::{lfw, unique_temp_root, write_workflow_crate};

#[test]
fn nested_composite_preserves_comfy_runtime_events_artifacts_and_replay_drift()
-> Result<(), Box<dyn std::error::Error>> {
    let root = nested_project()?;
    fs::write(root.join("nested.png"), b"first nested upload")?;
    let server = MockComfyUi::start(
        ["nested-original", "nested-replay"]
            .into_iter()
            .flat_map(completed_cycle)
            .collect(),
    )?;
    let inputs = json!({
        "workflow":{"1":{"class_type":"LoadImage","inputs":{"image":"old.png"}}},
        "uploads":[{"path":"nested.png","bind":[{"node_id":"1","input":"image"}]}],
        "server_url":server.url,
        "poll_interval_ms":1
    });
    fs::write(root.join("nested-run.json"), serde_json::to_vec(&inputs)?)?;

    let original = lfw(
        &root,
        [
            "run",
            "lightflow.nested_root",
            "--inputs",
            "@nested-run.json",
        ],
    )?;
    let run_id = original["run_id"].as_str().expect("run id").to_owned();
    assert_eq!(
        original["nodes"][0]["nodes"][0]["runtime"]["executor_id"],
        "comfyui.api.v1"
    );
    assert!(original["nodes"][0]["runtime"].is_null());

    let trace = lfw(&root, ["trace", &run_id])?;
    let nested_event = trace["events"]
        .as_array()
        .expect("events")
        .iter()
        .find(|event| event["node_path"] == "middle/comfy")
        .expect("nested event");
    assert_eq!(nested_event["depth"], 1);
    assert_eq!(nested_event["parent_node_id"], "middle");
    assert_eq!(nested_event["runtime"]["executor_id"], "comfyui.api.v1");

    let artifacts = lfw(&root, ["artifacts", "--run", &run_id])?;
    assert_eq!(
        artifacts["artifacts"].as_array().expect("artifacts").len(),
        1
    );
    assert_eq!(artifacts["artifacts"][0]["node_id"], "comfy");
    assert_eq!(artifacts["artifacts"][0]["node_path"], "middle/comfy");

    fs::write(root.join("nested.png"), b"changed nested upload")?;
    let replay = lfw(&root, ["replay", &run_id])?;
    assert_eq!(replay["replay"]["runtime_changed"], true);
    assert_eq!(
        replay["replay"]["original_runtime"][0]["node_path"],
        "middle/comfy"
    );
    assert_eq!(
        replay["replay"]["replayed_runtime"][0]["node_path"],
        "middle/comfy"
    );
    assert_ne!(
        replay["replay"]["original_runtime"][0]["runtime"]["replay_fingerprint"]["uploads"][0]["sha256"],
        replay["replay"]["replayed_runtime"][0]["runtime"]["replay_fingerprint"]["uploads"][0]["sha256"]
    );

    assert_eq!(server.finish().len(), 8);
    fs::remove_dir_all(root)?;
    Ok(())
}

fn completed_cycle(prompt_id: &str) -> Vec<MockResponse> {
    let mut history = serde_json::Map::new();
    history.insert(
        prompt_id.to_owned(),
        json!({
            "status":{"completed":true,"status_str":"success"},
            "outputs":{"9":{"images":[{"filename":"nested.png","subfolder":"","type":"output"}]}}
        }),
    );
    vec![
        MockResponse::json(json!({"name":"nested.png","subfolder":"lightflow","type":"input"})),
        MockResponse::json(json!({"prompt_id":prompt_id})),
        MockResponse::json(Value::Object(history)),
        MockResponse::bytes("image/png", b"nested image"),
    ]
}

fn nested_project() -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    fs::create_dir_all(&root)?;
    lfw(&root, ["init"])?;
    use_local_lightflow_dependency(&root)?;
    lfw(
        &root,
        [
            "new",
            "comfy_run",
            "--runtime",
            "lightflow.comfyui.workflow",
        ],
    )?;
    write_workflow_crate(
        &root,
        "lightflow.nested_middle",
        &composite_source("lightflow.nested_middle", "comfy", "lightflow.comfy_run"),
    )?;
    write_workflow_crate(
        &root,
        "lightflow.nested_root",
        &composite_source("lightflow.nested_root", "middle", "lightflow.nested_middle"),
    )?;
    Ok(root)
}

fn composite_source(_workflow_id: &str, node_id: &str, child_id: &str) -> String {
    format!(
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {{
    workflow!()
        .name("Nested")
        .input("workflow", "json")
        .input("uploads", "json")
        .input("server_url", "text")
        .input("poll_interval_ms", "integer")
        .output("prompt_id", "text")
        .depends_on("{child_id}", "0.1.0")
        .node("{node_id}", "{child_id}")
        .build()
}}
"#
    )
}
