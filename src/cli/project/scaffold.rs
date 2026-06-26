use super::templates::{
    NodeTemplate, example_contract_test, example_contract_test_for_crate, example_skill_source,
    example_workflow_source, package_ident_from_id, package_name_from_id, title_from_id,
    workflow_skill_name,
};
use crate::cli::{CliError, CliResult};
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};

pub(in crate::cli) fn init_workflow_project(root: &Path) -> CliResult<serde_json::Value> {
    let workflows = root.join("workflows");
    let create_example = !root.join("Cargo.toml").exists();
    fs::create_dir_all(&workflows)?;

    let mut created = Vec::new();
    write_init_text(&root.join(".gitignore"), &project_gitignore(), &mut created)?;
    write_init_text(
        &root.join("Cargo.toml"),
        &workspace_manifest(),
        &mut created,
    )?;
    write_init_text(
        &workflows.join("README.md"),
        "# Workflows\n\nEach top-level directory is one category. Workflow crates live at `<category>/<short-name>/src/lib.rs`.\n",
        &mut created,
    )?;
    if create_example {
        let skill_name = workflow_skill_name("lightflow.example");
        write_init_text(
            &workflow_manifest_path(root, "lightflow.example", Some("examples"), false),
            &workflow_manifest("lightflow.example"),
            &mut created,
        )?;
        write_init_text(
            &workflow_source_path(root, "lightflow.example", Some("examples"), false),
            &example_workflow_source("lightflow.example", "Example Workflow", None),
            &mut created,
        )?;
        write_init_text(
            &workflow_skill_path(
                root,
                "lightflow.example",
                Some("examples"),
                &skill_name,
                false,
            ),
            &example_skill_source("Example Workflow", "lightflow.example", None),
            &mut created,
        )?;
    }

    Ok(json!({
        "kind": "workflow",
        "project_root": root,
        "created": created
    }))
}

pub(in crate::cli) fn init_plugin_project(root: &Path) -> CliResult<serde_json::Value> {
    let create_example = !root.join("Cargo.toml").exists();
    fs::create_dir_all(root)?;
    let mut created = Vec::new();
    write_init_text(&root.join(".gitignore"), &project_gitignore(), &mut created)?;
    write_init_text(
        &root.join("Cargo.toml"),
        &plugin_manifest(root),
        &mut created,
    )?;
    if create_example {
        let workflow_id = plugin_workflow_id(root);
        write_init_text(
            &root.join("src").join("lib.rs"),
            &example_workflow_source(&workflow_id, &plugin_title(root), None),
            &mut created,
        )?;
        write_init_text(
            &root.join("tests").join("contract.rs"),
            &example_contract_test_for_crate(
                &workflow_id,
                &package_ident_from_id(
                    root.file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("lightflow-plugin"),
                ),
                &NodeTemplate::passthrough(),
            ),
            &mut created,
        )?;
        write_init_text(
            &root
                .join(".agent")
                .join("skills")
                .join(workflow_skill_name(&workflow_id))
                .join("SKILL.md"),
            &example_skill_source(&plugin_title(root), &workflow_id, None),
            &mut created,
        )?;
    }
    Ok(json!({
        "kind": "plugin",
        "project_root": root,
        "created": created
    }))
}

pub(in crate::cli) fn add_workflow(
    root: &Path,
    workflow_id: &str,
    name: Option<&str>,
    category: Option<&str>,
    runtime: Option<&str>,
    global: bool,
) -> CliResult<serde_json::Value> {
    validate_spec_id(workflow_id, "workflow id")?;
    if let Some(category) = category {
        validate_spec_id(category, "workflow category")?;
    }
    let mut created = Vec::new();
    ensure_workspace_manifest(root, global, &mut created)?;
    let category =
        category.ok_or_else(|| CliError::Usage("lfw new requires --category <name>".to_owned()))?;
    let manifest_path = workflow_manifest_path(root, workflow_id, Some(category), global);
    let source_path = workflow_source_path(root, workflow_id, Some(category), global);
    let skill_path = workflow_skill_path(
        root,
        workflow_id,
        Some(category),
        &workflow_skill_name(workflow_id),
        global,
    );
    let template = NodeTemplate::for_runtime(runtime);
    let generated_title = title_from_id(workflow_id);
    let title = name.unwrap_or(&generated_title);
    write_new_text(
        &manifest_path,
        &workflow_manifest(workflow_id),
        &mut created,
    )?;
    write_new_text(
        &source_path,
        &example_workflow_source(workflow_id, title, Some(&template)),
        &mut created,
    )?;
    write_new_text(
        &skill_path,
        &example_skill_source(title, workflow_id, Some(&template)),
        &mut created,
    )?;
    let test_path = workflow_crate_dir(root, workflow_id, Some(category), global)
        .join("tests")
        .join("contract.rs");
    write_new_text(
        &test_path,
        &example_contract_test(workflow_id, &template),
        &mut created,
    )?;
    Ok(json!({
        "workflow_id": workflow_id,
        "category": category,
        "runtime": template.runtime,
        "example": template.example_command(workflow_id),
        "global": global,
        "crate_dir": workflow_crate_dir(root, workflow_id, Some(category), global),
        "path": source_path,
        "created": created
    }))
}

fn ensure_workspace_manifest(
    root: &Path,
    global: bool,
    created: &mut Vec<String>,
) -> CliResult<()> {
    let manifest_path = root.join("Cargo.toml");
    if manifest_path.exists() {
        return Ok(());
    }
    let manifest = if global {
        workflow_collection_manifest()
    } else {
        workspace_manifest()
    };
    write_new_text(&manifest_path, &manifest, created)
}

pub(in crate::cli) fn workspace_manifest() -> String {
    format!(
        "[workspace]\nresolver = \"3\"\nmembers = [\"workflows/*/*\"]\n\n[workspace.dependencies]\nlightflow = {:?}\n",
        env!("CARGO_PKG_VERSION")
    )
}

pub(in crate::cli) fn workflow_collection_manifest() -> String {
    format!(
        "[workspace]\nresolver = \"3\"\nmembers = [\"workflows/*/*\"]\n\n[workspace.dependencies]\nlightflow = {:?}\n",
        env!("CARGO_PKG_VERSION")
    )
}

fn project_gitignore() -> String {
    [
        "/target/",
        "/.cache/",
        "/.test-xdg/",
        "/lfw.lock",
        "",
        "# Local editor and OS files",
        ".DS_Store",
        "*.swp",
        "*.swo",
        "",
    ]
    .join("\n")
}

fn plugin_manifest(root: &Path) -> String {
    let package = root
        .file_name()
        .and_then(|name| name.to_str())
        .map(package_name_from_id)
        .unwrap_or_else(|| "lightflow-plugin".to_owned());
    format!(
        "[package]\nname = {:?}\nversion = \"0.1.0\"\nedition = \"2024\"\ndescription = \"LightFlow workflow plugin.\"\nlicense = \"MIT OR Apache-2.0\"\n\n[dependencies]\nlightflow = {:?}\n",
        package,
        env!("CARGO_PKG_VERSION")
    )
}

fn plugin_workflow_id(root: &Path) -> String {
    let suffix = root
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("plugin")
        .replace('-', "_");
    format!("lightflow.{suffix}")
}

fn plugin_title(root: &Path) -> String {
    title_from_id(
        root.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("plugin"),
    )
}

fn workflow_manifest(workflow_id: &str) -> String {
    format!(
        "[package]\nname = {:?}\nversion = \"0.1.0\"\nedition = \"2024\"\ndescription = {:?}\nlicense = \"MIT OR Apache-2.0\"\nrepository = {:?}\n\n[dependencies]\nlightflow = {{ workspace = true }}\n",
        package_name_from_id(workflow_id),
        format!("LightFlow workflow {}", workflow_id),
        env!("CARGO_PKG_REPOSITORY")
    )
}

fn workflow_crate_dir(
    root: &Path,
    workflow_id: &str,
    category: Option<&str>,
    _global: bool,
) -> PathBuf {
    let mut path = root.join("workflows");
    if let Some(category) = category {
        path = path.join(category);
    }
    path.join(workflow_crate_dir_name(workflow_id))
}

pub(in crate::cli) fn workflow_crate_dir_name(workflow_id: &str) -> String {
    workflow_id
        .strip_prefix("lightflow.")
        .unwrap_or(workflow_id)
        .replace('.', "_")
}

fn workflow_manifest_path(
    root: &Path,
    workflow_id: &str,
    category: Option<&str>,
    global: bool,
) -> PathBuf {
    workflow_crate_dir(root, workflow_id, category, global).join("Cargo.toml")
}

fn workflow_source_path(
    root: &Path,
    workflow_id: &str,
    category: Option<&str>,
    global: bool,
) -> PathBuf {
    workflow_crate_dir(root, workflow_id, category, global)
        .join("src")
        .join("lib.rs")
}

fn workflow_skill_path(
    root: &Path,
    workflow_id: &str,
    category: Option<&str>,
    skill_name: &str,
    global: bool,
) -> PathBuf {
    workflow_crate_dir(root, workflow_id, category, global)
        .join(".agent")
        .join("skills")
        .join(skill_name)
        .join("SKILL.md")
}

pub(in crate::cli) fn validate_spec_id(value: &str, label: &str) -> CliResult<()> {
    if value.is_empty()
        || value == "."
        || value == ".."
        || value.contains('/')
        || value.contains('\\')
    {
        return Err(CliError::Usage(format!("invalid {label}: {value}")));
    }
    Ok(())
}

pub(in crate::cli) fn normalize_workflow_id(value: &str) -> String {
    let value = value.strip_suffix(".rs").unwrap_or(value);
    if value.starts_with("lightflow.") {
        value.to_owned()
    } else {
        format!("lightflow.{value}")
    }
}

fn write_new_text(path: &Path, body: &str, created: &mut Vec<String>) -> CliResult<()> {
    if path.exists() {
        return Err(CliError::Usage(format!(
            "{} already exists; refusing to overwrite",
            path.display()
        )));
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, body)?;
    created.push(path.to_string_lossy().into_owned());
    Ok(())
}

fn write_init_text(path: &Path, body: &str, created: &mut Vec<String>) -> CliResult<()> {
    if path.exists() {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, body)?;
    created.push(path.to_string_lossy().into_owned());
    Ok(())
}
