use std::fs;

use super::{fixtures, temp_root, write_history_fixture};
use serde_json::json;

use super::super::{RunListOptions, get_run, list_runs, list_runs_with_options};

#[test]
fn get_run_rejects_path_traversal_selectors() -> Result<(), Box<dyn std::error::Error>> {
    let root = temp_root();
    let outside = root.join("outside");
    fs::create_dir_all(&outside)?;
    fs::create_dir_all(super::super::storage::runs_root(&root))?;
    fs::write(outside.join("manifest.json"), "{}")?;
    fs::write(outside.join("execution.json"), "{}")?;
    fs::write(outside.join("events.jsonl"), "")?;

    let direct_error = get_run(&root, "../../outside")
        .expect_err("direct traversal selectors should be rejected")
        .to_string();
    assert!(direct_error.contains("invalid run id path segment"));

    fs::write(
        super::super::storage::runs_root(&root).join("last"),
        "../../outside",
    )?;
    let last_error = get_run(&root, "last")
        .expect_err("last should not be allowed to point outside runs")
        .to_string();
    assert!(last_error.contains("invalid run id path segment"));
    assert!(outside.exists());

    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn list_runs_reports_bad_manifests_without_failing() -> Result<(), Box<dyn std::error::Error>> {
    let root = temp_root();
    write_history_fixture(&root)?;
    let broken_dir = super::super::storage::run_dir(&root, "run-broken");
    fs::create_dir_all(&broken_dir)?;
    fs::write(broken_dir.join("manifest.json"), "{not json")?;
    let missing_id_dir = super::super::storage::run_dir(&root, "run-missing-id");
    fs::create_dir_all(&missing_id_dir)?;
    fs::write(
        missing_id_dir.join("manifest.json"),
        fixtures::json_bytes(&json!({
            "kind": "workflow_run",
            "status": "completed",
            "started_at_ms": 3,
            "completed_at_ms": 4,
            "stages": []
        }))?,
    )?;

    let catalog = list_runs(&root)?;

    assert_eq!(catalog.runs.len(), 1);
    assert_eq!(catalog.runs[0].run_id, "run-test");
    assert_eq!(catalog.issues.len(), 2);
    assert!(
        catalog
            .issues
            .iter()
            .any(|issue| issue.contains("could not read run manifest")),
        "{:?}",
        catalog.issues
    );
    assert!(
        catalog
            .issues
            .iter()
            .any(|issue| issue.contains("run manifest is missing run_id")),
        "{:?}",
        catalog.issues
    );
    assert!(get_run(&root, "run-broken").is_err());

    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn list_runs_reports_unknown_run_ids() -> Result<(), Box<dyn std::error::Error>> {
    let root = temp_root();
    write_history_fixture(&root)?;
    let unknown_dir = super::super::storage::run_dir(&root, "run-unknown");
    fs::create_dir_all(&unknown_dir)?;
    fs::write(
        unknown_dir.join("manifest.json"),
        fixtures::json_bytes(&json!({
            "kind": "workflow_run",
            "run_id": "run-unknown",
            "started_at_ms": 3,
            "completed_at_ms": 4,
            "stages": []
        }))?,
    )?;
    fs::write(unknown_dir.join("execution.json"), "{}")?;
    fs::write(unknown_dir.join("events.jsonl"), "")?;

    let catalog = list_runs(&root)?;

    assert_eq!(catalog.total, 2);
    assert_eq!(catalog.completed_count, 1);
    assert_eq!(catalog.failed_count, 0);
    assert_eq!(catalog.unknown_count, 1);
    assert_eq!(catalog.unknown_run_ids, vec!["run-unknown"]);

    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn list_runs_infers_legacy_status_from_terminal_events() -> Result<(), Box<dyn std::error::Error>> {
    let root = temp_root();
    write_history_fixture(&root)?;

    let completed_dir = super::super::storage::run_dir(&root, "run-legacy-completed");
    fs::create_dir_all(&completed_dir)?;
    fs::write(
        completed_dir.join("manifest.json"),
        fixtures::json_bytes(&json!({
            "kind": "workflow_run",
            "run_id": "run-legacy-completed",
            "started_at_ms": 3,
            "completed_at_ms": 4,
            "stages": [{"workflow_id": "lightflow.legacy"}]
        }))?,
    )?;
    fs::write(completed_dir.join("execution.json"), "{}")?;
    fs::write(
        completed_dir.join("events.jsonl"),
        "{\"event\":\"run_started\",\"run_id\":\"run-legacy-completed\",\"at_ms\":3}\\n\
         {\"event\":\"run_finished\",\"run_id\":\"run-legacy-completed\",\"at_ms\":4}\\n",
    )?;

    let failed_dir = super::super::storage::run_dir(&root, "run-legacy-failed");
    fs::create_dir_all(&failed_dir)?;
    fs::write(
        failed_dir.join("manifest.json"),
        fixtures::json_bytes(&json!({
            "kind": "workflow_run",
            "run_id": "run-legacy-failed",
            "started_at_ms": 5,
            "completed_at_ms": 6,
            "stages": [{"workflow_id": "lightflow.legacy"}]
        }))?,
    )?;
    fs::write(failed_dir.join("execution.json"), "{}")?;
    fs::write(
        failed_dir.join("events.jsonl"),
        "{\"event\":\"run_started\",\"run_id\":\"run-legacy-failed\",\"at_ms\":5}\\n\
         {\"event\":\"run_failed\",\"run_id\":\"run-legacy-failed\",\"at_ms\":6}\\n",
    )?;

    let catalog = list_runs(&root)?;

    assert_eq!(catalog.total, 3);
    assert_eq!(catalog.completed_count, 2);
    assert_eq!(catalog.failed_count, 1);
    assert_eq!(catalog.unknown_count, 0);
    assert!(catalog.unknown_run_ids.is_empty());
    assert_eq!(
        catalog
            .runs
            .iter()
            .find(|run| run.run_id == "run-legacy-completed")
            .expect("legacy completed")
            .status,
        "completed"
    );
    assert_eq!(
        catalog
            .runs
            .iter()
            .find(|run| run.run_id == "run-legacy-failed")
            .expect("legacy failed")
            .status,
        "failed"
    );

    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn list_runs_can_limit_and_filter_summaries() -> Result<(), Box<dyn std::error::Error>> {
    let root = temp_root();
    write_history_fixture(&root)?;

    let failed_dir = super::super::storage::run_dir(&root, "run-failed");
    fs::create_dir_all(&failed_dir)?;
    fs::write(
        failed_dir.join("manifest.json"),
        fixtures::json_bytes(&json!({
            "kind": "workflow_run",
            "run_id": "run-failed",
            "status": "failed",
            "started_at_ms": 3,
            "completed_at_ms": 4,
            "stages": [{"workflow_id": "lightflow.other"}]
        }))?,
    )?;
    fs::write(failed_dir.join("execution.json"), "{}")?;
    fs::write(failed_dir.join("events.jsonl"), "")?;

    let completed_dir = super::super::storage::run_dir(&root, "run-completed-new");
    fs::create_dir_all(&completed_dir)?;
    fs::write(
        completed_dir.join("manifest.json"),
        fixtures::json_bytes(&json!({
            "kind": "workflow_run",
            "run_id": "run-completed-new",
            "status": "completed",
            "started_at_ms": 5,
            "completed_at_ms": 6,
            "stages": [{"workflow_id": "lightflow.other"}]
        }))?,
    )?;
    fs::write(completed_dir.join("execution.json"), "{}")?;
    fs::write(completed_dir.join("events.jsonl"), "")?;

    let limited = list_runs_with_options(
        &root,
        &RunListOptions {
            limit: Some(1),
            ..RunListOptions::default()
        },
    )?;
    assert_eq!(limited.total, 1);
    assert_eq!(limited.runs[0].run_id, "run-completed-new");

    let failed = list_runs_with_options(
        &root,
        &RunListOptions {
            status: Some("failed".to_owned()),
            ..RunListOptions::default()
        },
    )?;
    assert_eq!(failed.total, 1);
    assert_eq!(failed.failed_count, 1);
    assert_eq!(failed.runs[0].run_id, "run-failed");

    let workflow = list_runs_with_options(
        &root,
        &RunListOptions {
            workflow_id: Some("lightflow.fixture".to_owned()),
            ..RunListOptions::default()
        },
    )?;
    assert_eq!(workflow.total, 1);
    assert_eq!(workflow.runs[0].run_id, "run-test");

    let _ = fs::remove_dir_all(root);
    Ok(())
}
