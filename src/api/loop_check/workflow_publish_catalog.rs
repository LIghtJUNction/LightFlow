use super::project_workspace_inspection::discover_present_project_workspaces;
use super::publish_readiness::{
    categorized_workflow_manifest_path, internal_path_dependencies, order_workflow_publish_checks,
    workflow_package_by_dir, workflow_publish_check_from_manifest,
};
use super::workflow_crates::{
    discover_local_workflow_crates, discover_workflow_collection_crates, workflow_id_from_crate,
};
use super::{
    ApiError, ApiResult, ApiService, LocalLoopCheck, ProjectWorkspaceOptions,
    WorkflowPublishCatalog, WorkflowPublishCheck, WorkflowPublishOptions,
};
use std::fs;
use std::path::Path;

pub(super) fn workflow_publish_check_for_service(
    service: &ApiService,
    workflow_id: &str,
) -> ApiResult<WorkflowPublishCheck> {
    match workflow_publish_check_at_root(service.repo_root(), workflow_id) {
        Ok(check) => Ok(check),
        Err(root_error) => {
            for workspace in discover_present_project_workspaces(service.repo_root())? {
                if let Ok(mut check) =
                    workflow_publish_check_from_search_path(&workspace.root, workflow_id)
                {
                    check.workspace = workspace.display_prefix.display().to_string();
                    return Ok(check);
                }
            }
            for path in &service.workflow_paths {
                if let Ok(check) = workflow_publish_check_from_search_path(path, workflow_id) {
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
    let mut checks = Vec::new();
    let mut package_by_dir = workflow_package_by_dir(root)?;
    for crate_dir in discover_local_workflow_crates(root)? {
        let workflow_id = workflow_id_from_crate(&crate_dir)?;
        checks.push(workflow_publish_check_from_manifest(
            &workflow_id,
            crate_dir.join("Cargo.toml"),
            root,
        )?);
    }
    for workspace in discover_present_project_workspaces(root)? {
        package_by_dir.extend(workflow_package_by_dir(&workspace.root)?);
        for crate_dir in discover_local_workflow_crates(&workspace.root)? {
            let workflow_id = workflow_id_from_crate(&crate_dir)?;
            if options.project.is_none()
                && checks.iter().any(|check| check.workflow_id == workflow_id)
            {
                continue;
            }
            let mut check = workflow_publish_check_from_manifest(
                &workflow_id,
                crate_dir.join("Cargo.toml"),
                &workspace.root,
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
            checks.push(workflow_publish_check_from_manifest(
                &workflow_id,
                crate_dir.join("Cargo.toml"),
                path,
            )?);
        }
    }
    for check in &mut checks {
        check.internal_dependencies = internal_path_dependencies(&check.manifest, &package_by_dir)?
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

    if let Some(project) = options.project.as_deref() {
        let project_catalog = service.project_workspaces_with_options(ProjectWorkspaceOptions {
            dirty_only: false,
            project: Some(project.to_owned()),
        })?;
        let matched = project_catalog.matched_project_workspace.ok_or_else(|| {
            ApiError::InvalidRequest(format!(
                "project workspace filter matched no workspace: {project}"
            ))
        })?;
        let workspace_label = format!("projects/{matched}");
        catalog
            .checks
            .retain(|check| check.workspace == workspace_label);
        catalog.project = Some(project.to_owned());
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

pub(super) fn push_publish_check(
    service: &ApiService,
    checks: &mut Vec<LocalLoopCheck>,
) -> ApiResult<()> {
    match service.workflow_publish_checks() {
        Ok(catalog) if catalog.total == 0 => {
            checks.push(LocalLoopCheck::warning(
                "loop.publish.workflow_crates",
                "no workflow crates found for lfw publish --workflows",
            ));
        }
        Ok(catalog) if catalog.publishable => {
            checks.push(
                LocalLoopCheck::passed(
                    "loop.publish.workflow_crates",
                    "workflow crates are present for lfw publish --workflows",
                )
                .count(catalog.total),
            );
            checks.push(
                LocalLoopCheck::passed(
                    "loop.publish.readiness",
                    "all workflow crates pass publish preflight checks",
                )
                .count(catalog.checks.len()),
            );
        }
        Ok(catalog) => {
            checks.push(
                LocalLoopCheck::passed(
                    "loop.publish.workflow_crates",
                    "workflow crates are present for lfw publish --workflows",
                )
                .count(catalog.total),
            );
            let blocked = catalog
                .checks
                .iter()
                .filter(|check| !check.publishable)
                .count();
            checks.push(
                LocalLoopCheck::warning(
                    "loop.publish.readiness",
                    format!(
                        "{blocked} of {} workflow crates are not publishable yet; inspect /publish or lfw publish --workflows",
                        catalog.checks.len()
                    ),
                )
                .count(catalog.issues.len()),
            );
        }
        Err(error) => checks.push(LocalLoopCheck::failed(
            "loop.publish.readiness",
            format!("workflow publish readiness could not be inspected: {error}"),
        )),
    }
    Ok(())
}

fn workflow_publish_check_at_root(
    root: &Path,
    workflow_id: &str,
) -> ApiResult<WorkflowPublishCheck> {
    let manifest = categorized_workflow_manifest_path(root, workflow_id)?;
    workflow_publish_check_from_manifest(workflow_id, manifest, root)
}

fn workflow_publish_check_from_search_path(
    path: &Path,
    workflow_id: &str,
) -> ApiResult<WorkflowPublishCheck> {
    if let Ok(check) = workflow_publish_check_from_crate(path, workflow_id) {
        return Ok(check);
    }
    if path.join("workflows").is_dir() {
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
    let source = fs::read_to_string(&lib)?;
    let needle = format!("workflow(\"{workflow_id}\")");
    if !source.contains(&needle) {
        return Err(ApiError::NotFound(format!("workflow {workflow_id}")));
    }
    let root = crate_dir.parent().unwrap_or(crate_dir);
    workflow_publish_check_from_manifest(workflow_id, manifest, root)
}
