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
fn lfw_release_check_reports_release_gates_without_running_them()
-> Result<(), Box<dyn std::error::Error>> {
    let report = lfw(Path::new(env!("CARGO_MANIFEST_DIR")), ["release", "check"])?;
    assert_eq!(report["dry_run"], true);
    assert_eq!(report["valid"], true);
    assert_eq!(report["issues"], serde_json::json!([]));
    assert_eq!(report["workflow_id"], "lightflow.text_plan");
    assert_eq!(
        report["project_root"],
        Path::new(env!("CARGO_MANIFEST_DIR")).display().to_string()
    );

    let checks = report["checks"].as_array().expect("release checks");
    let passed = checks
        .iter()
        .filter(|check| check["status"] == "passed")
        .count();
    let warning_count = checks
        .iter()
        .filter(|check| check["status"] == "warning")
        .count();
    let failed = checks
        .iter()
        .filter(|check| check["status"] == "failed")
        .count();
    let planned = checks
        .iter()
        .filter(|check| check["status"] == "planned")
        .count();
    let skipped = checks
        .iter()
        .filter(|check| check["status"] == "skipped")
        .count();
    assert_eq!(report["passed"], passed);
    assert_eq!(report["warning_count"], warning_count);
    assert_eq!(report["failed"], failed);
    assert_eq!(report["planned"], planned);
    assert_eq!(report["skipped"], skipped);
    assert!(
        checks
            .iter()
            .any(|check| check["id"] == "release.artifact.changelog"
                && check["status"] == "passed"
                && check["path"] == "CHANGELOG.md")
    );
    assert!(checks.iter().any(
        |check| check["id"] == "release.artifact.v0_2_checklist" && check["status"] == "passed"
    ));
    assert!(checks.iter().any(
        |check| check["id"] == "release.artifact.runtime_verification"
            && check["status"] == "passed"
    ));
    assert!(checks.iter().any(
        |check| check["id"] == "release.artifact.local_workflow_loop"
            && check["status"] == "passed"
    ));
    for id in [
        "release.document.changelog_cli",
        "release.document.changelog_api",
        "release.document.changelog_workflows",
        "release.document.changelog_runtime",
        "release.document.changelog_known_limitations",
        "release.document.changelog_migration_notes",
        "release.document.local_workflow_loop",
    ] {
        assert!(
            checks
                .iter()
                .any(|check| check["id"] == id && check["status"] == "passed"),
            "missing passed release document check {id}"
        );
    }
    assert!(checks.iter().any(|check| {
        check["id"] == "release.review.workflow_change_skills"
            && check["kind"] == "review"
            && (check["status"] == "passed" || check["status"] == "warning")
    }));
    assert!(checks.iter().any(|check| {
        check["id"] == "release.review.local_workflow_loop"
            && check["kind"] == "review"
            && (check["status"] == "passed" || check["status"] == "warning")
            && check["count"].as_u64().is_some()
    }));
    assert!(checks.iter().any(|check| {
        check["id"] == "release.review.local_workflow_loop"
            && check["details"].as_array().is_some_and(|details| {
                details.iter().any(|detail| {
                    detail
                        .as_str()
                        .unwrap_or_default()
                        .contains("loop.models.readiness")
                })
            })
    }));
    assert!(checks.iter().any(|check| {
        check["id"] == "release.review.selected_workflow_loop"
            && check["kind"] == "review"
            && check["status"] == "passed"
            && check["message"]
                .as_str()
                .is_some_and(|message| message.contains("lightflow.text_plan"))
    }));
    assert!(checks.iter().any(|check| {
        check["id"] == "release.review.project_workspaces"
            && check["kind"] == "review"
            && (check["status"] == "passed" || check["status"] == "warning")
            && check["path"] == "projects"
    }));
    assert!(checks.iter().any(|check| {
        check["id"] == "release.review.workflow_publish_ready"
            && check["kind"] == "review"
            && check["status"] == "passed"
            && check["count"].as_u64().is_some_and(|count| count > 0)
            && check["message"]
                .as_str()
                .is_some_and(|message| message.contains("workflow crate"))
    }));

    let planned_commands = checks
        .iter()
        .filter(|check| check["kind"] == "command")
        .map(|check| check["command"].clone())
        .collect::<Vec<_>>();
    assert_eq!(
        planned_commands,
        vec![
            serde_json::json!(["cargo", "fmt", "--check"]),
            serde_json::json!(["cargo", "run", "--bin", "lfw", "--", "loop", "check"]),
            serde_json::json!([
                "cargo",
                "run",
                "--bin",
                "lfw",
                "--",
                "loop",
                "check",
                "lightflow.text_plan",
                "--require-replay"
            ]),
            serde_json::json!(["cargo", "run", "--bin", "lfw", "--", "loop", "changes"]),
            serde_json::json!(["cargo", "run", "--bin", "lfw", "--", "loop", "projects"]),
            serde_json::json!([
                "cargo", "run", "--bin", "lfw", "--", "loop", "projects", "--dirty"
            ]),
            serde_json::json!([
                "cargo",
                "run",
                "--bin",
                "lfw",
                "--",
                "publish",
                "--workflows",
                "--require-publishable"
            ]),
            serde_json::json!(["cargo", "clippy", "--all-targets", "--", "-D", "warnings"]),
            serde_json::json!(["cargo", "test"]),
            serde_json::json!([
                "cargo",
                "test",
                "--test",
                "standard_nodes",
                "repository_workflow_crates_have_agent_skills"
            ]),
            serde_json::json!(["cargo", "test", "--features", "rig", "--test", "llm_rig"]),
            serde_json::json!(["cargo", "check", "--features", "flux-native"]),
        ]
    );
    assert!(checks.iter().any(|check| check["status"] == "planned"));
    Ok(())
}

#[test]
fn lfw_dev_check_reports_developer_gate_plan_without_running_commands()
-> Result<(), Box<dyn std::error::Error>> {
    let report = lfw(
        Path::new(env!("CARGO_MANIFEST_DIR")),
        ["dev", "check", "--workflow", "lightflow.text_to_image"],
    )?;
    assert_eq!(report["profile"], "development");
    assert_eq!(report["dry_run"], true);
    assert_eq!(report["workflow_id"], "lightflow.text_to_image");

    let checks = report["checks"].as_array().expect("dev checks");
    assert!(checks.iter().all(|check| check["kind"] != "artifact"));
    assert!(checks.iter().all(|check| check["kind"] != "document"));
    let local_loop_review = checks
        .iter()
        .find(|check| check["id"] == "release.review.local_workflow_loop")
        .expect("local loop review");
    let local_loop_details = local_loop_review["details"]
        .as_array()
        .expect("local loop review details");
    if local_loop_review["status"] == "failed" {
        assert!(
            local_loop_details.len() > 1,
            "local loop review details:\n{local_loop_details:#?}"
        );
        assert!(
            local_loop_details.iter().any(|detail| detail
                .as_str()
                .unwrap_or_default()
                .contains("HTTP `/workflows/")),
            "local loop review details:\n{local_loop_details:#?}"
        );
    }
    assert!(
        checks
            .iter()
            .any(|check| check["id"] == "release.command.fmt" && check["status"] == "planned")
    );
    assert!(
        checks
            .iter()
            .any(|check| check["id"] == "release.command.clippy" && check["status"] == "planned")
    );
    assert!(
        checks
            .iter()
            .any(|check| check["id"] == "release.command.test" && check["status"] == "planned")
    );
    Ok(())
}
