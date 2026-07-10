use super::normalize_existing_path;
use crate::api::dsl::read_optional_workflow_source;
use crate::api::util::validate_id_segment;
use crate::api::{ApiError, ApiResult, WORKFLOW_DIR};
use crate::workflow::WorkflowSpec;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use toml_edit::{DocumentMut, Item};

pub(super) fn read_path_dependency_workflows(
    workflows: &mut Vec<WorkflowSpec>,
    manifests: &mut BTreeSet<PathBuf>,
    visited_libs: &mut BTreeSet<PathBuf>,
) -> ApiResult<()> {
    let mut scanned = BTreeSet::new();
    while let Some(manifest) = manifests
        .iter()
        .find(|manifest| !scanned.contains(*manifest))
        .cloned()
    {
        scanned.insert(manifest.clone());
        for dependency_dir in cargo_path_dependencies(&manifest)? {
            let manifest = dependency_dir.join("Cargo.toml");
            if manifest.exists() {
                manifests.insert(normalize_existing_path(&manifest)?);
            }
            let lib = dependency_dir.join("src/lib.rs");
            if !lib.exists() {
                continue;
            }
            let lib = normalize_existing_path(&lib)?;
            if !visited_libs.insert(lib.clone()) {
                continue;
            }
            if let Some(mut workflow) = read_optional_workflow_source(&lib)? {
                workflow.category = Some(
                    dependency_category(&dependency_dir).unwrap_or_else(|| "extensions".to_owned()),
                );
                workflows.push(workflow);
            }
        }
    }
    Ok(())
}

pub(super) fn dependency_category(dependency_dir: &Path) -> Option<String> {
    let category = dependency_dir.parent()?.file_name()?.to_str()?;
    let marker = dependency_dir.parent()?.parent()?.file_name()?.to_str()?;
    if marker == WORKFLOW_DIR && validate_id_segment(category, "workflow category").is_ok() {
        Some(category.to_owned())
    } else {
        None
    }
}

fn cargo_path_dependencies(manifest: &Path) -> ApiResult<Vec<PathBuf>> {
    let manifest_dir = manifest.parent().ok_or_else(|| {
        ApiError::InvalidRequest(format!("manifest {:?} has no parent", manifest))
    })?;
    let source = fs::read_to_string(manifest).map_err(ApiError::from)?;
    let document = source.parse::<DocumentMut>().map_err(|error| {
        ApiError::InvalidRequest(format!("invalid Cargo manifest {:?}: {error}", manifest))
    })?;
    let mut paths = Vec::new();
    collect_dependency_paths(manifest_dir, document.get("dependencies"), &mut paths)?;
    collect_dependency_paths(
        manifest_dir,
        document
            .get("workspace")
            .and_then(|workspace| workspace.get("dependencies")),
        &mut paths,
    )?;
    Ok(paths)
}

fn collect_dependency_paths(
    manifest_dir: &Path,
    dependencies: Option<&Item>,
    paths: &mut Vec<PathBuf>,
) -> ApiResult<()> {
    let Some(dependencies) = dependencies.and_then(Item::as_table_like) else {
        return Ok(());
    };
    for (_name, dependency) in dependencies.iter() {
        let Some(path) = dependency.get("path").and_then(Item::as_str) else {
            continue;
        };
        let path = manifest_dir.join(path);
        if path.exists() {
            paths.push(normalize_existing_path(&path)?);
        }
    }
    Ok(())
}
