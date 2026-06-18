use super::{CliError, CliResult, required_flag_value};
use serde_json::json;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy)]
pub(super) enum InitMode {
    Workflow,
    Plugin,
}

pub(super) struct InitOptions {
    pub(super) mode: InitMode,
    pub(super) root: PathBuf,
}

pub(super) fn parse_init_options(args: &[String]) -> CliResult<InitOptions> {
    let mut mode = InitMode::Workflow;
    let mut root = None;
    for arg in args {
        match arg.as_str() {
            "--workflow" => {
                if matches!(mode, InitMode::Plugin) {
                    return Err(CliError::Usage(
                        "--workflow cannot be combined with --plugin".to_owned(),
                    ));
                }
                mode = InitMode::Workflow;
            }
            "--plugin" => {
                if matches!(mode, InitMode::Plugin) {
                    return Err(CliError::Usage("duplicate flag --plugin".to_owned()));
                }
                mode = InitMode::Plugin;
            }
            value if value.starts_with("--") => {
                return Err(CliError::Usage(format!(
                    "unexpected argument for init: {value}"
                )));
            }
            value => {
                if root.is_some() {
                    return Err(CliError::Usage(format!(
                        "unexpected argument for init: {value}"
                    )));
                }
                root = Some(PathBuf::from(value));
            }
        }
    }
    Ok(InitOptions {
        mode,
        root: root.unwrap_or(env::current_dir()?),
    })
}

pub(super) struct AddWorkflowOptions {
    pub(super) workflow_id: String,
    pub(super) name: Option<String>,
    pub(super) category: Option<String>,
    pub(super) global: bool,
}

pub(super) fn parse_add_workflow_options(args: &[String]) -> CliResult<AddWorkflowOptions> {
    let mut workflow_id = None;
    let mut name = None;
    let mut category = None;
    let mut global = false;
    let mut index = 0;
    while index < args.len() {
        let flag = args[index].as_str();
        match flag {
            "--global" | "-g" => {
                global = true;
                index += 1;
                continue;
            }
            "--name" => {
                if name.is_some() {
                    return Err(CliError::Usage("duplicate flag --name".to_owned()));
                }
                name = Some(required_flag_value(args, index, flag)?.to_owned());
            }
            "--category" => {
                if category.is_some() {
                    return Err(CliError::Usage("duplicate flag --category".to_owned()));
                }
                let value = required_flag_value(args, index, flag)?;
                validate_spec_id(value, "workflow category")?;
                category = Some(value.to_owned());
            }
            value if value.starts_with("--") => {
                return Err(CliError::Usage(format!(
                    "unexpected argument for new: {value}"
                )));
            }
            value => {
                if workflow_id.is_some() {
                    return Err(CliError::Usage(format!(
                        "unexpected argument for new: {value}"
                    )));
                }
                workflow_id = Some(value.to_owned());
                index += 1;
                continue;
            }
        }
        index += 2;
    }
    let workflow_id =
        workflow_id.ok_or_else(|| CliError::Usage("missing workflow id".to_owned()))?;
    Ok(AddWorkflowOptions {
        workflow_id,
        name,
        category,
        global,
    })
}

pub(super) fn init_workflow_project(root: &Path) -> CliResult<serde_json::Value> {
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
            &example_workflow_source("lightflow.example", "Example Workflow"),
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
            &example_skill_source("Example Workflow", "lightflow.example"),
            &mut created,
        )?;
    }

    Ok(json!({
        "kind": "workflow",
        "project_root": root,
        "created": created
    }))
}

pub(super) fn init_plugin_project(root: &Path) -> CliResult<serde_json::Value> {
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
            &example_workflow_source(&workflow_id, &plugin_title(root)),
            &mut created,
        )?;
        write_init_text(
            &root
                .join(".agent")
                .join("skills")
                .join(workflow_skill_name(&workflow_id))
                .join("SKILL.md"),
            &example_skill_source(&plugin_title(root), &workflow_id),
            &mut created,
        )?;
    }
    Ok(json!({
        "kind": "plugin",
        "project_root": root,
        "created": created
    }))
}

pub(super) fn add_workflow(
    root: &Path,
    workflow_id: &str,
    name: Option<&str>,
    category: Option<&str>,
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
    let workflow_source =
        example_workflow_source(workflow_id, name.unwrap_or(&title_from_id(workflow_id)));
    write_new_text(
        &manifest_path,
        &workflow_manifest(workflow_id),
        &mut created,
    )?;
    write_new_text(&source_path, &workflow_source, &mut created)?;
    write_new_text(
        &skill_path,
        &example_skill_source(name.unwrap_or(&title_from_id(workflow_id)), workflow_id),
        &mut created,
    )?;
    Ok(json!({
        "workflow_id": workflow_id,
        "category": category,
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

pub(super) fn workspace_manifest() -> String {
    format!(
        "[workspace]\nresolver = \"3\"\nmembers = [\"workflows/*/*\"]\n\n[workspace.dependencies]\nlightflow = {:?}\n",
        env!("CARGO_PKG_VERSION")
    )
}

pub(super) fn workflow_collection_manifest() -> String {
    format!(
        "[workspace]\nresolver = \"3\"\nmembers = [\"*/*\"]\n\n[workspace.dependencies]\nlightflow = {:?}\n",
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
    global: bool,
) -> PathBuf {
    let mut path = if global {
        root.to_path_buf()
    } else {
        root.join("workflows")
    };
    if let Some(category) = category {
        path = path.join(category);
    }
    path.join(workflow_crate_dir_name(workflow_id))
}

pub(super) fn workflow_crate_dir_name(workflow_id: &str) -> String {
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

fn example_workflow_source(workflow_id: &str, name: &str) -> String {
    format!(
        "use lightflow::preload::*;\n\npub fn define() -> WorkflowSpec {{\n    workflow({})\n        .version(\"0.1.0\")\n        .name({})\n        .description(\"TODO: describe this workflow.\")\n        .input(\"value\", \"json\")\n        .output(\"value\", \"json\")\n        .build()\n}}\n",
        rust_string(workflow_id),
        rust_string(name)
    )
}

fn example_skill_source(name: &str, workflow_id: &str) -> String {
    format!(
        "---\nname: {}\ndescription: This skill should be used when working with the {} LightFlow workflow, configuring its inputs, running it through lfw, or composing it with other LightFlow workflows.\nversion: 0.1.0\n---\n\n# {}\n\nUse this skill to understand the workflow contract for `{}`.\n\n## Workflow\n\n- Workflow id: `{}`\n- Define expected inputs, outputs, model requirements, and runtime notes here.\n- Include concrete `lfw run` and `lfw sync` examples that are valid for this workflow.\n",
        rust_string(name),
        workflow_id,
        name,
        workflow_id,
        workflow_id
    )
}

pub(super) fn validate_spec_id(value: &str, label: &str) -> CliResult<()> {
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

pub(super) fn normalize_workflow_id(value: &str) -> String {
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

fn rust_string(value: &str) -> String {
    format!("{value:?}")
}

fn package_name_from_id(id: &str) -> String {
    let mut name = String::new();
    let mut previous_dash = false;
    for character in id.chars() {
        if character.is_ascii_alphanumeric() {
            name.push(character.to_ascii_lowercase());
            previous_dash = false;
        } else if !previous_dash {
            name.push('-');
            previous_dash = true;
        }
    }
    let name = name.trim_matches('-');
    if name.is_empty() {
        "workflow".to_owned()
    } else {
        name.to_owned()
    }
}

fn workflow_skill_name(id: &str) -> String {
    package_name_from_id(id)
}

fn title_from_id(id: &str) -> String {
    let suffix = id.rsplit('.').next().unwrap_or(id);
    suffix
        .split(['_', '-'])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => format!("{}{}", first.to_ascii_uppercase(), chars.as_str()),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
