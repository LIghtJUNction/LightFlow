use super::workflow_crates::WorkflowPublishPlan;
use crate::api::internal_path_dependency_packages;
use crate::cli::{CliError, CliResult};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use toml_edit::DocumentMut;

pub(super) fn workflow_package_by_dir_from_plans(
    plans: &[WorkflowPublishPlan],
) -> BTreeMap<PathBuf, String> {
    plans
        .iter()
        .filter_map(|plan| {
            plan.manifest_path
                .parent()
                .and_then(|dir| dir.canonicalize().ok())
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
    workspace_documents: &BTreeMap<PathBuf, Option<DocumentMut>>,
) -> CliResult<()> {
    for plan in plans.iter_mut() {
        plan.internal_dependencies =
            internal_path_dependencies(plan, package_by_dir, workspace_documents)?;
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
    plan: &WorkflowPublishPlan,
    package_by_dir: &BTreeMap<PathBuf, String>,
    workspace_documents: &BTreeMap<PathBuf, Option<DocumentMut>>,
) -> CliResult<BTreeSet<String>> {
    let workspace_document = workspace_documents
        .get(&plan.workspace_root)
        .ok_or_else(|| {
            CliError::Usage(format!(
                "workspace manifest cache is missing for {}",
                plan.workspace_root.display()
            ))
        })?
        .as_ref();
    let manifest_dir = plan
        .manifest_path
        .parent()
        .unwrap_or_else(|| Path::new("."));
    Ok(internal_path_dependency_packages(
        &plan.manifest_document,
        workspace_document,
        manifest_dir,
        &plan.workspace_root,
        package_by_dir,
    ))
}

#[cfg(test)]
mod tests {
    use super::super::workflow_crates::workflow_publish_plan;
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn order_workflow_publish_plans_orders_path_dependencies_first() {
        let root = test_dir("publish-order");
        let a = workflow_crate(root.path(), "a", Some("b"));
        let b = workflow_crate(root.path(), "b", None);
        let mut plans = vec![plan(&a), plan(&b)];
        let package_by_dir = workflow_package_by_dir_from_plans(&plans);
        let workspace_documents = workspace_documents_for_plans(&plans);

        order_workflow_publish_plans(&mut plans, &package_by_dir, &workspace_documents)
            .expect("publish plan order");

        assert_eq!(
            plans
                .iter()
                .map(|plan| plan.package.as_str())
                .collect::<Vec<_>>(),
            vec!["b", "a"]
        );
    }

    #[test]
    fn order_workflow_publish_plans_rejects_path_dependency_cycles() {
        let root = test_dir("publish-cycle");
        let a = workflow_crate(root.path(), "a", Some("b"));
        let b = workflow_crate(root.path(), "b", Some("a"));
        let mut plans = vec![plan(&a), plan(&b)];
        let package_by_dir = workflow_package_by_dir_from_plans(&plans);
        let workspace_documents = workspace_documents_for_plans(&plans);

        let error = order_workflow_publish_plans(&mut plans, &package_by_dir, &workspace_documents)
            .expect_err("cycle");

        assert!(error.to_string().contains("cycle"));
    }

    #[test]
    fn order_workflow_publish_plans_orders_workspace_path_dependencies_first() {
        let root = test_dir("publish-workspace-order");
        fs::create_dir_all(root.path()).expect("workspace root");
        fs::write(
            root.path().join("Cargo.toml"),
            r#"
[workspace]

[workspace.dependencies]
b = { path = "b", version = "0.1.0" }
"#,
        )
        .expect("workspace manifest");
        let a = workspace_dependency_workflow_crate(root.path(), "a", "b");
        let b = workflow_crate(root.path(), "b", None);
        let mut plans = vec![plan_with_workspace_root(&a, root.path()), plan(&b)];
        let package_by_dir = workflow_package_by_dir_from_plans(&plans);
        let workspace_documents = workspace_documents_for_plans(&plans);

        order_workflow_publish_plans(&mut plans, &package_by_dir, &workspace_documents)
            .expect("publish plan order");

        assert_eq!(
            plans
                .iter()
                .map(|plan| plan.package.as_str())
                .collect::<Vec<_>>(),
            vec!["b", "a"]
        );
        assert_eq!(
            plans
                .iter()
                .find(|plan| plan.package == "a")
                .expect("dependent plan")
                .internal_dependencies,
            BTreeSet::from(["b".to_owned()])
        );
    }

    #[test]
    fn order_workflow_publish_plans_rejects_missing_workspace_document_cache() {
        let root = test_dir("publish-missing-workspace-cache");
        let manifest = workflow_crate(root.path(), "a", None);
        let mut plans = vec![plan(&manifest)];
        let package_by_dir = workflow_package_by_dir_from_plans(&plans);

        let error = order_workflow_publish_plans(&mut plans, &package_by_dir, &BTreeMap::new())
            .expect_err("missing workspace cache");

        assert!(
            error
                .to_string()
                .contains("workspace manifest cache is missing"),
            "error: {error}"
        );
    }

    #[test]
    fn dedupe_workflow_publish_plans_keeps_first_matching_workflow_id() {
        let root = test_dir("publish-dedupe");
        let first = workflow_crate(root.path(), "first", None);
        let duplicate = workflow_crate(root.path(), "duplicate", None);
        let anonymous = workflow_crate(root.path(), "anonymous", None);
        let mut first_plan = plan(&first);
        first_plan.workflow_id = Some("lightflow.shared".to_owned());
        let mut duplicate_plan = plan(&duplicate);
        duplicate_plan.workflow_id = Some("lightflow.shared".to_owned());
        let mut anonymous_plan = plan(&anonymous);
        anonymous_plan.workflow_id = None;
        let mut plans = vec![first_plan, duplicate_plan, anonymous_plan];

        dedupe_workflow_publish_plans(&mut plans);

        assert_eq!(
            plans
                .iter()
                .map(|plan| plan.package.as_str())
                .collect::<Vec<_>>(),
            vec!["first", "anonymous"]
        );
    }

    fn workflow_crate(root: &Path, package: &str, dependency: Option<&str>) -> PathBuf {
        let crate_dir = root.join(package);
        fs::create_dir_all(&crate_dir).expect("workflow crate dir");
        let dependencies = dependency
            .map(|dependency| {
                format!("\n[dependencies]\n{dependency} = {{ path = \"../{dependency}\" }}\n")
            })
            .unwrap_or_default();
        fs::write(
            crate_dir.join("Cargo.toml"),
            format!("[package]\nname = \"{package}\"\nversion = \"0.1.0\"\n{dependencies}"),
        )
        .expect("manifest");
        crate_dir.join("Cargo.toml")
    }

    fn workspace_dependency_workflow_crate(
        root: &Path,
        package: &str,
        dependency: &str,
    ) -> PathBuf {
        let crate_dir = root.join(package);
        fs::create_dir_all(&crate_dir).expect("workflow crate dir");
        fs::write(
            crate_dir.join("Cargo.toml"),
            format!(
                "[package]\nname = \"{package}\"\nversion = \"0.1.0\"\n\n[dependencies]\n{dependency} = {{ workspace = true }}\n"
            ),
        )
        .expect("manifest");
        crate_dir.join("Cargo.toml")
    }

    fn plan(manifest_path: &Path) -> WorkflowPublishPlan {
        plan_with_workspace_root(
            manifest_path,
            manifest_path.parent().unwrap_or_else(|| Path::new(".")),
        )
    }

    fn plan_with_workspace_root(
        manifest_path: &Path,
        workspace_root: &Path,
    ) -> WorkflowPublishPlan {
        workflow_publish_plan(manifest_path, "root", workspace_root, None, false, false)
            .expect("workflow publish plan")
    }

    fn workspace_documents_for_plans(
        plans: &[WorkflowPublishPlan],
    ) -> BTreeMap<PathBuf, Option<DocumentMut>> {
        plans
            .iter()
            .map(|plan| {
                let document = super::super::cargo::workspace_document(&plan.workspace_root)
                    .expect("workspace document");
                (plan.workspace_root.clone(), document)
            })
            .collect()
    }

    struct TestDir {
        path: PathBuf,
    }

    impl TestDir {
        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn test_dir(name: &str) -> TestDir {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock after unix epoch")
            .as_nanos();
        TestDir {
            path: std::env::temp_dir().join(format!(
                "lightflow-cli-publish-ordering-{name}-{}-{nanos}",
                std::process::id()
            )),
        }
    }
}
