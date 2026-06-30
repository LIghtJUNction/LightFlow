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
fn lfw_publish_workflows_dedupes_linked_workspace_duplicates()
-> Result<(), Box<dyn std::error::Error>> {
    let base = unique_temp_root();
    let root = base.join("core");
    let sibling = base.join("lightflow-std");
    fs::create_dir_all(&root)?;
    fs::create_dir_all(&sibling)?;

    lfw(&root, ["init"])?;
    lfw(&sibling, ["init"])?;
    complete_generated_workflow_metadata(&root, "examples", "example")?;
    complete_generated_workflow_metadata(&sibling, "examples", "example")?;

    let projects = root.join("projects");
    fs::create_dir_all(&projects)?;
    std::os::unix::fs::symlink(&sibling, projects.join("lightflow-std"))?;

    let api_catalog = serde_json::to_value(ApiService::new(&root).workflow_publish_checks()?)?;
    let cli_plan = lfw(&root, ["publish", "--workflows"])?;
    assert_eq!(api_catalog["total"], 1);
    assert_eq!(cli_plan["total"], api_catalog["total"]);
    assert_eq!(
        cli_plan["publishable_count"],
        api_catalog["publishable_count"]
    );
    assert_eq!(cli_plan["blocked_count"], api_catalog["blocked_count"]);
    assert_eq!(cli_plan["crates"].as_array().expect("crates").len(), 1);
    assert_eq!(cli_plan["crates"][0]["workflow_id"], "lightflow.example");
    assert_eq!(cli_plan["crates"][0]["workspace"], "root");
    assert!(
        !cli_plan["crates"][0]["manifest"]
            .as_str()
            .expect("manifest")
            .contains("projects/lightflow-std"),
        "cli plan:\n{cli_plan:#?}"
    );

    let scoped_plan = lfw(
        &root,
        ["publish", "--workflows", "--project", "lightflow-std"],
    )?;
    assert_eq!(scoped_plan["project"], "lightflow-std");
    assert_eq!(scoped_plan["project_filter_matched"], true);
    assert_eq!(scoped_plan["total"], 1);
    assert_eq!(scoped_plan["crates"][0]["workflow_id"], "lightflow.example");
    assert_eq!(
        scoped_plan["crates"][0]["workspace"],
        "projects/lightflow-std"
    );
    assert!(
        scoped_plan["crates"][0]["manifest"]
            .as_str()
            .expect("manifest")
            .contains("projects/lightflow-std"),
        "scoped plan:\n{scoped_plan:#?}"
    );

    let scoped_api_catalog = serde_json::to_value(
        ApiService::new(&root).workflow_publish_checks_with_options(&WorkflowPublishOptions {
            project: Some("lightflow-std".to_owned()),
        })?,
    )?;
    assert_eq!(scoped_api_catalog["project"], "lightflow-std");
    assert_eq!(scoped_api_catalog["project_filter_matched"], true);
    assert_eq!(
        scoped_api_catalog["matched_project_workspace"],
        "lightflow-std"
    );
    assert_eq!(scoped_api_catalog["total"], 1);
    assert_eq!(
        scoped_api_catalog["checks"][0]["workspace"],
        "projects/lightflow-std"
    );

    let scoped_mcp = lfw(
        &root,
        [
            "mcp",
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"lightflow.workflow.publish_list","arguments":{"project":"std"}}}"#,
        ],
    )?;
    let scoped_mcp_text = scoped_mcp["result"]["content"][0]["text"]
        .as_str()
        .expect("scoped publish list mcp text");
    let scoped_mcp_catalog: serde_json::Value = serde_json::from_str(scoped_mcp_text)?;
    assert_eq!(scoped_mcp_catalog["project"], "std");
    assert_eq!(
        scoped_mcp_catalog["matched_project_workspace"],
        "lightflow-std"
    );
    assert_eq!(scoped_mcp_catalog["total"], 1);
    assert_eq!(
        scoped_mcp_catalog["checks"][0]["workspace"],
        "projects/lightflow-std"
    );

    let scoped_alias_plan = lfw(&root, ["publish", "--workflows", "--project", "std"])?;
    assert_eq!(scoped_alias_plan["project"], "std");
    assert_eq!(scoped_alias_plan["project_filter_matched"], true);
    assert_eq!(
        scoped_alias_plan["crates"][0]["workspace"],
        "projects/lightflow-std"
    );

    let scoped_label_plan = lfw(
        &root,
        [
            "publish",
            "--workflows",
            "--project",
            "projects/lightflow-std",
        ],
    )?;
    assert_eq!(scoped_label_plan["project"], "projects/lightflow-std");
    assert_eq!(scoped_label_plan["project_filter_matched"], true);
    assert_eq!(
        scoped_label_plan["crates"][0]["workspace"],
        "projects/lightflow-std"
    );
    let scoped_relative_path_plan = lfw(
        &root,
        [
            "publish",
            "--workflows",
            "--project",
            "./projects/lightflow-std",
        ],
    )?;
    assert_eq!(
        scoped_relative_path_plan["project"],
        "./projects/lightflow-std"
    );
    assert_eq!(scoped_relative_path_plan["project_filter_matched"], true);
    assert_eq!(
        scoped_relative_path_plan["crates"][0]["workspace"],
        "projects/lightflow-std"
    );

    let project_path = root.join("projects/lightflow-std");
    let scoped_path_plan = lfw(
        &root,
        [
            "publish",
            "--workflows",
            "--project",
            project_path.to_str().expect("project path"),
        ],
    )?;
    assert_eq!(
        scoped_path_plan["project"],
        project_path.to_str().expect("project path")
    );
    assert_eq!(scoped_path_plan["project_filter_matched"], true);
    assert_eq!(
        scoped_path_plan["crates"][0]["workspace"],
        "projects/lightflow-std"
    );

    let unknown_project = lfw_command(&root)
        .args(["publish", "--workflows", "--project", "lightflow-typo"])
        .output()?;
    assert!(!unknown_project.status.success());
    let unknown_stderr = String::from_utf8_lossy(&unknown_project.stderr);
    assert!(
        unknown_stderr.contains("project workspace filter matched no workspace: lightflow-typo"),
        "stderr:\n{unknown_stderr}"
    );

    let _ = fs::remove_dir_all(base);
    Ok(())
}

#[test]
#[cfg(unix)]
fn lfw_loop_check_fails_when_linked_project_cannot_be_inspected()
-> Result<(), Box<dyn std::error::Error>> {
    let base = unique_temp_root();
    let root = base.join("core");
    let sibling = base.join("lightflow-std");
    fs::create_dir_all(&root)?;
    fs::create_dir_all(&sibling)?;

    fs::write(root.join("README.md"), "# Core\n")?;
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
            "initial core",
        ],
    )?;

    lfw(&sibling, ["init"])?;
    let projects = root.join("projects");
    fs::create_dir_all(&projects)?;
    std::os::unix::fs::symlink(&sibling, projects.join("lightflow-std"))?;

    let output = lfw_command(&root).args(["loop", "check"]).output()?;
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("\"id\":\"loop.source_changes.safety\""),
        "stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("\"status\":\"failed\""),
        "stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("projects/lightflow-std: git status failed"),
        "stderr:\n{stderr}"
    );

    let _ = fs::remove_dir_all(base);
    Ok(())
}
