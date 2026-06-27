mod support;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use support::*;

#[test]
fn abc_workflow_projects_resolve_import_run_and_add_modes() -> Result<(), Box<dyn std::error::Error>>
{
    let base = unique_temp_root();
    let project_a = base.join("lightflow-a");
    let project_b = base.join("lightflow-b");
    let project_c = base.join("lightflow-c");
    fs::create_dir_all(&base)?;

    write_leaf_project(&project_b, "b", "lightflow.b", "B")?;
    write_leaf_project(&project_c, "c", "lightflow.c", "C")?;
    write_a_project(&project_a, &project_b, &project_c)?;

    for project in [&project_a, &project_b, &project_c] {
        let workspace = fs::read_to_string(project.join("Cargo.toml"))?;
        assert!(workspace.contains("[workspace.dependencies]"));
        assert!(workspace.contains("lightflow = { path = "));
        let crate_manifest = workflow_manifest(project)?;
        assert!(fs::read_to_string(crate_manifest)?.contains("lightflow = { workspace = true }"));
    }

    let incomplete_deps = lfw(&project_a, ["deps", "lightflow.a"])?;
    assert_eq!(incomplete_deps["complete"], false);
    assert_eq!(
        incomplete_deps["missing_workflows"],
        serde_json::json!(["lightflow.b", "lightflow.c"])
    );

    let b_path = project_b
        .join(".lightflow/workflows/abc/b")
        .display()
        .to_string();
    let c_path = project_c
        .join(".lightflow/workflows/abc/c")
        .display()
        .to_string();
    let c_relative_path = "../lightflow-c/.lightflow/workflows/abc/c";
    let editable_b = lfw(
        &project_a,
        [
            "add",
            "lightflow-b",
            "--path",
            b_path.as_str(),
            "--editable",
        ],
    )?;
    assert_eq!(editable_b["dependency"], "lightflow-b");
    assert_eq!(editable_b["source"]["path"], b_path);
    assert_eq!(editable_b["editable"], true);

    let path_c = lfw(
        &project_a,
        ["add", "lightflow-c", "--path", c_relative_path],
    )?;
    assert_eq!(path_c["dependency"], "lightflow-c");
    assert_eq!(path_c["source"]["path"], c_relative_path);
    assert_eq!(path_c["editable"], false);

    let manifest = fs::read_to_string(project_a.join("Cargo.toml"))?;
    assert!(manifest.contains(&format!("lightflow-b = {{ path = \"{b_path}\" }}")));
    assert!(
        manifest.contains("lightflow-c = { path = \"../lightflow-c/.lightflow/workflows/abc/c\" }")
    );
    assert!(!manifest.contains("editable"));

    let listed = lfw(&project_a, ["list"])?;
    let ids = workflow_ids(&listed);
    assert_eq!(ids, vec!["lightflow.a", "lightflow.b", "lightflow.c"]);

    let deps = lfw(&project_a, ["deps", "lightflow.a"])?;
    assert_eq!(deps["complete"], true);
    assert_eq!(
        deps["workflow_order"],
        serde_json::json!(["lightflow.b", "lightflow.c", "lightflow.a"])
    );

    let true_run = lfw(
        &project_a,
        [
            "run",
            "lightflow.a",
            "-i",
            "use_b=true",
            "-i",
            "value=from-b",
        ],
    )?;
    assert_eq!(true_run["outputs"]["value"], "from-b");
    assert_eq!(true_run["nodes"][0]["selected_workflow_id"], "lightflow.b");

    let false_run = lfw(
        &project_a,
        [
            "run",
            "lightflow.a",
            "-i",
            "use_b=false",
            "-i",
            "value=from-c",
        ],
    )?;
    assert_eq!(false_run["outputs"]["value"], "from-c");
    assert_eq!(false_run["nodes"][0]["selected_workflow_id"], "lightflow.c");

    let global_project = base.join("global-consumer");
    write_a_project(&global_project, &project_b, &project_c)?;
    lfw(&global_project, ["init"])?;
    let global_b = lfw(
        &global_project,
        [
            "add",
            "-g",
            "lightflow-b",
            "--path",
            b_path.as_str(),
            "--editable",
        ],
    )?;
    assert_eq!(global_b["global"], true);
    assert_eq!(global_b["editable"], true);
    let global_c = lfw(
        &global_project,
        ["add", "-g", "lightflow-c", "--path", c_path.as_str()],
    )?;
    assert_eq!(global_c["global"], true);

    let global_manifest = fs::read_to_string(global_project.join(".lightflow/Cargo.toml"))?;
    assert!(global_manifest.contains("lightflow-b"));
    assert!(global_manifest.contains("lightflow-c"));
    let global_deps = lfw(&global_project, ["deps", "lightflow.a"])?;
    assert_eq!(global_deps["complete"], true);

    let import_collection = base.join("import-collection");
    write_empty_workspace(&import_collection)?;
    write_leaf_project_in_workspace(&import_collection, "b", "lightflow.b", "B")?;
    write_leaf_project_in_workspace(&import_collection, "c", "lightflow.c", "C")?;
    let import_project = base.join("import-consumer");
    write_a_project(&import_project, &project_b, &project_c)?;
    let imported = lfw(
        &import_project,
        ["import", import_collection.to_str().unwrap()],
    )?;
    let imported_packages = imported["imported"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["package"].as_str().unwrap_or_default())
        .collect::<Vec<_>>();
    assert_eq!(imported_packages, vec!["lightflow-b", "lightflow-c"]);
    let import_deps = lfw(&import_project, ["deps", "lightflow.a"])?;
    assert_eq!(import_deps["complete"], true);

    let git_import_collection = base.join("git-import-collection");
    write_empty_workspace(&git_import_collection)?;
    write_leaf_project_in_workspace(&git_import_collection, "b", "lightflow.b", "B")?;
    write_leaf_project_in_workspace(&git_import_collection, "c", "lightflow.c", "C")?;
    init_git_repo(&git_import_collection)?;
    let git_import_project = base.join("git-import-consumer");
    write_a_project(&git_import_project, &project_b, &project_c)?;
    let git_import_url = format!("file://{}", git_import_collection.display());
    let git_imported = lfw(
        &git_import_project,
        [
            "import",
            "--git",
            "--name",
            "abc-import",
            git_import_url.as_str(),
        ],
    )?;
    assert_eq!(git_imported["source"]["git"], git_import_url);
    let git_imported_packages = git_imported["imported"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["package"].as_str().unwrap_or_default())
        .collect::<Vec<_>>();
    assert_eq!(git_imported_packages, vec!["lightflow-b", "lightflow-c"]);
    let git_imported_paths = git_imported["imported"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["path"].as_str().unwrap_or_default())
        .collect::<Vec<_>>();
    assert!(
        git_imported_paths
            .iter()
            .all(|path| path.contains(".lightflow/repos/abc-import/.lightflow/workflows/abc/")),
        "git import paths should use the local repo cache: {git_imported_paths:?}"
    );
    assert!(
        git_imported["imported"]
            .as_array()
            .unwrap()
            .iter()
            .all(|item| item["dependency"]["editable"] == false)
    );
    let git_import_deps = lfw(&git_import_project, ["deps", "lightflow.a"])?;
    assert_eq!(git_import_deps["complete"], true);

    let registry_project = base.join("registry-install");
    write_empty_workspace(&registry_project)?;
    let registry = lfw(
        &registry_project,
        ["add", "lightflow-b", "--version", "0.1.0"],
    )?;
    assert_eq!(registry["source"]["registry"], "crates.io");
    assert_eq!(registry["version"], "0.1.0");
    assert!(
        fs::read_to_string(registry_project.join("Cargo.toml"))?
            .contains("lightflow-b = { version = \"0.1.0\" }")
    );

    let github_project = base.join("github-install");
    write_empty_workspace(&github_project)?;
    let github_url = "https://github.com/lightjunction/lightflow-b";
    let github = lfw(
        &github_project,
        [
            "add",
            "lightflow-b",
            "--git",
            github_url,
            "--package",
            "lightflow-b",
        ],
    )?;
    assert_eq!(github["source"]["git"], github_url);
    assert_eq!(github["package"], "lightflow-b");
    assert!(fs::read_to_string(github_project.join("Cargo.toml"))?.contains(
        "lightflow-b = { git = \"https://github.com/lightjunction/lightflow-b\", package = \"lightflow-b\" }"
    ));

    let git_repo = base.join("lightflow-b-git");
    write_standalone_workflow_crate(&git_repo, "lightflow-b", "lightflow.git_b")?;
    init_git_repo(&git_repo)?;
    let git_project = base.join("git-install");
    write_fetch_workspace(&git_project)?;
    let git_url = format!("file://{}", git_repo.display());
    let git = lfw(
        &git_project,
        [
            "add",
            "lightflow-b-git",
            "--git",
            git_url.as_str(),
            "--package",
            "lightflow-b",
        ],
    )?;
    assert_eq!(git["source"]["git"], git_url);
    assert_eq!(git["package"], "lightflow-b");
    run_ok(Command::new("cargo").arg("fetch").current_dir(&git_project))?;
    let git_lock = fs::read_to_string(git_project.join("Cargo.lock"))?;
    assert!(git_lock.contains("name = \"lightflow-b\""));

    let _ = fs::remove_dir_all(base);
    Ok(())
}

fn write_empty_workspace(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    fs::create_dir_all(root)?;
    fs::write(
        root.join("Cargo.toml"),
        format!(
            r#"[workspace]
resolver = "3"
members = [".lightflow/workflows/*/*"]

[workspace.dependencies]
lightflow = {{ path = {:?} }}
"#,
            env!("CARGO_MANIFEST_DIR")
        ),
    )?;
    Ok(())
}

fn write_fetch_workspace(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
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

[dependencies]
lightflow-b-git = { workspace = true }
"#,
    )?;
    fs::write(root.join("app/src/lib.rs"), "pub fn app() {}\n")?;
    Ok(())
}

fn write_leaf_project(
    root: &Path,
    short_name: &str,
    workflow_id: &str,
    display_name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    write_empty_workspace(root)?;
    write_leaf_project_in_workspace(root, short_name, workflow_id, display_name)
}

fn write_leaf_project_in_workspace(
    root: &Path,
    short_name: &str,
    workflow_id: &str,
    display_name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let crate_dir = root.join(".lightflow/workflows/abc").join(short_name);
    write_workflow_crate_at(
        &crate_dir,
        &workflow_id.replace('.', "-"),
        &format!(
            r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {{
    workflow("{workflow_id}")
        .version("0.1.0")
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

#[test]
fn add_path_dependency_creates_missing_workspace_dependencies_table()
-> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    fs::create_dir_all(&root)?;
    fs::write(
        root.join("Cargo.toml"),
        r#"[workspace]
members = []
resolver = "2"
"#,
    )?;

    let added = lfw(
        &root,
        [
            "add",
            "lightflow-local",
            "--path",
            "../lightflow-local/workflows/demo",
            "--editable",
        ],
    )?;
    assert_eq!(added["dependency"], "lightflow-local");
    assert_eq!(added["source"]["path"], "../lightflow-local/workflows/demo");
    assert_eq!(added["editable"], true);

    let manifest = fs::read_to_string(root.join("Cargo.toml"))?;
    assert!(manifest.contains("[workspace.dependencies]"));
    assert!(
        manifest.contains("lightflow-local = { path = \"../lightflow-local/workflows/demo\" }")
    );

    let _ = fs::remove_dir_all(root);
    Ok(())
}

fn write_a_project(
    root: &Path,
    project_b: &Path,
    project_c: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    write_empty_workspace(root)?;
    let b_hint = relative_path(root, &project_b.join(".lightflow/workflows/abc/b"));
    let c_hint = relative_path(root, &project_c.join(".lightflow/workflows/abc/c"));
    let source = format!(
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {{
    workflow("lightflow.a")
        .version("0.1.0")
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
    let crate_dir = root.join(".lightflow/workflows/abc/a");
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

fn write_standalone_workflow_crate(
    crate_dir: &Path,
    crate_name: &str,
    workflow_id: &str,
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
        format!(
            r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {{
    workflow("{workflow_id}")
        .version("0.1.0")
        .name("Git B")
        .input("value", "text")
        .output("value", "text")
        .build()
}}
"#
        ),
    )?;
    Ok(())
}

fn init_git_repo(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
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

fn run_ok(command: &mut Command) -> Result<(), Box<dyn std::error::Error>> {
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
        .join(".lightflow/workflows/abc")
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

fn workflow_manifest(project: &Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let abc = project.join(".lightflow/workflows/abc");
    let mut entries = fs::read_dir(abc)?
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

fn workflow_ids(list: &serde_json::Value) -> Vec<&str> {
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
