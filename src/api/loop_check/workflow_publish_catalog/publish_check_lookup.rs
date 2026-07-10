use super::super::publish_readiness::{
    categorized_workflow_manifest_path, workflow_publish_check_from_manifest,
};
use super::super::workflow_crates::{
    discover_local_workflow_crates, discover_workflow_collection_crates, workflow_id_from_crate,
};
use super::super::{ApiError, ApiResult, WorkflowPublishCheck};
use std::path::Path;

pub(super) fn workflow_publish_check_at_root(
    root: &Path,
    workflow_id: &str,
) -> ApiResult<WorkflowPublishCheck> {
    let manifest = categorized_workflow_manifest_path(root, workflow_id)?;
    workflow_publish_check_from_manifest(workflow_id, manifest, root)
}

pub(super) fn workflow_publish_check_from_search_path(
    path: &Path,
    workflow_id: &str,
) -> ApiResult<WorkflowPublishCheck> {
    if let Ok(check) = workflow_publish_check_from_crate(path, workflow_id) {
        return Ok(check);
    }
    if path.join(".lightflow").join("workflows").is_dir() || path.join("workflows").is_dir() {
        if let Ok(check) = workflow_publish_check_at_root(path, workflow_id) {
            return Ok(check);
        }
        for crate_dir in discover_local_workflow_crates(path)? {
            if workflow_id_from_crate(&crate_dir)? == workflow_id {
                return workflow_publish_check_from_manifest(
                    workflow_id,
                    crate_dir.join("Cargo.toml"),
                    path,
                );
            }
        }
    }
    for crate_dir in discover_workflow_collection_crates(path)? {
        if workflow_id_from_crate(&crate_dir)? == workflow_id {
            return workflow_publish_check_from_manifest(
                workflow_id,
                crate_dir.join("Cargo.toml"),
                path,
            );
        }
    }
    Err(ApiError::NotFound(format!("workflow {workflow_id}")))
}

fn workflow_publish_check_from_crate(
    crate_dir: &Path,
    workflow_id: &str,
) -> ApiResult<WorkflowPublishCheck> {
    let manifest = crate_dir.join("Cargo.toml");
    let lib = crate_dir.join("src").join("lib.rs");
    if !manifest.exists() || !lib.exists() {
        return Err(ApiError::NotFound(format!(
            "workflow crate does not exist: {}",
            crate_dir.display()
        )));
    }
    if workflow_id_from_crate(crate_dir)? != workflow_id {
        return Err(ApiError::NotFound(format!("workflow {workflow_id}")));
    }
    let root = crate_dir.parent().unwrap_or(crate_dir);
    workflow_publish_check_from_manifest(workflow_id, manifest, root)
}
