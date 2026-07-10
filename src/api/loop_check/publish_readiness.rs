use super::workflow_crates::discover_local_workflow_crates;
use super::{ApiError, ApiResult, WorkflowPublishCheck};
use crate::api::{
    cargo_manifest_api_error, cargo_publish_command, internal_path_dependency_packages,
    package_field_value, publish_issues, read_cargo_manifest, read_workspace_cargo_manifest,
    workflow_publish_metadata_issues,
};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use toml_edit::DocumentMut;

pub(super) fn workflow_publish_check_from_manifest(
    workflow_id: &str,
    manifest: PathBuf,
    root: &Path,
) -> ApiResult<WorkflowPublishCheck> {
    let workspace_document = read_publish_workspace_document(root)?;
    let mut check = workflow_publish_check_from_manifest_without_dependencies(
        workflow_id,
        manifest,
        root,
        workspace_document.as_ref(),
    )?;
    let package_by_dir = workflow_package_by_dir(root)?;
    check.internal_dependencies = internal_path_dependencies(
        &check.manifest,
        workspace_document.as_ref(),
        root,
        &package_by_dir,
    )?
    .into_iter()
    .collect();
    Ok(check)
}

pub(super) fn workflow_publish_check_from_manifest_without_dependencies(
    workflow_id: &str,
    manifest: PathBuf,
    root: &Path,
    workspace_document: Option<&DocumentMut>,
) -> ApiResult<WorkflowPublishCheck> {
    if !manifest.exists() {
        return Err(ApiError::NotFound(format!(
            "publish manifest does not exist: {}",
            manifest.display()
        )));
    }
    let document = read_cargo_manifest(&manifest).map_err(cargo_manifest_api_error)?;
    workflow_publish_check_from_manifest_document(
        workflow_id,
        manifest,
        root,
        &document,
        workspace_document,
    )
}

pub(super) fn workflow_publish_check_from_manifest_document(
    workflow_id: &str,
    manifest: PathBuf,
    root: &Path,
    document: &DocumentMut,
    workspace_document: Option<&DocumentMut>,
) -> ApiResult<WorkflowPublishCheck> {
    let mut issues = publish_issues(document, workspace_document);
    issues.extend(workflow_publish_metadata_issues(&manifest));
    let package = package_field(document, "name")?;
    let version = package_field(document, "version")?;
    Ok(WorkflowPublishCheck {
        workflow_id: workflow_id.to_owned(),
        package,
        version,
        workspace: publish_workspace_label(root, &manifest),
        command: cargo_publish_command(&manifest, true, false),
        publishable: issues.is_empty(),
        issues,
        internal_dependencies: Vec::new(),
        manifest,
    })
}

fn publish_workspace_label(root: &Path, manifest: &Path) -> String {
    let Ok(relative) = manifest.strip_prefix(root) else {
        return "external".to_owned();
    };
    let mut components = relative.components();
    match (components.next(), components.next()) {
        (Some(first), Some(name)) if first.as_os_str() == "projects" => {
            format!("projects/{}", name.as_os_str().to_string_lossy())
        }
        _ => "root".to_owned(),
    }
}

pub(super) fn categorized_workflow_manifest_path(
    root: &Path,
    workflow_id: &str,
) -> ApiResult<PathBuf> {
    crate::api::categorized_workflow_manifest_path(root, workflow_id).map_err(ApiError::Io)
}

pub(super) fn read_publish_workspace_document(root: &Path) -> ApiResult<Option<DocumentMut>> {
    read_workspace_cargo_manifest(root).map_err(cargo_manifest_api_error)
}

fn package_field(document: &DocumentMut, field: &str) -> ApiResult<String> {
    package_field_value(document, field).ok_or_else(|| {
        ApiError::InvalidRequest(format!("Cargo manifest is missing package.{field}"))
    })
}

pub(super) fn workflow_package_by_dir(root: &Path) -> ApiResult<BTreeMap<PathBuf, String>> {
    let mut packages = BTreeMap::new();
    for crate_dir in discover_local_workflow_crates(root)? {
        let manifest = crate_dir.join("Cargo.toml");
        let document = read_cargo_manifest(&manifest).map_err(cargo_manifest_api_error)?;
        if let Ok(package) = package_field(&document, "name")
            && let Ok(crate_dir) = crate_dir.canonicalize()
        {
            packages.insert(crate_dir, package);
        }
    }
    Ok(packages)
}

pub(super) fn internal_path_dependencies(
    manifest_path: &Path,
    workspace_document: Option<&DocumentMut>,
    workspace_root: &Path,
    package_by_dir: &BTreeMap<PathBuf, String>,
) -> ApiResult<BTreeSet<String>> {
    let document = read_cargo_manifest(manifest_path).map_err(cargo_manifest_api_error)?;
    Ok(internal_path_dependencies_from_document(
        &document,
        manifest_path,
        workspace_document,
        workspace_root,
        package_by_dir,
    ))
}

pub(super) fn internal_path_dependencies_from_document(
    document: &DocumentMut,
    manifest_path: &Path,
    workspace_document: Option<&DocumentMut>,
    workspace_root: &Path,
    package_by_dir: &BTreeMap<PathBuf, String>,
) -> BTreeSet<String> {
    let manifest_dir = manifest_path.parent().unwrap_or_else(|| Path::new("."));
    internal_path_dependency_packages(
        document,
        workspace_document,
        manifest_dir,
        workspace_root,
        package_by_dir,
    )
}

pub(super) fn order_workflow_publish_checks(
    checks: &mut Vec<WorkflowPublishCheck>,
) -> ApiResult<()> {
    let mut pending = std::mem::take(checks);
    pending.sort_by(|left, right| left.workflow_id.cmp(&right.workflow_id));
    let mut published = BTreeSet::new();
    let mut ordered = Vec::new();
    while !pending.is_empty() {
        let ready = pending.iter().position(|check| {
            check
                .internal_dependencies
                .iter()
                .all(|dependency| published.contains(dependency))
        });
        let Some(index) = ready else {
            return Err(ApiError::InvalidRequest(
                "workflow crate path dependencies contain a cycle".to_owned(),
            ));
        };
        let check = pending.remove(index);
        published.insert(check.package.clone());
        ordered.push(check);
    }
    *checks = ordered;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publish_workspace_label_reports_root_project_and_external_manifests() {
        let root = Path::new("/repo");

        assert_eq!(
            publish_workspace_label(root, &root.join("workflows/text/plan/Cargo.toml")),
            "root"
        );
        assert_eq!(
            publish_workspace_label(
                root,
                &root.join("projects/lightflow-std/workflows/std/text_prompt/Cargo.toml"),
            ),
            "projects/lightflow-std"
        );
        assert_eq!(
            publish_workspace_label(root, Path::new("/other/workflows/text/plan/Cargo.toml")),
            "external"
        );
    }

    #[test]
    fn order_workflow_publish_checks_orders_dependencies_first() {
        let mut checks = vec![
            check("lightflow.a", "a", ["b"]),
            check("lightflow.b", "b", []),
        ];

        order_workflow_publish_checks(&mut checks).expect("publish checks order");

        assert_eq!(
            checks
                .iter()
                .map(|check| check.package.as_str())
                .collect::<Vec<_>>(),
            vec!["b", "a"]
        );
    }

    #[test]
    fn order_workflow_publish_checks_rejects_dependency_cycles() {
        let mut checks = vec![
            check("lightflow.a", "a", ["b"]),
            check("lightflow.b", "b", ["a"]),
        ];

        let error = order_workflow_publish_checks(&mut checks).expect_err("dependency cycle");

        assert!(error.to_string().contains("cycle"));
    }

    fn check<const N: usize>(
        workflow_id: &str,
        package: &str,
        internal_dependencies: [&str; N],
    ) -> WorkflowPublishCheck {
        WorkflowPublishCheck {
            workflow_id: workflow_id.to_owned(),
            package: package.to_owned(),
            version: "0.1.0".to_owned(),
            workspace: "root".to_owned(),
            manifest: PathBuf::from(format!("{package}/Cargo.toml")),
            publishable: true,
            issues: Vec::new(),
            command: Vec::new(),
            internal_dependencies: internal_dependencies
                .into_iter()
                .map(ToOwned::to_owned)
                .collect(),
        }
    }
}
