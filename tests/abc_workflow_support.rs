#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub(crate) fn write_empty_workspace(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    fs::create_dir_all(root)?;
    fs::write(
        root.join("Cargo.toml"),
        format!(
            r#"[workspace]
resolver = "3"
members = [".lightflow/workflows/*"]

[workspace.dependencies]
lightflow = {{ path = {:?} }}
"#,
            env!("CARGO_MANIFEST_DIR")
        ),
    )?;
    Ok(())
}

pub(crate) fn write_fetch_workspace(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    fs::create_dir_all(root.join("app/src"))?;
    fs::write(
        root.join("Cargo.toml"),
        format!(
            r#"[workspace]
resolver = "3"
members = ["app"]

[workspace.dependencies]
lightflow = {{ path = {:?} }}
"#,
            env!("CARGO_MANIFEST_DIR")
        ),
    )?;
    fs::write(
        root.join("app/Cargo.toml"),
        r#"[package]
name = "git-install-app"
version = "0.1.0"
edition = "2024"
publish = false

"#,
    )?;
    fs::write(root.join("app/src/lib.rs"), "pub fn app() {}\n")?;
    Ok(())
}

pub(crate) fn write_leaf_project(
    root: &Path,
    short_name: &str,
    workflow_id: &str,
    display_name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    write_empty_workspace(root)?;
    write_leaf_project_in_workspace(root, short_name, workflow_id, display_name)
}

pub(crate) fn write_leaf_project_in_workspace(
    root: &Path,
    short_name: &str,
    workflow_id: &str,
    display_name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let crate_dir = root.join(".lightflow/workflows").join(short_name);
    write_workflow_crate_at(
        &crate_dir,
        &workflow_id.replace('.', "-"),
        &format!(
            r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {{
    workflow!()
        .name("{display_name}")
        .input("value", "text")
        .output("value", "text")
        .build()
}}
"#
        ),
    )?;
    write_skill(root, short_name, workflow_id)?;
    Ok(())
}

pub(crate) fn write_a_project(
    root: &Path,
    project_b: &Path,
    project_c: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    write_empty_workspace(root)?;
    let b_hint = relative_path(root, &project_b.join(".lightflow/workflows/b"));
    let c_hint = relative_path(root, &project_c.join(".lightflow/workflows/c"));
    let source = format!(
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {{
    workflow!()
        .name("A")
        .input("use_b", "boolean")
        .input("value", "text")
        .output("value", "text")
        .depends_on_path("lightflow.b", "0.1.0", "lightflow-b", "{b_hint}")
        .depends_on_path("lightflow.c", "0.1.0", "lightflow-c", "{c_hint}")
        .if_node("choose", "use_b", true, "lightflow.b", "lightflow.c")
        .build()
}}
"#
    );
    let crate_dir = root.join(".lightflow/workflows/a");
    write_workflow_crate_at(&crate_dir, "lightflow-a", &source)?;
    write_skill(root, "a", "lightflow.a")?;
    Ok(())
}

fn write_workflow_crate_at(
    crate_dir: &Path,
    crate_name: &str,
    source: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    fs::create_dir_all(crate_dir.join("src"))?;
    fs::write(
        crate_dir.join("Cargo.toml"),
        format!(
            r#"[package]
name = "{crate_name}"
version = "0.1.0"
edition = "2024"
publish = false

[dependencies]
lightflow = {{ workspace = true }}
"#
        ),
    )?;
    fs::write(crate_dir.join("src/lib.rs"), source)?;
    Ok(())
}

pub(crate) fn write_standalone_workflow_crate(
    crate_dir: &Path,
    crate_name: &str,
    _workflow_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    fs::create_dir_all(crate_dir.join("src"))?;
    fs::write(
        crate_dir.join("Cargo.toml"),
        format!(
            r#"[package]
name = "{crate_name}"
version = "0.1.0"
edition = "2024"
publish = false

[dependencies]
lightflow = {{ path = {:?} }}
"#,
            env!("CARGO_MANIFEST_DIR")
        ),
    )?;
    fs::write(
        crate_dir.join("src/lib.rs"),
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow!()
        .name("Git B")
        .input("value", "text")
        .output("value", "text")
        .build()
}
"#,
    )?;
    Ok(())
}

pub(crate) fn init_git_repo(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    run_ok(Command::new("git").arg("init").current_dir(root))?;
    run_ok(Command::new("git").args(["add", "."]).current_dir(root))?;
    run_ok(
        Command::new("git")
            .args([
                "-c",
                "user.email=lightflow@example.invalid",
                "-c",
                "user.name=LightFlow Test",
                "commit",
                "-m",
                "initial workflow",
            ])
            .current_dir(root),
    )?;
    Ok(())
}

pub(crate) fn run_ok(command: &mut Command) -> Result<(), Box<dyn std::error::Error>> {
    let output = command.output()?;
    if output.status.success() {
        return Ok(());
    }
    Err(format!(
        "command failed with status {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
    .into())
}

fn write_skill(
    project: &Path,
    short_name: &str,
    workflow_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let skill_dir = project
        .join(".lightflow/workflows")
        .join(short_name)
        .join(".agent/skills")
        .join(workflow_id.replace('.', "-"));
    fs::create_dir_all(&skill_dir)?;
    fs::write(
        skill_dir.join("SKILL.md"),
        format!(
            r#"---
name: {}
description: Use the {workflow_id} test workflow.
version: 0.1.0
---

# {workflow_id}

Run with `lfw run {workflow_id}`.
"#,
            workflow_id.replace('.', "-")
        ),
    )?;
    Ok(())
}

pub(crate) fn workflow_manifest(project: &Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let workflows = project.join(".lightflow/workflows");
    let mut entries = fs::read_dir(workflows)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<Vec<_>, _>>()?;
    entries.sort();
    entries
        .into_iter()
        .find_map(|entry| {
            let manifest = entry.join("Cargo.toml");
            manifest.exists().then_some(manifest)
        })
        .ok_or_else(|| "missing workflow manifest".into())
}

pub(crate) fn workflow_ids(list: &serde_json::Value) -> Vec<&str> {
    list["workflows"]
        .as_array()
        .expect("workflows list returns an array")
        .iter()
        .map(|workflow| workflow["id"].as_str().unwrap_or_default())
        .collect()
}

fn relative_path(from: &Path, to: &Path) -> String {
    let Some(parent) = from.parent() else {
        return to.display().to_string();
    };
    if let Ok(sibling_path) = to.strip_prefix(parent) {
        return format!("../{}", sibling_path.display());
    }
    to.display().to_string()
}
