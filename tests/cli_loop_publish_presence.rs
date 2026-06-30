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
fn lfw_loop_check_uses_project_workspaces_for_publish_crate_presence()
-> Result<(), Box<dyn std::error::Error>> {
    let base = unique_temp_root();
    let root = base.join("core");
    let std = root.join("projects/lightflow-std");
    fs::create_dir_all(&root)?;
    fs::create_dir_all(&std)?;

    lfw(&root, ["init"])?;
    fs::remove_dir_all(root.join(".lightflow/workflows"))?;
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
    lfw(&std, ["init"])?;
    complete_generated_workflow_metadata(&std, "examples", "example")?;
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

    let loop_check = lfw(&root, ["loop", "check"])?;
    let checks = loop_check["checks"].as_array().expect("loop checks");
    let publish_crates = checks
        .iter()
        .find(|check| check["id"] == "loop.publish.workflow_crates")
        .expect("publish crate presence check");
    assert_eq!(publish_crates["status"], "passed");
    assert_eq!(publish_crates["count"], 1);
    assert!(
        checks.iter().all(|check| {
            check["message"]
                .as_str()
                .is_none_or(|message| !message.contains("no workflow crates found"))
        }),
        "loop checks:\n{checks:#?}"
    );

    let _ = fs::remove_dir_all(base);
    Ok(())
}
