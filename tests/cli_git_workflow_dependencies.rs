mod cli_project_support;
mod support;

use cli_project_support::{git_ok, use_local_lightflow_dependency};
use std::fs;
use std::path::Path;
use std::process::Command;
use support::{lfw, unique_temp_root};

#[test]
fn add_git_single_workflow_crate_is_discoverable() -> Result<(), Box<dyn std::error::Error>> {
    let base = unique_temp_root();
    let project = base.join("consumer");
    let workflow = base.join("git-workflow");
    fs::create_dir_all(&project)?;
    write_workflow_crate(&workflow, "lightflow-git-example")?;
    init_git_repo(&workflow)?;
    lfw(&project, ["init"])?;
    use_local_lightflow_dependency(&project)?;

    let git_url = format!("file://{}", workflow.display());
    let added = lfw(
        &project,
        ["add", "lightflow-git-example", "--git", &git_url],
    )?;
    assert_eq!(added["source"]["git"], git_url);
    let manifest = fs::read_to_string(project.join("Cargo.toml"))?;
    assert!(manifest.contains("[dependencies]"));
    assert!(manifest.contains("lightflow-git-example = { git = "));

    assert!(workflow_list_contains(
        &lfw(&project, ["list"])?,
        "lightflow.git_example"
    ));

    let _ = fs::remove_dir_all(base);
    Ok(())
}

#[test]
fn cargo_add_path_single_workflow_crate_is_discoverable() -> Result<(), Box<dyn std::error::Error>>
{
    let base = unique_temp_root();
    let project = base.join("consumer");
    let workflow = base.join("path-workflow");
    fs::create_dir_all(&project)?;
    write_workflow_crate(&workflow, "lightflow-cargo-example")?;
    lfw(&project, ["init"])?;
    use_local_lightflow_dependency(&project)?;

    let output = Command::new("cargo")
        .args(["add", "--path"])
        .arg(&workflow)
        .current_dir(&project)
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "cargo add failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }

    let manifest = fs::read_to_string(project.join("Cargo.toml"))?;
    assert!(manifest.contains("[dependencies]"));
    assert!(manifest.contains("lightflow-cargo-example"));
    assert!(workflow_list_contains(
        &lfw(&project, ["list"])?,
        "lightflow.cargo_example"
    ));

    let _ = fs::remove_dir_all(base);
    Ok(())
}

fn workflow_list_contains(list: &serde_json::Value, expected: &str) -> bool {
    list["workflows"]
        .as_array()
        .is_some_and(|workflows| workflows.iter().any(|workflow| workflow["id"] == expected))
}

fn write_workflow_crate(root: &Path, package: &str) -> Result<(), Box<dyn std::error::Error>> {
    fs::create_dir_all(root.join("src"))?;
    fs::write(
        root.join("Cargo.toml"),
        format!(
            r#"[package]
name = {package:?}
version = "0.1.0"
edition = "2024"

[dependencies]
lightflow = {{ path = {:?} }}
"#,
            env!("CARGO_MANIFEST_DIR")
        ),
    )?;
    fs::write(
        root.join("src/lib.rs"),
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow!().name("Cargo Workflow Example").build()
}
"#,
    )?;
    Ok(())
}

fn init_git_repo(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    git_ok(root, ["init", "-q"])?;
    git_ok(root, ["config", "user.email", "tests@example.com"])?;
    git_ok(root, ["config", "user.name", "LightFlow Tests"])?;
    git_ok(root, ["add", "."])?;
    git_ok(root, ["commit", "-q", "-m", "fixture"])
}
