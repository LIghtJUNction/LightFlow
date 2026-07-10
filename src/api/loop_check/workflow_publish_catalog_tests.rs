use super::test_support::temp_root;
use crate::api::{ApiService, WorkflowPublishOptions};
use std::fs;
use std::path::Path;

#[test]
fn publish_catalog_resolves_project_workspace_dependencies_and_filters_duplicates()
-> Result<(), Box<dyn std::error::Error>> {
    let root = temp_root();
    fs::create_dir_all(&root)?;
    write_publishable_workflow(
        &root,
        "workflows/std/app",
        "lightflow.shared_app",
        "shared-app",
    )?;

    let project_root = root.join("projects/lightflow-std");
    fs::create_dir_all(&project_root)?;
    fs::write(
        project_root.join("Cargo.toml"),
        r#"
[workspace]
members = ["workflows/std/base", "workflows/std/app"]

[workspace.dependencies]
project-base = { path = "workflows/std/base", version = "0.1.0" }
"#,
    )?;
    write_publishable_workflow(
        &project_root,
        "workflows/std/base",
        "lightflow.project_base",
        "project-base",
    )?;
    write_publishable_workflow_with_dependencies(
        &project_root,
        "workflows/std/app",
        "lightflow.shared_app",
        "shared-app",
        "[dependencies]\nproject-base = { workspace = true }\n",
    )?;
    let service = ApiService::new(&root);

    let default_catalog = service.workflow_publish_checks()?;

    assert_eq!(default_catalog.total, 2);
    assert_eq!(
        default_catalog
            .checks
            .iter()
            .map(|check| check.package.as_str())
            .collect::<Vec<_>>(),
        vec!["project-base", "shared-app"]
    );
    assert!(
        default_catalog
            .checks
            .iter()
            .any(|check| check.workflow_id == "lightflow.shared_app"
                && check.workspace == "root"
                && check.package == "shared-app"),
        "default catalog:\n{default_catalog:#?}"
    );

    let project_catalog =
        service.workflow_publish_checks_with_options(&WorkflowPublishOptions {
            project: Some("std".to_owned()),
        })?;

    assert_eq!(project_catalog.project.as_deref(), Some("std"));
    assert_eq!(project_catalog.project_filter_matched, Some(true));
    assert_eq!(
        project_catalog.matched_project_workspace.as_deref(),
        Some("lightflow-std")
    );
    assert_eq!(
        project_catalog
            .checks
            .iter()
            .map(|check| check.package.as_str())
            .collect::<Vec<_>>(),
        vec!["project-base", "shared-app"]
    );
    let app = project_catalog
        .checks
        .iter()
        .find(|check| check.package == "shared-app")
        .expect("project app check");
    assert_eq!(app.workspace, "projects/lightflow-std");
    assert_eq!(app.internal_dependencies, vec!["project-base"]);

    let project_path = project_root.display().to_string();
    for project in ["lightflow-std", "projects/lightflow-std", &project_path] {
        let catalog = service.workflow_publish_checks_with_options(&WorkflowPublishOptions {
            project: Some(project.to_owned()),
        })?;
        assert_eq!(catalog.project.as_deref(), Some(project));
        assert_eq!(catalog.project_filter_matched, Some(true));
        assert_eq!(
            catalog.matched_project_workspace.as_deref(),
            Some("lightflow-std")
        );
        assert_eq!(
            catalog
                .checks
                .iter()
                .map(|check| check.package.as_str())
                .collect::<Vec<_>>(),
            vec!["project-base", "shared-app"]
        );
    }

    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn scoped_publish_catalog_ignores_unselected_manifest_errors()
-> Result<(), Box<dyn std::error::Error>> {
    let root = temp_root();
    fs::create_dir_all(&root)?;
    let broken_root_crate = root.join("workflows/std/broken");
    fs::create_dir_all(broken_root_crate.join("src"))?;
    fs::write(broken_root_crate.join("src/lib.rs"), "pub fn define() {}")?;
    fs::write(broken_root_crate.join("Cargo.toml"), "[package")?;

    let project_root = root.join("projects/lightflow-std");
    fs::create_dir_all(&project_root)?;
    write_publishable_workflow(
        &project_root,
        "workflows/std/app",
        "lightflow.project_app",
        "project-app",
    )?;

    let catalog =
        ApiService::new(&root).workflow_publish_checks_with_options(&WorkflowPublishOptions {
            project: Some("std".to_owned()),
        })?;

    assert_eq!(catalog.total, 1);
    assert_eq!(catalog.project_filter_matched, Some(true));
    assert_eq!(
        catalog.matched_project_workspace.as_deref(),
        Some("lightflow-std")
    );
    assert_eq!(catalog.checks[0].package, "project-app");

    let _ = fs::remove_dir_all(root);
    Ok(())
}

fn write_publishable_workflow(
    root: &Path,
    relative_crate_dir: &str,
    workflow_id: &str,
    package: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    write_publishable_workflow_with_dependencies(root, relative_crate_dir, workflow_id, package, "")
}

fn write_publishable_workflow_with_dependencies(
    root: &Path,
    relative_crate_dir: &str,
    _workflow_id: &str,
    package: &str,
    dependencies: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let crate_dir = root.join(relative_crate_dir);
    fs::create_dir_all(crate_dir.join("src"))?;
    fs::write(
        crate_dir.join("Cargo.toml"),
        format!(
            r#"
[package]
name = "{package}"
version = "0.1.0"
edition = "2024"
description = "Publishable workflow fixture."
license = "MIT"

{dependencies}"#
        ),
    )?;
    fs::write(
        crate_dir.join("src/lib.rs"),
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow!()
        .name("Publishable Fixture")
        .output("value", "json")
        .output_description("value", "Fixture output.")
        .build()
}
"#,
    )?;
    Ok(())
}
