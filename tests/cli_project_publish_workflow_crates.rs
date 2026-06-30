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
fn lfw_publish_plans_publishable_workflow_crates() -> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    fs::create_dir_all(&root)?;
    lfw(&root, ["init"])?;

    let workflow_plan = lfw(&root, ["publish", "lightflow.example"])?;
    assert_eq!(workflow_plan["dry_run"], true);
    assert_eq!(workflow_plan["target"]["workflow_id"], "lightflow.example");
    assert_eq!(workflow_plan["package"], "lightflow-example");
    assert_eq!(workflow_plan["version"], "0.1.0");
    assert_eq!(workflow_plan["publishable"], false);
    assert!(
        workflow_plan["issues"]
            .as_array()
            .unwrap()
            .iter()
            .any(|issue| issue == "workflow.description contains unresolved TODO")
    );
    assert_eq!(
        workflow_plan["command"],
        serde_json::json!([
            "cargo",
            "publish",
            "--manifest-path",
            ".lightflow/workflows/examples/example/Cargo.toml",
            "--dry-run"
        ])
    );
    complete_generated_workflow_metadata(&root, "examples", "example")?;
    let workflow_plan = lfw(&root, ["publish", "lightflow.example"])?;
    assert_eq!(workflow_plan["publishable"], true);
    assert_eq!(workflow_plan["issues"], serde_json::json!([]));

    let workspace_root_publish = Command::new(env!("CARGO_BIN_EXE_lfw"))
        .arg("publish")
        .current_dir(&root)
        .output()?;
    assert!(!workspace_root_publish.status.success());
    assert!(
        String::from_utf8_lossy(&workspace_root_publish.stderr)
            .contains("Cargo manifest is missing package.name")
    );

    let root_plan = lfw(Path::new(env!("CARGO_MANIFEST_DIR")), ["publish"])?;
    assert_eq!(root_plan["package"], "lightflow");
    assert_eq!(root_plan["publishable"], true);

    let extension = root.join("extensions/lightflow-extension");
    write_publishable_extension_crate(&extension)?;
    let extension_plan = lfw(
        &root,
        ["publish", "--crate", "extensions/lightflow-extension"],
    )?;
    assert_eq!(extension_plan["package"], "lightflow-extension");
    assert_eq!(extension_plan["publishable"], true);
    assert_eq!(
        extension_plan["command"],
        serde_json::json!([
            "cargo",
            "publish",
            "--manifest-path",
            "extensions/lightflow-extension/Cargo.toml",
            "--dry-run"
        ])
    );

    let child_workspace = root.join("vendor/child-workspace");
    let child_crate = child_workspace.join("crates/app");
    fs::create_dir_all(&child_crate)?;
    fs::write(
        child_workspace.join("Cargo.toml"),
        r#"
[workspace]
members = ["crates/app"]

[workspace.dependencies]
local-only = { path = "../local-only" }
"#,
    )?;
    fs::write(
        child_crate.join("Cargo.toml"),
        r#"
[package]
name = "child-app"
version = "0.1.0"
description = "Child app."
license = "MIT"

[dependencies]
local-only = { workspace = true }
"#,
    )?;
    let child_plan = lfw(
        &root,
        ["publish", "--crate", "vendor/child-workspace/crates/app"],
    )?;
    assert_eq!(child_plan["package"], "child-app");
    assert_eq!(child_plan["publishable"], false);
    assert!(
        child_plan["issues"]
            .as_array()
            .unwrap()
            .iter()
            .any(|issue| issue
                .as_str()
                .unwrap()
                .contains("dependency local-only uses path without a crates.io version"))
    );

    let workflows_plan = lfw(&root, ["publish", "--workflows"])?;
    assert_eq!(workflows_plan["dry_run"], true);
    assert_eq!(workflows_plan["target"]["kind"], "workflows");
    assert_eq!(workflows_plan["publishable"], true);
    assert_eq!(workflows_plan["total"], 1);
    assert_eq!(workflows_plan["publishable_count"], 1);
    assert_eq!(workflows_plan["blocked_count"], 0);
    assert_eq!(workflows_plan["crates"].as_array().unwrap().len(), 1);
    assert_eq!(workflows_plan["crates"][0]["package"], "lightflow-example");
    assert_eq!(
        workflows_plan["commands"][0],
        serde_json::json!([
            "cargo",
            "publish",
            "--manifest-path",
            ".lightflow/workflows/examples/example/Cargo.toml",
            "--dry-run"
        ])
    );
    let dirty_workflows_plan = lfw(&root, ["publish", "--workflows", "--allow-dirty"])?;
    assert_eq!(
        dirty_workflows_plan["commands"][0],
        serde_json::json!([
            "cargo",
            "publish",
            "--manifest-path",
            ".lightflow/workflows/examples/example/Cargo.toml",
            "--allow-dirty",
            "--dry-run"
        ])
    );
    let strict_workflows_plan = lfw(&root, ["publish", "--workflows", "--require-publishable"])?;
    assert_eq!(strict_workflows_plan["publishable"], true);
    assert_eq!(strict_workflows_plan["total"], 1);
    assert_eq!(strict_workflows_plan["publishable_count"], 1);
    assert_eq!(strict_workflows_plan["blocked_count"], 0);

    lfw(&root, ["new", "lightflow.base", "--category", "examples"])?;
    lfw(&root, ["new", "lightflow.top", "--category", "examples"])?;
    complete_generated_workflow_metadata(&root, "examples", "base")?;
    complete_generated_workflow_metadata(&root, "examples", "top")?;
    let top_manifest_path = root.join(".lightflow/workflows/examples/top/Cargo.toml");
    let mut top_manifest = fs::read_to_string(&top_manifest_path)?;
    top_manifest.push_str("lightflow-base = { path = \"../base\", version = \"0.1.0\" }\n");
    fs::write(&top_manifest_path, top_manifest)?;
    let ordered_plan = lfw(&root, ["publish", "--workflows"])?;
    let packages = ordered_plan["crates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|crate_plan| crate_plan["package"].as_str().unwrap())
        .collect::<Vec<_>>();
    let base_index = packages
        .iter()
        .position(|package| *package == "lightflow-base")
        .unwrap();
    let top_index = packages
        .iter()
        .position(|package| *package == "lightflow-top")
        .unwrap();
    assert!(base_index < top_index);
    assert_eq!(
        ordered_plan["crates"][top_index]["internal_dependencies"],
        serde_json::json!(["lightflow-base"])
    );
    let publish_catalog = serde_json::to_value(ApiService::new(&root).workflow_publish_checks()?)?;
    let api_packages = publish_catalog["checks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|check| check["package"].as_str().unwrap())
        .collect::<Vec<_>>();
    let api_base_index = api_packages
        .iter()
        .position(|package| *package == "lightflow-base")
        .unwrap();
    let api_top_index = api_packages
        .iter()
        .position(|package| *package == "lightflow-top")
        .unwrap();
    assert!(api_base_index < api_top_index);
    assert_eq!(
        publish_catalog["checks"][api_top_index]["internal_dependencies"],
        serde_json::json!(["lightflow-base"])
    );
    assert_eq!(
        publish_catalog["checks"][api_top_index]["version"],
        serde_json::json!("0.1.0")
    );

    let root_manifest_path = root.join("Cargo.toml");
    let mut root_manifest = fs::read_to_string(&root_manifest_path)?;
    root_manifest.push_str("bad-workspace = { path = \"../bad-workspace\" }\n");
    fs::write(&root_manifest_path, root_manifest)?;
    let example_manifest_path = root.join(".lightflow/workflows/examples/example/Cargo.toml");
    let mut example_manifest = fs::read_to_string(&example_manifest_path)?;
    example_manifest.push_str("bad-workspace = { workspace = true }\n");
    fs::write(&example_manifest_path, example_manifest)?;
    let blocked_plan = lfw(&root, ["publish", "--workflows"])?;
    assert_eq!(blocked_plan["publishable"], false);
    assert_eq!(blocked_plan["total"], 3);
    assert_eq!(blocked_plan["publishable_count"], 2);
    assert_eq!(blocked_plan["blocked_count"], 1);
    assert!(
        blocked_plan["issues"]
            .as_array()
            .unwrap()
            .iter()
            .any(|issue| issue
                .as_str()
                .unwrap()
                .contains("dependency bad-workspace uses path without a crates.io version"))
    );
    let strict_blocked = lfw_command(&root)
        .args(["publish", "--workflows", "--require-publishable"])
        .output()?;
    assert!(!strict_blocked.status.success());
    let strict_stderr = String::from_utf8_lossy(&strict_blocked.stderr);
    assert!(strict_stderr.contains("\"publishable\":false"));
    assert!(
        strict_stderr.contains("dependency bad-workspace uses path without a crates.io version")
    );

    let _ = fs::remove_dir_all(root);
    Ok(())
}

fn complete_generated_workflow_metadata(
    root: &Path,
    category: &str,
    name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let path = root
        .join(".lightflow/workflows")
        .join(category)
        .join(name)
        .join("src/lib.rs");
    let source = fs::read_to_string(&path)?
        .replace(
            "TODO: describe this workflow.",
            "Publishes a completed test workflow.",
        )
        .replace(
            "TODO: describe the input value.",
            "Input value for the test workflow.",
        )
        .replace(
            "TODO: describe the output value.",
            "Output value from the test workflow.",
        )
        .replace(
            "TODO: describe the runtime input value.",
            "Runtime input value for the test workflow.",
        )
        .replace(
            "TODO: describe the runtime output value.",
            "Runtime output value from the test workflow.",
        );
    fs::write(path, source)?;
    Ok(())
}
