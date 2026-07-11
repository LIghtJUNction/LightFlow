use super::{PROJECT_LIGHTFLOW_DIR, WORKFLOW_DIR, read_dir_paths};
use crate::api::{ApiError, ApiResult};
use std::fs;
use std::io;
use std::path::Path;
use toml_edit::{DocumentMut, Item};

pub(in crate::api) fn ensure_workflow_save_workspace(root: &Path) -> ApiResult<()> {
    let manifest = root.join("Cargo.toml");
    let source = fs::read_to_string(&manifest).map_err(|error| {
        if error.kind() == io::ErrorKind::NotFound {
            workflow_save_workspace_error(root)
        } else {
            ApiError::from(error)
        }
    })?;
    let document = source.parse::<DocumentMut>().map_err(|error| {
        ApiError::InvalidRequest(format!(
            "invalid Cargo manifest {}: {error}; run `lfw init` before saving workflows",
            manifest.display()
        ))
    })?;
    let has_official_member = document
        .get("workspace")
        .and_then(|workspace| workspace.get("members"))
        .and_then(Item::as_array)
        .is_some_and(|members| {
            members
                .iter()
                .any(|member| member.as_str() == Some(".lightflow/workflows/*"))
        });
    let has_core_lightflow = document
        .get("workspace")
        .and_then(|workspace| workspace.get("dependencies"))
        .and_then(Item::as_table_like)
        .is_some_and(|dependencies| {
            dependencies
                .iter()
                .any(|(name, dependency)| is_core_lightflow_dependency(name, dependency))
        });
    if has_official_member && has_core_lightflow {
        Ok(())
    } else {
        Err(workflow_save_workspace_error(root))
    }
}

fn workflow_save_workspace_error(root: &Path) -> ApiError {
    ApiError::InvalidRequest(format!(
        "{} is not an initialized LightFlow workflow workspace; run `lfw init` before saving workflows",
        root.display()
    ))
}

pub(super) fn should_skip_empty_template_workspace_metadata(manifest: &Path) -> ApiResult<bool> {
    let source = fs::read_to_string(manifest).map_err(ApiError::from)?;
    let document = source.parse::<DocumentMut>().map_err(|error| {
        ApiError::InvalidRequest(format!("invalid Cargo manifest {:?}: {error}", manifest))
    })?;
    if document.get("package").is_some() && !is_generated_workflow_host(&document) {
        return Ok(false);
    }
    let Some(members) = document
        .get("workspace")
        .and_then(|workspace| workspace.get("members"))
        .and_then(Item::as_array)
    else {
        return Ok(false);
    };
    if members.len() != 1 {
        return Ok(false);
    }

    let root = manifest.parent().ok_or_else(|| {
        ApiError::InvalidRequest(format!("manifest {:?} has no parent", manifest))
    })?;
    if !has_only_core_lightflow_dependencies(manifest)? {
        return Ok(false);
    }
    let collection = match members.get(0).and_then(|member| member.as_str()) {
        Some(".lightflow/workflows/*") => root.join(PROJECT_LIGHTFLOW_DIR).join(WORKFLOW_DIR),
        Some("workflows/*") => root.join(WORKFLOW_DIR),
        _ => return Ok(false),
    };
    match read_dir_paths(&collection) {
        Ok(crates) => {
            for crate_dir in crates.into_iter().filter(|path| path.is_dir()) {
                let crate_manifest = crate_dir.join("Cargo.toml");
                if crate_manifest.is_file()
                    && !has_only_core_lightflow_dependencies(&crate_manifest)?
                {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(true),
        Err(error) => Err(ApiError::from(error)),
    }
}

fn is_generated_workflow_host(document: &DocumentMut) -> bool {
    document
        .get("package")
        .and_then(|package| package.get("publish"))
        .and_then(Item::as_bool)
        == Some(false)
        && document
            .get("lib")
            .and_then(|lib| lib.get("path"))
            .and_then(Item::as_str)
            == Some(".lightflow/workspace.rs")
}

fn has_only_core_lightflow_dependencies(manifest: &Path) -> ApiResult<bool> {
    let source = fs::read_to_string(manifest).map_err(ApiError::from)?;
    let document = source.parse::<DocumentMut>().map_err(|error| {
        ApiError::InvalidRequest(format!("invalid Cargo manifest {:?}: {error}", manifest))
    })?;
    if !dependency_table_has_only_core_lightflow(document.get("dependencies")) {
        return Ok(false);
    }
    let target_dependencies_are_core = document
        .get("target")
        .and_then(Item::as_table_like)
        .is_none_or(|targets| {
            targets.iter().all(|(_target, config)| {
                dependency_table_has_only_core_lightflow(config.get("dependencies"))
            })
        });
    Ok(target_dependencies_are_core)
}

fn dependency_table_has_only_core_lightflow(dependencies: Option<&Item>) -> bool {
    dependencies
        .and_then(Item::as_table_like)
        .is_none_or(|dependencies| {
            dependencies
                .iter()
                .all(|(key, dependency)| is_core_lightflow_dependency(key, dependency))
        })
}

fn is_core_lightflow_dependency(name: &str, dependency: &Item) -> bool {
    name == "lightflow" || dependency.get("package").and_then(Item::as_str) == Some("lightflow")
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
