#![allow(unused_imports)]

mod cli_project_support;
mod support;

use cli_project_support::*;
use lightflow::api::{ApiService, WorkflowPublishOptions};
use serde_json::Value;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::process::Command;
use support::*;

#[test]
fn local_loop_agent_skill_failures_are_summarized() -> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    fs::create_dir_all(&root)?;
    lfw(&root, ["init"])?;
    complete_generated_workflow_metadata(&root, "examples", "example")?;

    for index in 0..7 {
        let name = format!("weak_skill_{index}");
        lfw(&root, ["new", &name, "--category", "examples"])?;
        complete_generated_workflow_metadata(&root, "examples", &name)?;
        fs::write(
            root.join(format!(
                ".lightflow/workflows/examples/{name}/.agent/skills/lightflow-weak-skill-{index}/SKILL.md"
            )),
            "# Weak skill\n\nThis file exists but does not describe how to run the workflow.\n",
        )?;
    }

    let report = ApiService::new(&root).local_loop_check(None)?;
    let check = report
        .checks
        .iter()
        .find(|check| check.id == "loop.workflow.agent_skills")
        .expect("agent skill check");
    assert_eq!(serde_json::to_value(check.status)?, "failed");
    assert_eq!(check.count, Some(7));
    assert_eq!(check.details.len(), 7);
    assert!(
        check.message.contains("and 2 more"),
        "agent skill message:\n{}",
        check.message
    );
    assert!(
        check
            .details
            .iter()
            .any(|detail| detail.contains("lightflow.weak_skill_6")),
        "agent skill details:\n{:#?}",
        check.details
    );

    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn lfw_loop_changes_requires_skill_update_with_workflow_edits()
-> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    fs::create_dir_all(&root)?;
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
        fs::read_to_string(&source_path)? + "\n// reviewed behavior change\n",
    )?;
    let missing_skill = lfw_command(&root).args(["loop", "changes"]).output()?;
    assert!(!missing_skill.status.success());
    let stderr = String::from_utf8_lossy(&missing_skill.stderr);
    assert!(stderr.contains("\"valid\":false"), "stderr:\n{stderr}");
    assert!(stderr.contains("\"blockers\":[\"examples/reviewed: workflow files changed without a colocated agent skill update\"]"), "stderr:\n{stderr}");
    assert!(
        stderr.contains("workflow files changed without a colocated agent skill update"),
        "stderr:\n{stderr}"
    );
    let unsafe_loop = lfw_command(&root).args(["loop", "check"]).output()?;
    assert!(!unsafe_loop.status.success());
    let unsafe_loop_stderr = String::from_utf8_lossy(&unsafe_loop.stderr);
    assert!(
        unsafe_loop_stderr.contains("loop.source_changes.safety"),
        "stderr:\n{unsafe_loop_stderr}"
    );
    assert!(
        unsafe_loop_stderr.contains("missing colocated agent skill updates"),
        "stderr:\n{unsafe_loop_stderr}"
    );
    let blocked_publish = lfw_command(&root)
        .args(["publish", "--workflows", "--apply"])
        .output()?;
    assert!(!blocked_publish.status.success());
    let publish_stderr = String::from_utf8_lossy(&blocked_publish.stderr);
    assert!(
        publish_stderr.contains("workflow files changed without a colocated agent skill update"),
        "stderr:\n{publish_stderr}"
    );
    assert!(
        publish_stderr.contains("\"valid\":false"),
        "stderr:\n{publish_stderr}"
    );

    let skill_path = root
        .join(".lightflow/workflows/examples/reviewed/.agent/skills/lightflow-reviewed/SKILL.md");
    fs::write(
        &skill_path,
        fs::read_to_string(&skill_path)? + "\nReview note: behavior changed with source.\n",
    )?;
    let paired = lfw(&root, ["loop", "changes"])?;
    assert_eq!(paired["valid"], true);
    assert_eq!(paired["passed"], 1);
    assert_eq!(paired["warnings"], 0);
    assert_eq!(paired["failed"], 0);
    assert_eq!(paired["blockers"], serde_json::json!([]));
    assert_eq!(
        paired["changed_workflows"][0]["workflow_key"],
        "examples/reviewed"
    );
    assert_eq!(paired["changed_workflows"][0]["workflow_changed"], true);
    assert_eq!(paired["changed_workflows"][0]["skill_changed"], true);
    assert_eq!(paired["changed_workflows"][0]["status"], "passed");

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
            "paired workflow and skill",
        ],
    )?;
    fs::write(
        &skill_path,
        fs::read_to_string(&skill_path)? + "\nReview note: skill docs clarified.\n",
    )?;
    let skill_only_loop = lfw(&root, ["loop", "check"])?;
    assert_eq!(skill_only_loop["valid"], true);
    let skill_only_checks = skill_only_loop["checks"].as_array().expect("loop checks");
    assert!(
        skill_only_checks.iter().any(|check| {
            check["id"] == "loop.source_changes.safety"
                && check["status"] == "passed"
                && check["message"].as_str().unwrap().contains(
                    "workflow source changes are paired with colocated agent skill updates",
                )
        }),
        "loop checks:\n{skill_only_checks:#?}"
    );
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
            "skill docs note",
        ],
    )?;
    lfw(
        &root,
        [
            "patch",
            "save",
            "qa-debug",
            r#"{"nodes":{"identity":{"disable":true}}}"#,
        ],
    )?;
    let patch_change = lfw(&root, ["loop", "changes"])?;
    assert_eq!(patch_change["valid"], true);
    assert_eq!(patch_change["passed"], 0);
    assert_eq!(patch_change["warnings"], 1);
    assert_eq!(patch_change["failed"], 0);
    assert_eq!(patch_change["blockers"], serde_json::json!([]));
    assert_eq!(
        patch_change["changed_workflows"][0]["workflow_key"],
        "patch:qa-debug"
    );
    assert_eq!(patch_change["changed_workflows"][0]["patch_changed"], true);
    assert_eq!(
        patch_change["changed_workflows"][0]["workflow_changed"],
        false
    );
    assert_eq!(patch_change["changed_workflows"][0]["skill_changed"], false);
    assert_eq!(patch_change["changed_workflows"][0]["status"], "warning");
    assert_eq!(
        patch_change["changed_workflows"][0]["patch_paths"][0],
        ".lightflow/patches/qa-debug.json"
    );

    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn lfw_loop_changes_tracks_untracked_workflow_files() -> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    fs::create_dir_all(&root)?;
    lfw(&root, ["init"])?;
    lfw(&root, ["new", "untracked", "--category", "examples"])?;
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

    let source_path = root.join(".lightflow/workflows/examples/untracked/src/extra.rs");
    fs::write(&source_path, "pub fn extra_behavior() {}\n")?;
    let missing_skill = lfw_command(&root).args(["loop", "changes"]).output()?;
    assert!(!missing_skill.status.success());
    let stderr = String::from_utf8_lossy(&missing_skill.stderr);
    assert!(stderr.contains("\"valid\":false"), "stderr:\n{stderr}");
    assert!(
        stderr.contains("workflow files changed without a colocated agent skill update"),
        "stderr:\n{stderr}"
    );
    assert!(stderr.contains("extra.rs"), "stderr:\n{stderr}");

    let skill_path = root
        .join(".lightflow/workflows/examples/untracked/.agent/skills/lightflow-untracked/SKILL.md");
    fs::write(
        &skill_path,
        fs::read_to_string(&skill_path)? + "\nReview note: extra source file added.\n",
    )?;
    let paired = lfw(&root, ["loop", "changes"])?;
    assert_eq!(paired["valid"], true);
    assert_eq!(
        paired["changed_workflows"][0]["workflow_key"],
        "examples/untracked"
    );
    assert_eq!(paired["changed_workflows"][0]["workflow_changed"], true);
    assert_eq!(paired["changed_workflows"][0]["skill_changed"], true);
    assert_eq!(paired["changed_workflows"][0]["status"], "passed");

    let _ = fs::remove_dir_all(root);
    Ok(())
}
