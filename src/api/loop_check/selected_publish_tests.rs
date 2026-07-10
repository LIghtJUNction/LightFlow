use super::test_support::{
    temp_root, test_workflow_manifest, write_test_extension_crate, write_test_workflow_crate,
};
use super::{ApiService, LocalLoopStatus};
use std::fs;

#[test]
fn selected_publish_check_includes_child_workflow_crates() -> Result<(), Box<dyn std::error::Error>>
{
    let root = temp_root();
    fs::create_dir_all(&root)?;
    write_test_workflow_crate(
        &root,
        "lightflow.blocked_child_publish",
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
workflow!()
    .name("Blocked Child Publish")
    .output("value", "json")
    .build()
}
"#,
    )?;
    write_test_workflow_crate(
        &root,
        "lightflow.parent_publish_graph",
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
workflow!()
    .name("Parent Publish Graph")
    .output("value", "json")
    .node("child", "lightflow.blocked_child_publish")
    .build()
}
"#,
    )?;
    fs::write(
        test_workflow_manifest(&root, "lightflow.parent_publish_graph"),
        r#"[package]
name = "lightflow-parent-publish-graph"
version = "0.1.0"
edition = "2024"
description = "Publishable parent fixture."
license = "MIT"

[dependencies]
lightflow = { workspace = true }
"#,
    )?;
    let service = ApiService::new(&root);

    let report = service.local_loop_check(Some("lightflow.parent_publish_graph"))?;

    assert!(
        report.checks.iter().any(|check| {
            check.id == "loop.selected.publish"
                && check.status == LocalLoopStatus::Warning
                && check
                    .message
                    .contains("1 of 2 selected workflow publish plan")
                && check.message.contains("lightflow.blocked_child_publish")
                && check.message.contains("package.publish is false")
        }),
        "loop checks:\n{:#?}",
        report.checks
    );
    assert!(report.next_commands.iter().any(|command| {
        command
            == &vec![
                "lfw".to_owned(),
                "publish".to_owned(),
                "lightflow.parent_publish_graph".to_owned(),
            ]
    }));
    assert!(report.next_commands.iter().any(|command| {
        command
            == &vec![
                "lfw".to_owned(),
                "publish".to_owned(),
                "--workflows".to_owned(),
            ]
    }));

    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn selected_publish_check_skips_external_dependency_workflows()
-> Result<(), Box<dyn std::error::Error>> {
    let root = temp_root();
    fs::create_dir_all(&root)?;
    fs::write(
        root.join("Cargo.toml"),
        r#"[package]
name = "test-root"
version = "0.1.0"
edition = "2024"

[dependencies]
lightflow-external-child = { path = "extensions/lightflow-external-child", version = "0.1.0" }
"#,
    )?;
    fs::create_dir_all(root.join("src"))?;
    fs::write(root.join("src/lib.rs"), "pub fn fixture() {}\n")?;
    write_test_extension_crate(
        &root,
        "lightflow.external_child",
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
workflow!()
    .name("External Child")
    .output("value", "json")
    .build()
}
"#,
    )?;
    write_test_workflow_crate(
        &root,
        "lightflow.local_parent_external_child",
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
workflow!()
    .name("Local Parent External Child")
    .output("value", "json")
    .node("child", "lightflow.external_child")
    .build()
}
"#,
    )?;
    fs::write(
        test_workflow_manifest(&root, "lightflow.local_parent_external_child"),
        r#"[package]
name = "lightflow-local-parent-external-child"
version = "0.1.0"
edition = "2024"
description = "Publishable parent fixture."
license = "MIT"

[dependencies]
lightflow = { workspace = true }
"#,
    )?;
    let service = ApiService::new(&root);

    let report = service.local_loop_check(Some("lightflow.local_parent_external_child"))?;

    assert!(
        report.checks.iter().any(|check| {
            check.id == "loop.selected.publish"
                && check.status == LocalLoopStatus::Passed
                && check.count == Some(1)
                && check
                    .message
                    .contains("dependency graph has publishable local crates")
        }),
        "loop checks:\n{:#?}",
        report.checks
    );
    assert!(!report.next_commands.iter().any(|command| {
        command
            == &vec![
                "lfw".to_owned(),
                "publish".to_owned(),
                "--workflows".to_owned(),
            ]
    }));

    let _ = fs::remove_dir_all(root);
    Ok(())
}
