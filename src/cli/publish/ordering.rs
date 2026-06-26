use super::workflow_crates::WorkflowPublishPlan;
use crate::cli::{CliError, CliResult};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use toml_edit::{DocumentMut, Item};

pub(super) fn workflow_package_by_dir_from_plans(
    plans: &[WorkflowPublishPlan],
) -> BTreeMap<PathBuf, String> {
    plans
        .iter()
        .filter_map(|plan| {
            plan.manifest_path
                .parent()
                .and_then(|dir| canonicalize_existing(dir).ok())
                .map(|dir| (dir, plan.package.clone()))
        })
        .collect()
}

pub(super) fn dedupe_workflow_publish_plans(plans: &mut Vec<WorkflowPublishPlan>) {
    let mut seen_workflows = BTreeSet::new();
    let mut deduped = Vec::new();
    for plan in std::mem::take(plans) {
        if let Some(workflow_id) = &plan.workflow_id
            && !seen_workflows.insert(workflow_id.clone())
        {
            continue;
        }
        deduped.push(plan);
    }
    *plans = deduped;
}

pub(super) fn order_workflow_publish_plans(
    plans: &mut Vec<WorkflowPublishPlan>,
    package_by_dir: &BTreeMap<PathBuf, String>,
) -> CliResult<()> {
    for plan in plans.iter_mut() {
        plan.internal_dependencies =
            internal_path_dependencies(&plan.manifest_path, package_by_dir)?;
    }

    let mut pending = std::mem::take(plans);
    let mut published = BTreeSet::new();
    let mut ordered = Vec::new();
    while !pending.is_empty() {
        let ready = pending
            .iter()
            .position(|plan| plan.internal_dependencies.is_subset(&published));
        let Some(index) = ready else {
            return Err(CliError::Usage(
                "workflow crate path dependencies contain a cycle".to_owned(),
            ));
        };
        let plan = pending.remove(index);
        published.insert(plan.package.clone());
        ordered.push(plan);
    }
    *plans = ordered;
    Ok(())
}

fn internal_path_dependencies(
    manifest_path: &Path,
    package_by_dir: &BTreeMap<PathBuf, String>,
) -> CliResult<BTreeSet<String>> {
    let source = fs::read_to_string(manifest_path)?;
    let document = source
        .parse::<DocumentMut>()
        .map_err(|error| CliError::Usage(format!("invalid Cargo manifest: {error}")))?;
    let manifest_dir = manifest_path.parent().unwrap_or_else(|| Path::new("."));
    let mut dependencies = BTreeSet::new();
    collect_internal_path_dependencies(
        document.get("dependencies"),
        manifest_dir,
        package_by_dir,
        &mut dependencies,
    )?;
    collect_internal_path_dependencies(
        document.get("build-dependencies"),
        manifest_dir,
        package_by_dir,
        &mut dependencies,
    )?;
    collect_internal_path_dependencies(
        document.get("dev-dependencies"),
        manifest_dir,
        package_by_dir,
        &mut dependencies,
    )?;
    Ok(dependencies)
}

fn collect_internal_path_dependencies(
    dependencies: Option<&Item>,
    manifest_dir: &Path,
    package_by_dir: &BTreeMap<PathBuf, String>,
    internal_dependencies: &mut BTreeSet<String>,
) -> CliResult<()> {
    let Some(dependencies) = dependencies.and_then(Item::as_table_like) else {
        return Ok(());
    };
    for (_name, dependency) in dependencies.iter() {
        let Some(path) = dependency.get("path").and_then(Item::as_str) else {
            continue;
        };
        let dependency_dir = manifest_dir.join(path);
        if let Ok(dependency_dir) = canonicalize_existing(&dependency_dir)
            && let Some(package) = package_by_dir.get(&dependency_dir)
        {
            internal_dependencies.insert(package.clone());
        }
    }
    Ok(())
}

pub(super) fn canonicalize_existing(path: &Path) -> io::Result<PathBuf> {
    path.canonicalize()
}
