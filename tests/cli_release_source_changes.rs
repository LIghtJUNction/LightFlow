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
fn lfw_release_check_dry_run_reports_source_change_blockers()
-> Result<(), Box<dyn std::error::Error>> {
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
    lfw(&root, ["new", "reviewed", "--category", "examples"])?;
    complete_generated_workflow_metadata(&root, "examples", "example")?;
    complete_generated_workflow_metadata(&root, "examples", "reviewed")?;
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

    let source_path = root.join(".lightflow/workflows/examples/reviewed/src/lib.rs");
    fs::write(
        &source_path,
        fs::read_to_string(&source_path)? + "\n// release-blocking behavior change\n",
    )?;

    lfw(&root, ["run", "lightflow.example", "-i", "value={}"])?;
    let report = lfw(
        &root,
        ["release", "check", "--workflow", "lightflow.example"],
    )?;
    assert_eq!(report["valid"], false);
    assert!(
        report["issues"][0]
            .as_str()
            .expect("release issue")
            .contains("release.review.workflow_change_skills")
    );
    let checks = report["checks"].as_array().expect("release checks");
    let review_check = checks
        .iter()
        .find(|check| check["id"] == "release.review.workflow_change_skills")
        .expect("source change review check");
    assert_eq!(review_check["kind"], "review");
    assert_eq!(review_check["status"], "failed");
    assert!(
        review_check["message"]
            .as_str()
            .expect("review message")
            .contains("workflow source changes need colocated agent skill updates"),
        "review check:\n{review_check:#?}"
    );
    assert!(
        review_check["message"]
            .as_str()
            .expect("review message")
            .contains(
                "examples/reviewed: workflow files changed without a colocated agent skill update"
            ),
        "review check:\n{review_check:#?}"
    );

    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn lfw_release_check_dry_run_accepts_skill_only_source_changes()
-> Result<(), Box<dyn std::error::Error>> {
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
    lfw(&root, ["new", "reviewed", "--category", "examples"])?;
    complete_generated_workflow_metadata(&root, "examples", "example")?;
    complete_generated_workflow_metadata(&root, "examples", "reviewed")?;
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

    let skill_path = root
        .join(".lightflow/workflows/examples/reviewed/.agent/skills/lightflow-reviewed/SKILL.md");
    fs::write(
        &skill_path,
        fs::read_to_string(&skill_path)? + "\nReview note: skill docs clarified.\n",
    )?;

    lfw(&root, ["run", "lightflow.example", "-i", "value={}"])?;
    let report = lfw(
        &root,
        ["release", "check", "--workflow", "lightflow.example"],
    )?;
    assert_eq!(report["valid"], true, "release report:\n{report:#?}");
    assert_eq!(
        report["issues"],
        serde_json::json!([]),
        "release report:\n{report:#?}"
    );
    assert!(
        !report["warnings"]
            .as_array()
            .expect("release warnings")
            .iter()
            .any(|warning| {
                warning
                    .as_str()
                    .unwrap_or_default()
                    .contains("release.review.workflow_change_skills")
            }),
        "release report:\n{report:#?}"
    );
    let checks = report["checks"].as_array().expect("release checks");
    let review_check = checks
        .iter()
        .find(|check| check["id"] == "release.review.workflow_change_skills")
        .expect("source change review check");
    assert_eq!(review_check["kind"], "review");
    assert_eq!(review_check["status"], "passed");
    assert_eq!(review_check["count"], 1);
    assert!(
        review_check["message"]
            .as_str()
            .expect("review message")
            .contains("source-change safety passed"),
        "review check:\n{review_check:#?}"
    );

    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn lfw_release_check_apply_skips_commands_after_source_change_blockers()
-> Result<(), Box<dyn std::error::Error>> {
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
    lfw(&root, ["new", "reviewed", "--category", "examples"])?;
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

    let source_path = root.join(".lightflow/workflows/examples/reviewed/src/lib.rs");
    fs::write(
        &source_path,
        fs::read_to_string(&source_path)? + "\n// apply-blocking behavior change\n",
    )?;

    let output = lfw_command(&root)
        .args(["release", "check", "--apply"])
        .output()?;
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("\"id\":\"release.review.workflow_change_skills\""),
        "stderr:\n{stderr}"
    );
    assert!(stderr.contains("\"kind\":\"review\""), "stderr:\n{stderr}");
    assert!(
        stderr.contains("\"status\":\"failed\""),
        "stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("\"id\":\"release.command.fmt\""),
        "stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("\"status\":\"skipped\""),
        "stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("command skipped because an earlier release gate failed"),
        "stderr:\n{stderr}"
    );

    let _ = fs::remove_dir_all(root);
    Ok(())
}
