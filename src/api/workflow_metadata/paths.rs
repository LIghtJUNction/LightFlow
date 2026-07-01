use super::super::{LEGACY_LIGHTFLOW_DIR, PROJECT_LIGHTFLOW_DIR, WORKFLOW_DIR, util};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub(crate) fn categorized_workflow_manifest_path(
    root: &Path,
    workflow_id: &str,
) -> io::Result<PathBuf> {
    let project_workflows = root.join(PROJECT_LIGHTFLOW_DIR).join(WORKFLOW_DIR);
    let workflows = root.join(WORKFLOW_DIR);
    let legacy_workflows = root.join(LEGACY_LIGHTFLOW_DIR).join(WORKFLOW_DIR);
    let entries = match fs::read_dir(&project_workflows).or_else(|error| {
        if error.kind() == io::ErrorKind::NotFound {
            fs::read_dir(&workflows).or_else(|error| {
                if error.kind() == io::ErrorKind::NotFound {
                    fs::read_dir(&legacy_workflows)
                } else {
                    Err(error)
                }
            })
        } else {
            Err(error)
        }
    }) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(project_workflows.join(workflow_id).join("Cargo.toml"));
        }
        Err(error) => return Err(error),
    };
    for entry in entries {
        let path = entry?.path();
        if !path.is_dir() || path.join("src").join("lib.rs").exists() {
            continue;
        }
        let category = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        let manifest = path
            .join(util::workflow_crate_dir_name(workflow_id))
            .join("Cargo.toml");
        if manifest.exists() {
            return Ok(manifest);
        }
        if let Some(short_name) = workflow_category_short_name(workflow_id, category) {
            let manifest = path.join(short_name).join("Cargo.toml");
            if manifest.exists() {
                return Ok(manifest);
            }
        }
    }
    Ok(project_workflows.join(workflow_id).join("Cargo.toml"))
}

pub(super) fn workflow_lib_path(manifest: &Path) -> Option<PathBuf> {
    Some(manifest.parent()?.join("src").join("lib.rs"))
}

fn workflow_category_short_name(workflow_id: &str, category: &str) -> Option<String> {
    let prefixed = workflow_id
        .strip_prefix("lightflow.")
        .unwrap_or(workflow_id);
    let short = prefixed.strip_prefix(category)?.strip_prefix('.')?;
    Some(short.replace('.', "_"))
}
