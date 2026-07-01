use super::super::templates::title_from_id;
use crate::cli::{CliError, CliResult};
use std::path::{Path, PathBuf};

pub(super) fn plugin_workflow_id(root: &Path) -> String {
    let suffix = root
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("plugin")
        .replace('-', "_");
    format!("lightflow.{suffix}")
}

pub(super) fn plugin_title(root: &Path) -> String {
    title_from_id(
        root.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("plugin"),
    )
}

pub(super) fn workflow_crate_dir(
    root: &Path,
    workflow_id: &str,
    category: Option<&str>,
    global: bool,
) -> PathBuf {
    let mut path = if global {
        root.join("workflows")
    } else {
        project_workflow_dir(root)
    };
    if let Some(category) = category {
        path = path.join(category);
    }
    path.join(workflow_crate_dir_name(workflow_id))
}

pub(super) fn project_workflow_dir(root: &Path) -> PathBuf {
    root.join(".lightflow").join("workflows")
}

pub(in crate::cli) fn workflow_crate_dir_name(workflow_id: &str) -> String {
    workflow_id
        .strip_prefix("lightflow.")
        .unwrap_or(workflow_id)
        .replace('.', "_")
}

pub(super) fn workflow_manifest_path(
    root: &Path,
    workflow_id: &str,
    category: Option<&str>,
    global: bool,
) -> PathBuf {
    workflow_crate_dir(root, workflow_id, category, global).join("Cargo.toml")
}

pub(super) fn workflow_source_path(
    root: &Path,
    workflow_id: &str,
    category: Option<&str>,
    global: bool,
) -> PathBuf {
    workflow_crate_dir(root, workflow_id, category, global)
        .join("src")
        .join("lib.rs")
}

pub(super) fn workflow_skill_path(
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
