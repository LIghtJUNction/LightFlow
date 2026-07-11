use super::dsl::{read_optional_workflow_source, read_workflow_source};
use super::project_config::default_project_workflow_sources;
use super::{ApiError, ApiResult, LEGACY_LIGHTFLOW_DIR, PROJECT_LIGHTFLOW_DIR, WORKFLOW_DIR};
use crate::workflow::WorkflowSpec;
use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

mod cargo_dependencies;
mod path_dependencies;
mod template_workspace;

pub(super) use template_workspace::ensure_workflow_save_workspace;

pub(super) fn read_workflow_sources(
    root: &Path,
    workflow_paths: &[PathBuf],
) -> ApiResult<Vec<WorkflowSpec>> {
    let mut workflows = Vec::new();
    let mut manifests = BTreeSet::new();
    let mut visited_libs = BTreeSet::new();
    read_workflow_collection(
        &project_workflow_collection(root),
        true,
        &mut workflows,
        &mut manifests,
        &mut visited_libs,
    )?;
    read_workflow_collection(
        &root.join(WORKFLOW_DIR),
        true,
        &mut workflows,
        &mut manifests,
        &mut visited_libs,
    )?;
    read_workflow_collection(
        &root.join(LEGACY_LIGHTFLOW_DIR).join(WORKFLOW_DIR),
        true,
        &mut workflows,
        &mut manifests,
        &mut visited_libs,
    )?;
    read_project_workflow_collections(root, &mut workflows, &mut manifests, &mut visited_libs)?;

    for path in workflow_paths {
        read_workflow_search_path(path, &mut workflows, &mut manifests, &mut visited_libs)?;
    }

    let root_manifest = root.join("Cargo.toml");
    if root_manifest.exists() {
        manifests.insert(normalize_existing_path(&root_manifest)?);
    }
    path_dependencies::read_path_dependency_workflows(
        &mut workflows,
        &mut manifests,
        &mut visited_libs,
    )?;
    if root_manifest.exists()
        && !template_workspace::should_skip_empty_template_workspace_metadata(&root_manifest)?
    {
        cargo_dependencies::read_cargo_dependency_workflows(
            &root_manifest,
            &mut workflows,
            &mut manifests,
            &mut visited_libs,
        )?;
    }

    Ok(workflows)
}

fn read_project_workflow_collections(
    root: &Path,
    workflows: &mut Vec<WorkflowSpec>,
    manifests: &mut BTreeSet<PathBuf>,
    visited_libs: &mut BTreeSet<PathBuf>,
) -> ApiResult<()> {
    let projects = root.join("projects");
    let default_sources = default_project_workflow_sources(root)?
        .into_iter()
        .collect::<BTreeSet<_>>();
    let paths = match read_dir_paths(&projects) {
        Ok(paths) => paths,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(ApiError::from(error)),
    };

    for project in paths {
        if !project.is_dir() {
            continue;
        }
        let Some(name) = project.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        // Configured default sources are the baseline local node set. Other
        // sibling projects remain opt-in through LFW_PATH, lfw import, or
        // explicit workflow search paths.
        if !default_sources.contains(name) {
            continue;
        }
        read_workflow_collection(
            &project_workflow_collection(&project),
            true,
            workflows,
            manifests,
            visited_libs,
        )?;
        read_workflow_collection(
            &project.join(WORKFLOW_DIR),
            true,
            workflows,
            manifests,
            visited_libs,
        )?;
    }

    Ok(())
}

fn read_workflow_search_path(
    path: &Path,
    workflows: &mut Vec<WorkflowSpec>,
    manifests: &mut BTreeSet<PathBuf>,
    visited_libs: &mut BTreeSet<PathBuf>,
) -> ApiResult<()> {
    if !path.exists() {
        return Ok(());
    }
    let project_workflows = project_workflow_collection(path);
    if project_workflows.is_dir() {
        let manifest = path.join("Cargo.toml");
        if manifest.exists() {
            manifests.insert(normalize_existing_path(&manifest)?);
        }
        return read_workflow_collection(
            &project_workflows,
            false,
            workflows,
            manifests,
            visited_libs,
        );
    }
    if path.join(WORKFLOW_DIR).is_dir() {
        let manifest = path.join("Cargo.toml");
        if manifest.exists() {
            manifests.insert(normalize_existing_path(&manifest)?);
        }
        return read_workflow_collection(
            &path.join(WORKFLOW_DIR),
            false,
            workflows,
            manifests,
            visited_libs,
        );
    }
    if path.file_name().and_then(|name| name.to_str()) == Some(PROJECT_LIGHTFLOW_DIR) {
        return Ok(());
    }
    read_workflow_collection(path, false, workflows, manifests, visited_libs)
}

fn project_workflow_collection(root: &Path) -> PathBuf {
    root.join(PROJECT_LIGHTFLOW_DIR).join(WORKFLOW_DIR)
}

fn read_workflow_collection(
    collection: &Path,
    strict: bool,
    workflows: &mut Vec<WorkflowSpec>,
    manifests: &mut BTreeSet<PathBuf>,
    visited_libs: &mut BTreeSet<PathBuf>,
) -> ApiResult<()> {
    let manifest = collection.join("Cargo.toml");
    if manifest.exists() {
        manifests.insert(normalize_existing_path(&manifest)?);
    }
    match read_dir_paths(collection) {
        Ok(paths) => {
            for path in paths {
                if path.file_name().and_then(|name| name.to_str()) == Some("Cargo.toml") {
                    continue;
                }
                if path.is_dir() && path.join("src").join("lib.rs").exists() {
                    read_one_workflow_crate(
                        &path,
                        strict,
                        None,
                        workflows,
                        manifests,
                        visited_libs,
                    )?;
                    continue;
                }
                if path.is_dir() {
                    continue;
                }
            }
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(ApiError::from(error)),
    }
    Ok(())
}

fn read_one_workflow_crate(
    crate_dir: &Path,
    strict: bool,
    category: Option<&str>,
    workflows: &mut Vec<WorkflowSpec>,
    manifests: &mut BTreeSet<PathBuf>,
    visited_libs: &mut BTreeSet<PathBuf>,
) -> ApiResult<()> {
    let lib = crate_dir.join("src").join("lib.rs");
    if !lib.exists() {
        return Ok(());
    }
    let lib = normalize_existing_path(&lib)?;
    if !visited_libs.insert(lib.clone()) {
        return Ok(());
    }
    let workflow = if strict {
        Some(read_workflow_source(&lib)?)
    } else {
        read_optional_workflow_source(&lib)?
    };
    if let Some(mut workflow) = workflow {
        if let Some(category) = category {
            workflow.category = Some(category.to_owned());
        }
        workflows.push(workflow);
        let manifest = crate_dir.join("Cargo.toml");
        if manifest.exists() {
            manifests.insert(normalize_existing_path(&manifest)?);
        }
    }
    Ok(())
}

fn read_dir_paths(path: &Path) -> Result<Vec<PathBuf>, io::Error> {
    let mut paths = fs::read_dir(path)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<Vec<_>, _>>()?;
    paths.sort();
    Ok(paths)
}

fn normalize_existing_path(path: &Path) -> ApiResult<PathBuf> {
    path.canonicalize().map_err(ApiError::from)
}
