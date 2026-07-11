use super::super::templates::{package_name_from_id, title_from_id};
use crate::cli::{CliError, CliResult};
use crate::workflow::workflow_id_from_package_name;
use std::path::{Path, PathBuf};

pub(super) fn plugin_workflow_id(root: &Path) -> String {
    let root_name = root
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("plugin");
    workflow_id_from_package_name(&package_name_from_id(root_name))
}

pub(super) fn plugin_title(root: &Path) -> String {
    title_from_id(
        root.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("plugin"),
    )
}

pub(super) fn workflow_crate_dir(root: &Path, workflow_id: &str, global: bool) -> PathBuf {
    let path = if global {
        root.join("workflows")
    } else {
        project_workflow_dir(root)
    };
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

pub(super) fn workflow_manifest_path(root: &Path, workflow_id: &str, global: bool) -> PathBuf {
    workflow_crate_dir(root, workflow_id, global).join("Cargo.toml")
}

pub(super) fn workflow_source_path(root: &Path, workflow_id: &str, global: bool) -> PathBuf {
    workflow_crate_dir(root, workflow_id, global)
        .join("src")
        .join("lib.rs")
}

pub(super) fn workflow_skill_path(
    root: &Path,
    workflow_id: &str,
    skill_name: &str,
    global: bool,
) -> PathBuf {
    workflow_crate_dir(root, workflow_id, global)
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
    let suffix = value.strip_prefix("lightflow.").unwrap_or(value);
    format!("lightflow.{}", suffix.replace(['.', '-'], "_"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_workflow_id_matches_generated_manifest_package() {
        assert_eq!(
            plugin_workflow_id(Path::new("/tmp/lightflow-demo")),
            "lightflow.demo"
        );
    }
}
