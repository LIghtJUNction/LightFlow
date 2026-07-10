use super::templates::{
    NodeTemplate, example_contract_test, example_contract_test_for_crate, example_skill_source,
    example_workflow_source, package_ident_from_id, package_name_from_id, title_from_id,
    workflow_skill_name,
};
use crate::cli::{CliError, CliResult};
use files::{write_init_text, write_new_text};
use manifests::{plugin_manifest, project_gitignore, workflow_manifest};
use paths::{
    plugin_title, plugin_workflow_id, project_workflow_dir, workflow_crate_dir,
    workflow_manifest_path, workflow_skill_path, workflow_source_path,
};
use serde_json::json;
use std::fs;
use std::path::Path;

mod files;
mod manifests;
mod paths;
pub(in crate::cli) use manifests::{
    workflow_collection_manifest, workflow_host_package_name, workflow_host_source,
    workspace_manifest,
};
pub(in crate::cli) use paths::{normalize_workflow_id, validate_spec_id};

pub(in crate::cli) fn init_workflow_project(root: &Path) -> CliResult<serde_json::Value> {
    let workflows = project_workflow_dir(root);
    let create_example = !root.join("Cargo.toml").exists();
    fs::create_dir_all(&workflows)?;

    let mut created = Vec::new();
    write_init_text(&root.join(".gitignore"), &project_gitignore(), &mut created)?;
    write_init_text(
        &root.join("Cargo.toml"),
        &workspace_manifest(root),
        &mut created,
    )?;
    if create_example {
        write_init_text(
            &root.join(".lightflow/workspace.rs"),
            workflow_host_source(),
            &mut created,
        )?;
    }
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
        workflow_collection_manifest(root)
    } else {
        workspace_manifest(root)
    };
    write_new_text(&manifest_path, &manifest, created)?;
    write_new_text(
        &root.join(".lightflow/workspace.rs"),
        workflow_host_source(),
        created,
    )
}
