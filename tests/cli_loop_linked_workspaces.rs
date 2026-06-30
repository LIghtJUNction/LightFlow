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
#[cfg(unix)]
fn lfw_loop_changes_checks_linked_project_workspaces() -> Result<(), Box<dyn std::error::Error>> {
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
    lfw(&sibling, ["new", "linked", "--category", "examples"])?;
    git_ok(&sibling, ["init"])?;
    git_ok(&sibling, ["add", "."])?;
    git_ok(
        &sibling,
        [
            "-c",
            "user.name=LightFlow Test",
            "-c",
            "user.email=lightflow@example.test",
            "commit",
            "-m",
            "initial sibling workflow",
        ],
    )?;

    let projects = root.join("projects");
    fs::create_dir_all(&projects)?;
    std::os::unix::fs::symlink(&sibling, projects.join("lightflow-std"))?;

    let source_path = sibling.join(".lightflow/workflows/examples/linked/src/lib.rs");
    fs::write(
        &source_path,
        fs::read_to_string(&source_path)? + "\n// linked behavior change\n",
    )?;
    let missing_skill = lfw_command(&root).args(["loop", "changes"]).output()?;
    assert!(!missing_skill.status.success());
    let stderr = String::from_utf8_lossy(&missing_skill.stderr);
    assert!(stderr.contains("\"valid\":false"), "stderr:\n{stderr}");
    assert!(
        stderr.contains("lightflow-std:examples/linked"),
        "stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("projects/lightflow-std/.lightflow/workflows/examples/linked/src/lib.rs"),
        "stderr:\n{stderr}"
    );

    let skill_path = sibling
        .join(".lightflow/workflows/examples/linked/.agent/skills/lightflow-linked/SKILL.md");
    fs::write(
        &skill_path,
        fs::read_to_string(&skill_path)? + "\nReview note: linked behavior changed.\n",
    )?;
    let paired = lfw(&root, ["loop", "changes"])?;
    assert_eq!(paired["valid"], true);
    assert_eq!(
        paired["changed_workflows"][0]["workflow_key"],
        "lightflow-std:examples/linked"
    );
    assert_eq!(
        paired["changed_workflows"][0]["workflow_paths"][0],
        "projects/lightflow-std/.lightflow/workflows/examples/linked/src/lib.rs"
    );
    assert_eq!(paired["changed_workflows"][0]["status"], "passed");

    fs::remove_file(&skill_path)?;
    let missing_skill = lfw_command(&root).args(["loop", "check"]).output()?;
    assert!(!missing_skill.status.success());
    let stderr = String::from_utf8_lossy(&missing_skill.stderr);
    assert!(
        stderr.contains("loop.workflow.agent_skills"),
        "stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("projects/lightflow-std/.lightflow/workflows/examples/linked"),
        "stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("missing") || stderr.contains("no SKILL.md"),
        "stderr:\n{stderr}"
    );

    let _ = fs::remove_dir_all(base);
    Ok(())
}

#[test]
#[cfg(unix)]
fn lfw_loop_changes_checks_extra_linked_project_workspaces()
-> Result<(), Box<dyn std::error::Error>> {
    let base = unique_temp_root();
    let root = base.join("core");
    let sibling = base.join("custom-workflows");
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
    lfw(&sibling, ["new", "extra", "--category", "examples"])?;
    git_ok(&sibling, ["init"])?;
    git_ok(&sibling, ["add", "."])?;
    git_ok(
        &sibling,
        [
            "-c",
            "user.name=LightFlow Test",
            "-c",
            "user.email=lightflow@example.test",
            "commit",
            "-m",
            "initial extra workflow",
        ],
    )?;

    let projects = root.join("projects");
    fs::create_dir_all(&projects)?;
    std::os::unix::fs::symlink(&sibling, projects.join("custom-workflows"))?;

    let source_path = sibling.join(".lightflow/workflows/examples/extra/src/lib.rs");
    fs::write(
        &source_path,
        fs::read_to_string(&source_path)? + "\n// extra linked behavior change\n",
    )?;
    let missing_skill = lfw_command(&root).args(["loop", "changes"]).output()?;
    assert!(!missing_skill.status.success());
    let stderr = String::from_utf8_lossy(&missing_skill.stderr);
    assert!(
        stderr.contains("custom-workflows:examples/extra"),
        "stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("projects/custom-workflows/.lightflow/workflows/examples/extra/src/lib.rs"),
        "stderr:\n{stderr}"
    );
    let publish_catalog = serde_json::to_value(ApiService::new(&root).workflow_publish_checks()?)?;
    let extra_publish_check = publish_catalog["checks"]
        .as_array()
        .expect("publish checks")
        .iter()
        .find(|check| {
            check["manifest"].as_str().is_some_and(|manifest| {
                manifest.contains(
                    "projects/custom-workflows/.lightflow/workflows/examples/extra/Cargo.toml",
                )
            })
        })
        .expect("extra linked workflow publish check");
    assert_eq!(
        extra_publish_check["workspace"],
        "projects/custom-workflows"
    );
    assert_eq!(extra_publish_check["publishable"], false);
    let publish_plan = lfw(&root, ["publish", "--workflows"])?;
    assert_eq!(publish_plan["total"], 2);
    assert_eq!(publish_plan["publishable_count"], 0);
    assert_eq!(publish_plan["blocked_count"], 2);
    let extra_publish_plan = publish_plan["crates"]
        .as_array()
        .expect("publish plan crates")
        .iter()
        .find(|plan| {
            plan["manifest"].as_str().is_some_and(|manifest| {
                manifest.contains(
                    "projects/custom-workflows/.lightflow/workflows/examples/extra/Cargo.toml",
                )
            })
        })
        .expect("extra linked workflow publish plan");
    assert_eq!(extra_publish_plan["workspace"], "projects/custom-workflows");

    let _ = fs::remove_dir_all(base);
    Ok(())
}
