#![allow(unused_imports)]

mod cli_project_support;
mod support;

use cli_project_support::*;
use lightflow::api::{ApiService, CheckProfile, ReleaseCheckOptions};
use serde_json::Value;
use std::fs;
use std::path::Path;
use std::process::Command;
use support::*;

#[test]
fn lfw_release_check_dry_run_reports_local_loop_warnings() -> Result<(), Box<dyn std::error::Error>>
{
    let root = unique_temp_root();
    fs::create_dir_all(root.join("docs"))?;
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\n### CLI\n\n### API\n\n### Workflows\n\n### Runtime\n\n### Known Limitations\n\n### Migration Notes\n",
    )?;
    fs::write(root.join("docs/v0.2-checklist.md"), "# Checklist\n")?;
    fs::write(root.join("docs/runtime-verification.md"), "# Runtime\n")?;
    fs::write(
        root.join("docs/local-workflow-loop.md"),
        "# Local Workflow Loop\n\n## Verification Gates\n",
    )?;
    lfw(&root, ["init"])?;
    use_local_lightflow_dependency(&root)?;
    complete_generated_workflow_metadata(&root, "examples", "example")?;
    git_ok(&root, ["init"])?;
    git_ok(&root, ["add", "."])?;
    git_ok(
        &root,
        [
            "-c",
            "user.name=LightFlow Test",
            "-c",
            "user.email=lightflow@example.test",
            "commit",
            "-m",
            "initial workflow",
        ],
    )?;

    let run_dir = root.join(".lightflow/runs/run-unknown");
    fs::create_dir_all(&run_dir)?;
    fs::write(root.join(".lightflow/runs/last"), "run-unknown")?;
    fs::write(
        run_dir.join("manifest.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "kind": "workflow_run",
            "run_id": "run-unknown",
            "started_at_ms": 10,
            "completed_at_ms": 12,
            "stages": [{
                "workflow_id": "lightflow.example",
                "execution": {
                    "inputs": {},
                    "disabled_nodes": [],
                    "enabled_nodes": []
                }
            }]
        }))?,
    )?;
    fs::write(
        run_dir.join("events.jsonl"),
        r#"{"event":"run_started","run_id":"run-unknown","surface":"cli"}"#,
    )?;
    let second_run_dir = root.join(".lightflow/runs/run-unknown-2");
    fs::create_dir_all(&second_run_dir)?;
    fs::write(
        second_run_dir.join("manifest.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "kind": "workflow_run",
            "run_id": "run-unknown-2",
            "started_at_ms": 20,
            "completed_at_ms": 22,
            "stages": [{
                "workflow_id": "lightflow.example",
                "execution": {
                    "inputs": {},
                    "disabled_nodes": [],
                    "enabled_nodes": []
                }
            }]
        }))?,
    )?;
    fs::write(
        second_run_dir.join("events.jsonl"),
        r#"{"event":"run_started","run_id":"run-unknown-2","surface":"cli"}"#,
    )?;
    let completed_run_dir = root.join(".lightflow/runs/run-completed");
    fs::create_dir_all(&completed_run_dir)?;
    fs::write(
        completed_run_dir.join("manifest.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "kind": "workflow_run",
            "run_id": "run-completed",
            "status": "completed",
            "started_at_ms": 30,
            "completed_at_ms": 32,
            "stages": [{
                "workflow_id": "lightflow.example",
                "execution": {
                    "inputs": {},
                    "disabled_nodes": [],
                    "enabled_nodes": []
                }
            }]
        }))?,
    )?;
    fs::write(
        completed_run_dir.join("events.jsonl"),
        r#"{"event":"run_finished","run_id":"run-completed","surface":"cli"}"#,
    )?;

    let report = lfw(
        &root,
        ["release", "check", "--workflow", "lightflow.example"],
    )?;
    assert_eq!(report["valid"], true, "release report:\n{report:#?}");
    assert!(
        report["warnings"]
            .as_array()
            .expect("release warnings")
            .iter()
            .any(|warning| warning
                .as_str()
                .unwrap_or_default()
                .contains("loop.history.runs: run history has 2 unknown-status run")),
        "release report:\n{report:#?}"
    );
    let local_loop_review = report["checks"]
        .as_array()
        .expect("release checks")
        .iter()
        .find(|check| check["id"] == "release.review.local_workflow_loop")
        .expect("local workflow loop review");
    assert_eq!(local_loop_review["status"], "warning");
    assert_eq!(local_loop_review["count"], 2);

    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn release_check_reviews_configured_workflow_paths() -> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    let external = unique_temp_root();
    fs::create_dir_all(&root)?;
    fs::create_dir_all(&external)?;
    write_workflow_crate_in(
        &external,
        "lightflow.external_model",
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow!()
        .name("External Model")
        .input("model", "text")
        .output("value", "json")
        .model("external_weights", "text-to-image")
        .input_model_requirement("model", "external_weights")
        .build()
}
"#,
    )?;
    let run_dir = root.join(".lightflow/runs/run-external");
    fs::create_dir_all(&run_dir)?;
    fs::write(root.join(".lightflow/runs/last"), "run-external")?;
    fs::write(
        run_dir.join("manifest.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "kind": "workflow_run",
            "run_id": "run-external",
            "status": "completed",
            "started_at_ms": 10,
            "completed_at_ms": 12,
            "stages": [{
                "workflow_id": "lightflow.external_model",
                "execution": {
                    "inputs": {},
                    "disabled_nodes": [],
                    "enabled_nodes": []
                }
            }]
        }))?,
    )?;
    fs::write(
        run_dir.join("events.jsonl"),
        r#"{"event":"run_finished","run_id":"run-external","surface":"cli"}"#,
    )?;

    let service = ApiService::new(&root).with_workflow_paths(vec![external.clone()]);
    let report = service.release_check(&ReleaseCheckOptions {
        apply: false,
        workflow_id: "lightflow.external_model".to_owned(),
        project: None,
        profile: CheckProfile::Release,
    })?;
    let local_loop_review = report
        .checks
        .iter()
        .find(|check| check.id == "release.review.local_workflow_loop")
        .expect("local loop review");
    assert_eq!(serde_json::to_value(local_loop_review.status)?, "warning");
    assert!(
        local_loop_review
            .message
            .contains("lightflow.external_model::external_weights"),
        "release review message:\n{}",
        local_loop_review.message
    );
    assert!(
        local_loop_review
            .details
            .iter()
            .any(|detail| detail.contains("lightflow.external_model::external_weights")),
        "release review details:\n{:#?}",
        local_loop_review.details
    );
    let selected_review = report
        .checks
        .iter()
        .find(|check| check.id == "release.review.selected_workflow_loop")
        .expect("selected loop review");
    assert_eq!(serde_json::to_value(selected_review.status)?, "warning");
    assert!(
        selected_review.message.contains("loop.selected.models"),
        "selected review message:\n{}",
        selected_review.message
    );
    assert!(
        selected_review
            .details
            .iter()
            .any(|detail| detail.contains("loop.selected.models")),
        "selected review details:\n{:#?}",
        selected_review.details
    );

    let _ = fs::remove_dir_all(root);
    let _ = fs::remove_dir_all(external);
    Ok(())
}
