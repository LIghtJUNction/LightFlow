use super::super::{
    LEGACY_LIGHTFLOW_DIR, PROJECT_LIGHTFLOW_DIR, WORKFLOW_DIR, util, workflow_package_identity,
};
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
            return Ok(project_workflows
                .join(util::workflow_crate_dir_name(workflow_id))
                .join("Cargo.toml"));
        }
        Err(error) => return Err(error),
    };
    for entry in entries {
        let manifest = entry?.path().join("Cargo.toml");
        if workflow_package_identity(&manifest).is_ok_and(|(id, _)| id == workflow_id) {
            return Ok(manifest);
        }
    }
    Ok(project_workflows
        .join(util::workflow_crate_dir_name(workflow_id))
        .join("Cargo.toml"))
}

pub(super) fn workflow_lib_path(manifest: &Path) -> Option<PathBuf> {
    Some(manifest.parent()?.join("src").join("lib.rs"))
}
