use super::test_support::{std_project_path, temp_root};
use super::{ApiService, LocalLoopStatus};
use serde_json::json;
use std::fs;

#[test]
fn loop_check_reports_run_catalog_issues_without_hiding_valid_runs()
-> Result<(), Box<dyn std::error::Error>> {
    let root = temp_root();
    let runs_root = root.join(".lightflow/runs");
    let good_run = runs_root.join("run-good");
    fs::create_dir_all(&good_run)?;
    fs::write(runs_root.join("last"), "run-good")?;
    fs::write(
        good_run.join("manifest.json"),
        serde_json::to_vec_pretty(&json!({
            "kind": "workflow_run",
            "run_id": "run-good",
            "status": "completed",
            "stage_input_resolution": "resolved",
            "started_at_ms": 10,
            "completed_at_ms": 12,
            "stages": [
                {
                    "workflow_id": "lightflow.std",
                    "execution": {
                        "inputs": {},
                        "disabled_nodes": [],
                        "enabled_nodes": []
                    }
                },
                {
                    "workflow_id": "lightflow.text_plan",
                    "execution": {
                        "inputs": {},
                        "disabled_nodes": [],
                        "enabled_nodes": []
                    }
                }
            ]
        }))?,
    )?;
    fs::write(
        good_run.join("execution.json"),
        serde_json::to_vec_pretty(&json!({
            "workflow_id": "lightflow.text_plan",
            "outputs": {},
            "nodes": [],
            "artifacts": []
        }))?,
    )?;
    fs::write(
        good_run.join("events.jsonl"),
        r#"{"event":"run_started","run_id":"run-good","surface":"cli"}"#,
    )?;
    let broken_run = runs_root.join("run-broken");
    fs::create_dir_all(&broken_run)?;
    fs::write(broken_run.join("manifest.json"), "{not json")?;

    let service = ApiService::new(&root).with_workflow_paths(vec![std_project_path()]);
    let report = service.local_loop_check(Some("lightflow.text_plan"))?;
    let checks = report.checks;

    assert!(checks.iter().any(|check| {
        check.id == "loop.history.runs"
            && check.status == LocalLoopStatus::Warning
            && check.message.contains("non-fatal issue")
    }));
    assert!(checks.iter().any(|check| {
        check.id == "loop.selected.history.catalog"
            && check.status == LocalLoopStatus::Warning
            && check.message.contains("run-broken")
    }));
    assert!(checks.iter().any(|check| {
        check.id == "loop.selected.history" && check.status == LocalLoopStatus::Passed
    }));
    assert!(checks.iter().any(|check| {
        check.id == "loop.selected.replay" && check.status == LocalLoopStatus::Passed
    }));
    assert!(report.next_commands.iter().any(|command| {
        command == &vec!["lfw".to_owned(), "replay".to_owned(), "run-good".to_owned()]
    }));

    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn loop_check_warns_on_unknown_status_runs() -> Result<(), Box<dyn std::error::Error>> {
    let root = temp_root();
    let runs_root = root.join(".lightflow/runs");
    let unknown_run = runs_root.join("run-unknown");
    fs::create_dir_all(&unknown_run)?;
    fs::write(runs_root.join("last"), "run-unknown")?;
    fs::write(
        unknown_run.join("manifest.json"),
        serde_json::to_vec_pretty(&json!({
            "kind": "workflow_run",
            "run_id": "run-unknown",
            "started_at_ms": 10,
            "completed_at_ms": 12,
            "stages": [{
                "workflow_id": "lightflow.text_plan",
                "execution": {
                    "inputs": {},
                    "disabled_nodes": [],
                    "enabled_nodes": []
                }
            }]
        }))?,
    )?;
    fs::write(
        unknown_run.join("events.jsonl"),
        r#"{"event":"run_started","run_id":"run-unknown","surface":"cli"}"#,
    )?;

    let service = ApiService::new(&root).with_workflow_paths(vec![std_project_path()]);
    let report = service.local_loop_check(None)?;

    assert!(report.checks.iter().any(|check| {
        check.id == "loop.history.runs"
            && check.status == LocalLoopStatus::Warning
            && check.count == Some(1)
            && check.message.contains("unknown-status run")
    }));
    assert!(report.warning_messages.iter().any(|message| {
        message == "loop.history.runs: run history has 1 unknown-status run(s): run-unknown"
    }));

    let _ = fs::remove_dir_all(root);
    Ok(())
}
