#![allow(unused_imports)]
mod cli_project_support;
#[path = "cli_project_support/dirty_workspaces.rs"]
mod dirty_workspaces;
mod support;
use cli_project_support::*;
use dirty_workspaces::dirty_project_workspace_fixture;
use lightflow::api::{ApiService, WorkflowPublishOptions};
use serde_json::Value;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::process::Command;
use support::*;
#[test]
fn lfw_loop_projects_reports_dirty_git_workspaces() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = dirty_project_workspace_fixture()?;
    let base = fixture.base;
    let root = fixture.root;
    let branch_name = fixture.branch_name;
    let report = lfw(&root, ["loop", "projects"])?;
    assert_eq!(report["dirty_filter"], false);
    let std_workspace = report["workspaces"]
        .as_array()
        .expect("workspaces")
        .iter()
        .find(|workspace| workspace["name"] == "lightflow-std")
        .expect("lightflow-std workspace");
    assert_eq!(std_workspace["git_dirty"], true);
    assert_eq!(std_workspace["git_changed_count"], 1);
    assert_eq!(
        std_workspace["git_changed_paths"],
        serde_json::json!(["README.md"])
    );
    assert!(
        std_workspace["git_branch"]
            .as_str()
            .is_some_and(|branch| !branch.is_empty())
    );
    assert_eq!(
        std_workspace["git_upstream"],
        format!("origin/{branch_name}")
    );
    assert_eq!(
        std_workspace["git_remote_url"],
        "https://example.test/lightflow-std.git"
    );
    assert!(
        std_workspace["git_head"]
            .as_str()
            .is_some_and(|head| !head.is_empty())
    );
    assert_eq!(
        std_workspace["git_status_command"],
        serde_json::json!(["git", "-C", "projects/lightflow-std", "status", "--short"])
    );
    assert_eq!(
        std_workspace["git_stage_command"],
        serde_json::json!(["git", "-C", "projects/lightflow-std", "add", "."])
    );
    assert_eq!(
        std_workspace["git_commit_command"],
        serde_json::json!([
            "git",
            "-C",
            "projects/lightflow-std",
            "commit",
            "-m",
            "<message>"
        ])
    );
    assert_eq!(
        std_workspace["git_push_command"],
        serde_json::json!(["git", "-C", "projects/lightflow-std", "push"])
    );
    assert_eq!(std_workspace.get("git_status_error"), None);
    let dirty_report = lfw(&root, ["loop", "projects", "--dirty"])?;
    assert_eq!(dirty_report["dirty_filter"], true);
    assert_eq!(
        dirty_report["known_workspace_names"],
        serde_json::json!(["lightflow-flux", "lightflow-rig", "lightflow-std"])
    );
    assert_eq!(
        dirty_report["known_project_workspaces"],
        dirty_report["known_workspace_names"]
    );
    assert_eq!(
        dirty_report["known_workspace_aliases"]["std"],
        "lightflow-std"
    );
    assert_eq!(
        dirty_report["known_project_aliases"],
        dirty_report["known_workspace_aliases"]
    );
    assert_eq!(dirty_report["present_count"], 1);
    assert_eq!(dirty_report["linked_count"], 1);
    assert_eq!(dirty_report["workflow_crate_count"], 1);
    let dirty_workspaces = dirty_report["workspaces"]
        .as_array()
        .expect("dirty workspaces");
    assert_eq!(dirty_workspaces.len(), 1);
    assert_eq!(dirty_workspaces[0]["name"], "lightflow-std");
    assert_eq!(dirty_workspaces[0]["aliases"], serde_json::json!(["std"]));
    assert_eq!(
        dirty_workspaces[0]["git_changed_paths"],
        serde_json::json!(["README.md"])
    );
    let std_report = lfw(&root, ["loop", "projects", "--project", "lightflow-std"])?;
    assert_eq!(std_report["present_count"], 1);
    assert_eq!(std_report["workspaces"][0]["name"], "lightflow-std");
    assert_eq!(
        std_report["workspaces"][0]["git_changed_paths"],
        serde_json::json!(["README.md"])
    );
    let std_alias_report = lfw(&root, ["loop", "projects", "--project", "std"])?;
    assert_eq!(std_alias_report["present_count"], 1);
    assert_eq!(std_alias_report["project_filter"], "std");
    assert_eq!(std_alias_report["project_filter_matched"], true);
    assert_eq!(
        std_alias_report["matched_project_workspace"],
        "lightflow-std"
    );
    assert_eq!(std_alias_report["workspaces"][0]["name"], "lightflow-std");
    let mcp_dirty_projects = lfw(
        &root,
        [
            "mcp",
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"lightflow.loop.projects","arguments":{"dirty":true,"project":"projects/lightflow-std"}}}"#,
        ],
    )?;
    let mcp_dirty_text = mcp_dirty_projects["result"]["content"][0]["text"]
        .as_str()
        .expect("mcp dirty projects text");
    let mcp_dirty_report: serde_json::Value = serde_json::from_str(mcp_dirty_text)?;
    assert_eq!(mcp_dirty_report["present_count"], 1);
    assert_eq!(mcp_dirty_report["workspaces"][0]["name"], "lightflow-std");
    let unknown_project = lfw_command(&root)
        .args(["loop", "projects", "--project", "lightflow-typo"])
        .output()?;
    assert!(!unknown_project.status.success());
    let stderr = String::from_utf8_lossy(&unknown_project.stderr);
    assert!(
        stderr.contains("project workspace filter matched no workspace: lightflow-typo"),
        "stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("known workspaces:")
            && stderr.contains("lightflow-flux")
            && stderr.contains("lightflow-rig")
            && stderr.contains("lightflow-std"),
        "stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("known aliases:")
            && stderr.contains("flux=lightflow-flux")
            && stderr.contains("rig=lightflow-rig")
            && stderr.contains("std=lightflow-std"),
        "stderr:\n{stderr}"
    );
    let loop_check = lfw(&root, ["loop", "check"])?;
    let checks = loop_check["checks"].as_array().expect("loop checks");
    let git_status = checks
        .iter()
        .find(|check| check["id"] == "loop.projects.git_status")
        .expect("project git status check");
    assert_eq!(git_status["status"], "warning");
    assert_eq!(git_status["count"], 1);
    assert!(
        git_status["details"]
            .as_array()
            .expect("details")
            .iter()
            .any(|detail| {
                detail.as_str().is_some_and(|detail| {
                    detail.contains("projects/lightflow-std has 1 changed path")
                })
            }),
        "git status check:\n{git_status:#?}"
    );
    let dev_check = lfw(&root, ["dev", "check"])?;
    let project_review = dev_check["checks"]
        .as_array()
        .expect("dev checks")
        .iter()
        .find(|check| check["id"] == "release.review.project_workspaces")
        .expect("project workspace review");
    assert_eq!(project_review["status"], "warning");
    assert!(
        project_review["details"]
            .as_array()
            .expect("project review details")
            .iter()
            .any(|detail| {
                detail.as_str().is_some_and(|detail| {
                    detail.contains("git -C projects/lightflow-std status --short")
                })
            }),
        "project review:\n{project_review:#?}"
    );
    assert!(
        project_review["details"]
            .as_array()
            .expect("project review details")
            .iter()
            .any(|detail| {
                detail.as_str().is_some_and(|detail| {
                    detail.contains("git -C projects/lightflow-std commit -m <message>")
                })
            }),
        "project review:\n{project_review:#?}"
    );
    let scoped_dev_check = lfw(&root, ["dev", "check", "--project", "lightflow-std"])?;
    assert_eq!(scoped_dev_check["project"], "lightflow-std");
    assert_eq!(scoped_dev_check["project_filter_matched"], true);
    assert_eq!(scoped_dev_check["project_config_present"], false);
    assert_eq!(scoped_dev_check["project_config_valid"], true);
    assert_eq!(
        scoped_dev_check["project_config_error"],
        serde_json::Value::Null
    );
    assert_eq!(
        scoped_dev_check["project_config_path"],
        root.join("projects/lightflow-projects.toml")
            .to_string_lossy()
            .as_ref()
    );
    assert_eq!(
        scoped_dev_check["project_config_template_command"],
        serde_json::json!(["lfw", "dev", "project-config-template"])
    );
    assert_eq!(
        scoped_dev_check["project_config_write_command"],
        serde_json::json!(["lfw", "dev", "project-config-template", "--write"])
    );
    assert_eq!(
        scoped_dev_check["project_submodule_update_command"],
        serde_json::json!([
            "git",
            "submodule",
            "update",
            "--init",
            "--recursive",
            "projects/lightflow-flux",
            "projects/lightflow-rig",
            "projects/lightflow-std"
        ])
    );
    assert_eq!(
        scoped_dev_check["default_workflow_sources"],
        serde_json::json!(["lightflow-std"])
    );
    assert_eq!(
        scoped_dev_check["known_optional_workspace_names"],
        serde_json::json!([])
    );
    assert_eq!(
        scoped_dev_check["known_project_workspaces"],
        serde_json::json!(["lightflow-flux", "lightflow-rig", "lightflow-std"])
    );
    assert_eq!(
        scoped_dev_check["known_project_aliases"]["std"],
        "lightflow-std"
    );
    let scoped_checks = scoped_dev_check["checks"]
        .as_array()
        .expect("scoped dev checks");
    let scoped_project_review = scoped_checks
        .iter()
        .find(|check| check["id"] == "release.review.project_workspaces")
        .expect("scoped project review");
    assert_eq!(scoped_project_review["count"], 1);
    assert!(
        scoped_project_review["details"]
            .as_array()
            .expect("scoped project review details")
            .iter()
            .any(|detail| detail.as_str() == Some("project filter: lightflow-std")),
        "scoped project review:\n{scoped_project_review:#?}"
    );
    let scoped_publish_review = scoped_checks
        .iter()
        .find(|check| check["id"] == "release.review.workflow_publish_ready")
        .expect("scoped publish review");
    assert_eq!(scoped_publish_review["count"], 1);
    assert!(
        scoped_publish_review["details"]
            .as_array()
            .expect("scoped publish review details")
            .iter()
            .any(|detail| detail.as_str() == Some("workspace: projects/lightflow-std")),
        "scoped publish review:\n{scoped_publish_review:#?}"
    );
    assert!(scoped_checks.iter().any(|check| {
        check["id"] == "release.command.project_workspaces"
            && check["command"]
                == serde_json::json!([
                    "cargo",
                    "run",
                    "--bin",
                    "lfw",
                    "--",
                    "loop",
                    "projects",
                    "--project",
                    "lightflow-std"
                ])
    }));
    assert!(scoped_checks.iter().any(|check| {
        check["id"] == "release.command.dirty_project_workspaces"
            && check["command"]
                == serde_json::json!([
                    "cargo",
                    "run",
                    "--bin",
                    "lfw",
                    "--",
                    "loop",
                    "projects",
                    "--dirty",
                    "--project",
                    "lightflow-std"
                ])
    }));
    assert!(scoped_checks.iter().any(|check| {
        check["id"] == "release.command.workflow_publish_ready"
            && check["command"]
                == serde_json::json!([
                    "cargo",
                    "run",
                    "--bin",
                    "lfw",
                    "--",
                    "publish",
                    "--workflows",
                    "--require-publishable",
                    "--project",
                    "lightflow-std"
                ])
    }));
    let scoped_alias_dev_check = lfw(&root, ["dev", "check", "--project", "std"])?;
    assert_eq!(scoped_alias_dev_check["project"], "std");
    assert_eq!(scoped_alias_dev_check["project_filter_matched"], true);
    assert_eq!(
        scoped_alias_dev_check["matched_project_workspace"],
        "lightflow-std"
    );
    let scoped_alias_review = scoped_alias_dev_check["checks"]
        .as_array()
        .expect("scoped alias dev checks")
        .iter()
        .find(|check| check["id"] == "release.review.project_workspaces")
        .expect("scoped alias project review");
    assert_eq!(scoped_alias_review["count"], 1);
    assert!(
        scoped_alias_review["details"]
            .as_array()
            .expect("scoped alias project review details")
            .iter()
            .any(|detail| detail.as_str() == Some("projects/lightflow-std has 1 changed path(s)")),
        "scoped alias project review:\n{scoped_alias_review:#?}"
    );
    let scoped_path_dev_check = lfw(
        &root,
        [
            "dev",
            "check",
            "--project",
            root.join("projects/lightflow-std")
                .to_str()
                .expect("project path"),
        ],
    )?;
    assert_eq!(scoped_path_dev_check["project_filter_matched"], true);
    assert_eq!(
        scoped_path_dev_check["matched_project_workspace"],
        "lightflow-std"
    );
    let scoped_relative_path_dev_check = lfw(
        &root,
        ["dev", "check", "--project", "./projects/lightflow-std"],
    )?;
    assert_eq!(
        scoped_relative_path_dev_check["project"],
        "./projects/lightflow-std"
    );
    assert_eq!(
        scoped_relative_path_dev_check["project_filter_matched"],
        true
    );
    assert_eq!(
        scoped_relative_path_dev_check["matched_project_workspace"],
        "lightflow-std"
    );
    let scoped_relative_path_release_check = lfw(
        &root,
        ["release", "check", "--project", "./projects/lightflow-std"],
    )?;
    assert_eq!(
        scoped_relative_path_release_check["project"],
        "./projects/lightflow-std"
    );
    assert_eq!(
        scoped_relative_path_release_check["project_filter_matched"],
        true
    );
    assert_eq!(
        scoped_relative_path_release_check["matched_project_workspace"],
        "lightflow-std"
    );
    let unknown_scoped_dev_check = lfw(&root, ["dev", "check", "--project", "lightflow-typo"])?;
    assert_eq!(unknown_scoped_dev_check["valid"], false);
    assert_eq!(unknown_scoped_dev_check["project_filter_matched"], false);
    assert_eq!(
        unknown_scoped_dev_check.get("matched_project_workspace"),
        None
    );
    assert_eq!(
        unknown_scoped_dev_check["known_project_workspaces"],
        serde_json::json!(["lightflow-flux", "lightflow-rig", "lightflow-std"])
    );
    let unknown_project_review = unknown_scoped_dev_check["checks"]
        .as_array()
        .expect("unknown project checks")
        .iter()
        .find(|check| check["id"] == "release.review.project_workspaces")
        .expect("unknown project review");
    assert_eq!(unknown_project_review["status"], "failed");
    assert!(
        unknown_scoped_dev_check["issues"]
            .as_array()
            .expect("unknown project issues")
            .iter()
            .any(|issue| {
                issue.as_str().is_some_and(|issue| {
                    issue.contains("project workspace filter matched no workspace: lightflow-typo")
                        && issue.contains("known aliases:")
                        && issue.contains("std=lightflow-std")
                })
            }),
        "unknown project dev check:\n{unknown_scoped_dev_check:#?}"
    );
    let unknown_scoped_apply = lfw_command(&root)
        .args(["dev", "check", "--apply", "--project", "lightflow-typo"])
        .output()?;
    assert!(!unknown_scoped_apply.status.success());
    let apply_stderr = String::from_utf8_lossy(&unknown_scoped_apply.stderr);
    assert!(
        apply_stderr.contains("\"valid\":false")
            && apply_stderr.contains("\"status\":\"skipped\"")
            && apply_stderr.contains("command skipped because an earlier release gate failed"),
        "stderr:\n{apply_stderr}"
    );
    let _ = fs::remove_dir_all(base);
    Ok(())
}
