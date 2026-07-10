mod support;

use std::fs;
use support::*;

#[test]
fn lfw_run_chains_workflows_with_pipe() -> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    fs::create_dir_all(&root)?;
    fs::write(
        root.join("Cargo.toml"),
        r#"[workspace]
resolver = "3"
members = ["workflows/*/*"]

[workspace.dependencies]
lightflow = { path = "." }
"#,
    )?;
    write_workflow_crate(
        &root,
        "lightflow.first",
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow!()
        .name("First")
        .input("text", "text")
        .output("text", "text")
        .build()
}
"#,
    )?;
    write_workflow_crate(
        &root,
        "lightflow.second",
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow!()
        .name("Second")
        .input("text", "text")
        .output("text", "text")
        .build()
}
"#,
    )?;
    write_workflow_crate(
        &root,
        "lightflow.broken",
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow!()
        .name("Broken")
        .input("text", "text")
        .output("text", "text")
        .runtime("runtime", "lightflow.missing.executor")
        .build()
}
"#,
    )?;

    let chained = lfw(
        &root,
        [
            "run",
            "lightflow.first",
            "-i",
            "text=hello",
            "|",
            "lightflow.second",
        ],
    )?;
    assert_eq!(chained["pipeline"], true);
    assert_eq!(chained["outputs"]["text"], "hello");
    assert_eq!(chained["stages"][0]["workflow_id"], "lightflow.first");
    assert_eq!(chained["stages"][1]["workflow_id"], "lightflow.second");
    assert_eq!(chained["stages"][1]["inputs"]["text"], "hello");
    let chained_run_id = chained["run_id"].as_str().expect("pipeline run id");
    let chained_trace = lfw(&root, ["trace", chained_run_id])?;
    assert_eq!(
        chained_trace["manifest"]["stage_input_resolution"],
        "resolved"
    );
    assert_eq!(
        chained_trace["manifest"]["stages"][1]["execution"]["inputs"]["text"],
        "hello"
    );
    let replayed = lfw(&root, ["replay", chained_run_id])?;
    assert_eq!(replayed["outputs"]["text"], "hello");
    let replayed_trace = lfw(&root, ["trace", replayed["run_id"].as_str().unwrap()])?;
    assert_eq!(
        replayed_trace["manifest"]["stages"][1]["execution"]["inputs"]["text"],
        "hello"
    );
    let runs = lfw(&root, ["runs", "list"])?;
    assert!(
        runs["runs"].as_array().expect("runs").iter().any(|run| {
            run["run_id"] == chained_run_id
                && run["duration_ms"].is_number()
                && run["surface"] == "cli"
                && run["workflow_id"] == "lightflow.first"
                && run["workflow_ids"] == serde_json::json!(["lightflow.first", "lightflow.second"])
        }),
        "runs:\n{runs}"
    );

    let overridden = lfw(
        &root,
        [
            "run",
            "lightflow.first",
            "-i",
            "text=hello",
            "|",
            "lightflow.second",
            "-i",
            "text=override",
        ],
    )?;
    assert_eq!(overridden["outputs"]["text"], "override");
    assert_eq!(overridden["stages"][1]["inputs"]["text"], "override");

    let failed = lfw_command(&root)
        .args([
            "run",
            "lightflow.first",
            "-i",
            "text=hello",
            "|",
            "lightflow.broken",
        ])
        .output()?;
    assert!(!failed.status.success());
    let failed_trace = lfw(&root, ["trace", "last"])?;
    assert_eq!(failed_trace["manifest"]["status"], "failed");
    assert_eq!(
        failed_trace["manifest"]["stages"][1]["execution"]["inputs"]["text"],
        "hello"
    );
    assert_eq!(
        failed_trace["execution"]["partial_execution"]["stages"][0]["workflow_id"],
        "lightflow.first"
    );
    assert_eq!(
        failed_trace["execution"]["partial_execution"]["outputs"]["text"],
        "hello"
    );
    assert!(
        failed_trace["events"]
            .as_array()
            .expect("failed events")
            .iter()
            .any(|event| {
                event["event"] == "stage_completed"
                    && event["stage_index"] == 0
                    && event["workflow_id"] == "lightflow.first"
                    && event["outputs"]["text"] == "hello"
            }),
        "failed trace events:\n{failed_trace}"
    );
    assert_eq!(
        failed_trace["events"]
            .as_array()
            .expect("failed events")
            .last()
            .expect("failed event tail")["event"],
        "run_failed"
    );

    let _ = fs::remove_dir_all(root);
    Ok(())
}
