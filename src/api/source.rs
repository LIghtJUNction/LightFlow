use super::dsl::{read_optional_workflow_source, read_workflow_source};
use super::project_config::default_project_workflow_sources;
use super::util::{path_file_name, validate_id_segment};
use super::{ApiError, ApiResult, LEGACY_LIGHTFLOW_DIR, WORKFLOW_DIR};
use crate::workflow::WorkflowSpec;
use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use toml_edit::{DocumentMut, Item};

pub(super) fn read_workflow_sources(
    root: &Path,
    workflow_paths: &[PathBuf],
) -> ApiResult<Vec<WorkflowSpec>> {
    let mut workflows = Vec::new();
    let mut manifests = BTreeSet::new();
    let mut visited_libs = BTreeSet::new();
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
    read_path_dependency_workflows(&mut workflows, &mut manifests, &mut visited_libs)?;

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
    read_workflow_collection(path, false, workflows, manifests, visited_libs)
}

fn read_workflow_collection(
    collection: &Path,
    _strict: bool,
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
                if path.extension().and_then(|extension| extension.to_str()) != Some("rs") {
                    if path.is_dir() {
                        if path.join("src").join("lib.rs").exists() {
                            return Err(ApiError::InvalidRequest(format!(
                                "workflow crates must be inside a category directory: {:?}",
                                path
                            )));
                        } else {
                            read_workflow_category(&path, workflows, manifests, visited_libs)?;
                        }
                    }
                    continue;
                }
                return Err(ApiError::InvalidRequest(format!(
                    "workflow source files must be inside a category directory: {:?}",
                    path
                )));
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
        workflow.category = category.map(str::to_owned);
        workflows.push(workflow);
        let manifest = crate_dir.join("Cargo.toml");
        if manifest.exists() {
            manifests.insert(normalize_existing_path(&manifest)?);
        }
    }
    Ok(())
}

fn read_workflow_category(
    category_dir: &Path,
    workflows: &mut Vec<WorkflowSpec>,
    manifests: &mut BTreeSet<PathBuf>,
    visited_libs: &mut BTreeSet<PathBuf>,
) -> ApiResult<()> {
    let category = path_file_name(category_dir, "workflow category")?;
    validate_id_segment(&category, "workflow category")?;
    match read_dir_paths(category_dir) {
        Ok(paths) => {
            for path in paths {
                if path.is_dir() && path.join("src").join("lib.rs").exists() {
                    read_one_workflow_crate(
                        &path,
                        true,
                        Some(&category),
                        workflows,
                        manifests,
                        visited_libs,
                    )?;
                    continue;
                }
                if path.is_dir() {
                    return Err(ApiError::InvalidRequest(format!(
                        "nested workflow category directories are not supported: {:?}",
                        path
                    )));
                }
                return Err(ApiError::InvalidRequest(format!(
                    "workflow categories must contain standard Rust workflow crates: {:?}",
                    path
                )));
            }
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(ApiError::from(error)),
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

fn read_path_dependency_workflows(
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

            let lib = dependency_dir.join("src").join("lib.rs");
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

fn dependency_category(dependency_dir: &Path) -> Option<String> {
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

fn normalize_existing_path(path: &Path) -> ApiResult<PathBuf> {
    path.canonicalize().map_err(ApiError::from)
}
