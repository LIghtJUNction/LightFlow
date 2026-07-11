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
fn lfw_loop_projects_reports_stale_parent_gitlinks() -> Result<(), Box<dyn std::error::Error>> {
    let base = unique_temp_root();
    let root = base.join("core");
    let std = root.join("projects/lightflow-std");
    fs::create_dir_all(&std)?;
    fs::write(root.join("README.md"), "# core\n")?;

    lfw(&std, ["init"])?;
    complete_generated_workflow_metadata(&std, "example")?;
    git_ok(&std, ["init"])?;
    git_ok(&std, ["add", "."])?;
    git_ok(
        &std,
        [
            "-c",
            "user.name=LightFlow Test",
            "-c",
            "user.email=lightflow@example.test",
            "commit",
            "-m",
            "initial std",
        ],
    )?;

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
            "initial parent gitlink",
        ],
    )?;

    fs::write(std.join("README.md"), "# updated std\n")?;
    git_ok(&std, ["add", "."])?;
    git_ok(
        &std,
        [
            "-c",
            "user.name=LightFlow Test",
            "-c",
            "user.email=lightflow@example.test",
            "commit",
            "-m",
            "update std",
        ],
    )?;

    let report = lfw(
        &root,
        ["loop", "projects", "--dirty", "--project", "lightflow-std"],
    )?;
    let workspace = &report["workspaces"][0];
    assert_eq!(workspace["name"], "lightflow-std");
    assert_eq!(workspace["git_dirty"], false);
    assert_eq!(workspace["parent_gitlink_changed"], true);
    assert_ne!(workspace["parent_gitlink_head"], workspace["git_head"]);
    assert_eq!(
        workspace["parent_gitlink_stage_command"],
        serde_json::json!(["git", "add", "projects/lightflow-std"])
    );

    let _ = fs::remove_dir_all(base);
    Ok(())
}
