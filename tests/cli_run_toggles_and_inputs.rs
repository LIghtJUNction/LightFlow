mod support;

use serde_json::Value;
use std::fs;
use support::*;

#[test]
fn lfwx_runs_workflow_and_temporarily_toggles_nodes() -> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    write_project_specs(&root)?;

    let run = lfwx(&root, ["lightflow.parent", "--input", "in=hello"])?;
    assert_eq!(run["workflow_id"], "lightflow.parent");
    assert_eq!(run["inputs"]["in"], "hello");
    assert_eq!(run["outputs"]["out"], "hello");
    assert_eq!(run["nodes"][0]["node_id"], "nested");
    assert_eq!(run["nodes"][0]["status"], "completed");
    assert!(run["nodes"][0]["duration_ms"].is_number());
    assert_eq!(run["nodes"][0]["attempts"], 1);
    assert_eq!(run["nodes"][1]["node_id"], "sink");
    assert_eq!(run["nodes"][1]["status"], "completed");
    assert!(run["nodes"][1]["duration_ms"].is_number());
    assert_eq!(run["nodes"][1]["attempts"], 1);

    let disabled = lfwx(
        &root,
        [
            "lightflow.parent",
            "--input",
            "in=hello",
            "--disable",
            "nested",
        ],
    )?;
    assert_eq!(disabled["nodes"][0]["node_id"], "nested");
    assert_eq!(disabled["nodes"][0]["status"], "skipped");
    assert_eq!(disabled["nodes"][0]["attempts"], 0);
    assert_eq!(disabled["outputs"]["out"], Value::Null);

    let enabled = lfw(
        &root,
        [
            "run",
            "lightflow.parent",
            "--input",
            "in=hello",
            "--disable",
            "nested",
            "--enable",
            "nested",
        ],
    )?;
    assert_eq!(enabled["nodes"][0]["status"], "completed");
    assert_eq!(enabled["outputs"]["out"], "hello");

    write_workflow_crate(
        &root,
        "lightflow.parent",
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow("lightflow.parent")
        .version("0.1.0")
        .name("Parent")
        .input("in", "json")
        .output("out", "json")
        .depends_on("lightflow.child", "0.1.0")
        .disabled_node("nested", "lightflow.child")
        .node("sink", "lightflow.sink")
        .edge("nested", "out", "sink", "in")
        .build()
}
"#,
    )?;
    let default_disabled = lfwx(&root, ["lightflow.parent", "--input", "in=hello"])?;
    assert_eq!(default_disabled["nodes"][0]["status"], "skipped");
    assert_eq!(default_disabled["outputs"]["out"], Value::Null);

    let enabled_from_source = lfwx(
        &root,
        [
            "lightflow.parent",
            "--input",
            "in=hello",
            "--enable",
            "nested",
        ],
    )?;
    assert_eq!(enabled_from_source["nodes"][0]["status"], "completed");
    assert_eq!(enabled_from_source["outputs"]["out"], "hello");

    write_workflow_crate(
        &root,
        "lightflow.io",
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow("lightflow.io")
        .version("0.1.0")
        .name("IO")
        .input("text", "text")
        .input("prompt", "text")
        .input("image_path", "path")
        .input("output_path", "path")
        .output("text", "text")
        .output("prompt", "text")
        .output("image_path", "path")
        .output("output_path", "path")
        .build()
}
"#,
    )?;
    let output_path = root.join("generated/image.png");
    let rich_run = lfw(
        &root,
        [
            "run",
            "lightflow.io",
            "--inputs",
            r#"{"text":"from-json"}"#,
            "--prompt",
            "a small house",
            "--image",
            "input/photo.png",
            "--output",
            output_path.to_str().unwrap(),
            "-i",
            "prompt=from-short-input",
        ],
    )?;
    assert_eq!(rich_run["inputs"]["text"], "from-json");
    assert_eq!(rich_run["inputs"]["prompt"], "from-short-input");
    assert_eq!(rich_run["inputs"]["image_path"], "input/photo.png");
    assert_eq!(
        rich_run["inputs"]["output_path"],
        output_path.to_str().unwrap()
    );
    assert_eq!(rich_run["outputs"]["prompt"], "from-short-input");
    assert_eq!(
        rich_run["outputs"]["output_path"],
        output_path.to_str().unwrap()
    );

    let lfx_run = lfx(
        &root,
        [
            "lightflow.io",
            "--text",
            "from-lfx",
            "--input",
            "prompt=from-input",
            "-o",
            "out.png",
        ],
    )?;
    assert_eq!(lfx_run["inputs"]["text"], "from-lfx");
    assert_eq!(lfx_run["inputs"]["prompt"], "from-input");
    assert_eq!(lfx_run["inputs"]["output_path"], "out.png");

    let _ = fs::remove_dir_all(root);
    Ok(())
}
