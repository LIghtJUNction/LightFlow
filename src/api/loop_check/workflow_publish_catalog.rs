use super::project_workspace_inspection::discover_present_project_workspaces;
use super::publish_readiness::{
    internal_path_dependencies_from_document, order_workflow_publish_checks,
    workflow_package_by_dir, workflow_publish_check_from_manifest_document,
};
use super::workflow_crates::{discover_local_workflow_crates, workflow_id_from_crate};
use super::{
    ApiError, ApiResult, ApiService, WorkflowPublishCatalog, WorkflowPublishCheck,
    WorkflowPublishOptions,
};
use crate::api::project_filter_matches;
use manifest_cache::{cached_manifest_document, cached_workspace_document};
pub(super) use publish_loop_check::push_publish_check;
use std::collections::BTreeMap;
use std::path::PathBuf;

mod manifest_cache;
mod publish_check_lookup;
mod publish_loop_check;

pub(super) fn workflow_publish_check_for_service(
    service: &ApiService,
    workflow_id: &str,
) -> ApiResult<WorkflowPublishCheck> {
    match publish_check_lookup::workflow_publish_check_at_root(service.repo_root(), workflow_id) {
        Ok(check) => Ok(check),
        Err(root_error) => {
            for workspace in discover_present_project_workspaces(service.repo_root())? {
                if let Ok(mut check) = publish_check_lookup::workflow_publish_check_from_search_path(
                    &workspace.root,
                    workflow_id,
                ) {
                    check.workspace = workspace.display_prefix.display().to_string();
                    return Ok(check);
                }
            }
            for path in &service.workflow_paths {
                if let Ok(check) =
                    publish_check_lookup::workflow_publish_check_from_search_path(path, workflow_id)
                {
                    return Ok(check);
                }
            }
            Err(root_error)
        }
    }
}

pub(super) fn workflow_publish_checks_with_options(
    service: &ApiService,
    options: &WorkflowPublishOptions,
) -> ApiResult<WorkflowPublishCatalog> {
    let root = service.repo_root();
    let scoped_project = if let Some(project) = options.project.as_deref() {
        let workspace = discover_present_project_workspaces(root)?
            .into_iter()
            .find(|workspace| {
                project_filter_matches(
                    project,
                    &workspace.name,
                    workspace.display_prefix.as_path(),
                    &workspace.root,
                )
            })
            .ok_or_else(|| {
                ApiError::InvalidRequest(format!(
                    "project workspace filter matched no workspace: {project}"
                ))
            })?;
        Some((project.to_owned(), workspace.name.clone(), workspace))
    } else {
        None
    };
    let mut checks = Vec::new();
    let mut package_by_dir = if scoped_project.is_none() {
        workflow_package_by_dir(root)?
    } else {
        BTreeMap::new()
    };
    let mut workspace_root_by_manifest = BTreeMap::new();
    let mut workspace_documents = BTreeMap::new();
    let mut manifest_documents = BTreeMap::new();
    if let Some((_project, _matched, workspace)) = &scoped_project {
        package_by_dir.extend(workflow_package_by_dir(&workspace.root)?);
        for crate_dir in discover_local_workflow_crates(&workspace.root)? {
            let workflow_id = workflow_id_from_crate(&crate_dir)?;
            let manifest = crate_dir.join("Cargo.toml");
            workspace_root_by_manifest.insert(manifest.clone(), workspace.root.clone());
            let workspace_document =
                cached_workspace_document(&mut workspace_documents, &workspace.root)?.as_ref();
            let document = cached_manifest_document(&mut manifest_documents, &manifest)?;
            let mut check = workflow_publish_check_from_manifest_document(
                &workflow_id,
                manifest,
                &workspace.root,
                document,
                workspace_document,
            )?;
            check.workspace = workspace.display_prefix.display().to_string();
            checks.push(check);
        }
    } else {
        for crate_dir in discover_local_workflow_crates(root)? {
            let workflow_id = workflow_id_from_crate(&crate_dir)?;
            let manifest = crate_dir.join("Cargo.toml");
            workspace_root_by_manifest.insert(manifest.clone(), root.to_path_buf());
            let workspace_document =
                cached_workspace_document(&mut workspace_documents, root)?.as_ref();
            let document = cached_manifest_document(&mut manifest_documents, &manifest)?;
            checks.push(workflow_publish_check_from_manifest_document(
                &workflow_id,
                manifest,
                root,
                document,
                workspace_document,
            )?);
        }
        for workspace in discover_present_project_workspaces(root)? {
            package_by_dir.extend(workflow_package_by_dir(&workspace.root)?);
            for crate_dir in discover_local_workflow_crates(&workspace.root)? {
                let workflow_id = workflow_id_from_crate(&crate_dir)?;
                if checks.iter().any(|check| check.workflow_id == workflow_id) {
                    continue;
                }
                let manifest = crate_dir.join("Cargo.toml");
                workspace_root_by_manifest.insert(manifest.clone(), workspace.root.clone());
                let workspace_document =
                    cached_workspace_document(&mut workspace_documents, &workspace.root)?.as_ref();
                let document = cached_manifest_document(&mut manifest_documents, &manifest)?;
                let mut check = workflow_publish_check_from_manifest_document(
                    &workflow_id,
                    manifest,
                    &workspace.root,
                    document,
                    workspace_document,
                )?;
                check.workspace = workspace.display_prefix.display().to_string();
                checks.push(check);
            }
        }
        for path in &service.workflow_paths {
            package_by_dir.extend(workflow_package_by_dir(path)?);
            for crate_dir in discover_local_workflow_crates(path)? {
                let workflow_id = workflow_id_from_crate(&crate_dir)?;
                if checks.iter().any(|check| check.workflow_id == workflow_id) {
                    continue;
                }
                let manifest = crate_dir.join("Cargo.toml");
                workspace_root_by_manifest.insert(manifest.clone(), path.to_path_buf());
                let workspace_document =
                    cached_workspace_document(&mut workspace_documents, path)?.as_ref();
                let document = cached_manifest_document(&mut manifest_documents, &manifest)?;
                checks.push(workflow_publish_check_from_manifest_document(
                    &workflow_id,
                    manifest,
                    path,
                    document,
                    workspace_document,
                )?);
            }
        }
    }
    for check in &mut checks {
        let workspace_root = workspace_root_by_manifest
            .get(&check.manifest)
            .map(PathBuf::as_path)
            .unwrap_or(root);
        let workspace_document =
            cached_workspace_document(&mut workspace_documents, workspace_root)?.as_ref();
        let document = cached_manifest_document(&mut manifest_documents, &check.manifest)?;
        check.internal_dependencies = internal_path_dependencies_from_document(
            document,
            &check.manifest,
            workspace_document,
            workspace_root,
            &package_by_dir,
        )
        .into_iter()
        .collect();
    }
    order_workflow_publish_checks(&mut checks)?;

    let mut catalog = WorkflowPublishCatalog {
        project_root: root.to_path_buf(),
        project: None,
        project_filter_matched: None,
        matched_project_workspace: None,
        publishable: false,
        total: 0,
        publishable_count: 0,
        blocked_count: 0,
        commands: Vec::new(),
        checks,
        issues: Vec::new(),
    };

    if let Some((project, matched, _workspace)) = scoped_project {
        catalog.project = Some(project);
        catalog.project_filter_matched = Some(true);
        catalog.matched_project_workspace = Some(matched);
    }

    recompute_workflow_publish_catalog(&mut catalog);
    Ok(catalog)
}

fn recompute_workflow_publish_catalog(catalog: &mut WorkflowPublishCatalog) {
    catalog.total = catalog.checks.len();
    catalog.publishable_count = catalog
        .checks
        .iter()
        .filter(|check| check.publishable)
        .count();
    catalog.blocked_count = catalog.total.saturating_sub(catalog.publishable_count);
    catalog.publishable = catalog.total > 0 && catalog.blocked_count == 0;
    catalog.commands = catalog
        .checks
        .iter()
        .map(|check| check.command.clone())
        .collect();
    catalog.issues = catalog
        .checks
        .iter()
        .flat_map(|check| {
            check
                .issues
                .iter()
                .map(|issue| format!("{}: {issue}", check.workflow_id))
        })
        .collect();
}
