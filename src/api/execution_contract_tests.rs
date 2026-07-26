use super::*;
use crate::workflow::{
    WorkflowExecutionOptions, WorkflowNode, WorkflowNodeKind, WorkflowPosition,
    workflow_with_identity,
};
use std::collections::BTreeMap;
use std::fs;

fn write_required_input_crate(root: &std::path::Path, package: &str, version: &str) {
    let crate_dir = root.join(".lightflow/workflows").join(package);
    fs::create_dir_all(crate_dir.join("src")).expect("crate dir");
    fs::write(
        crate_dir.join("Cargo.toml"),
        format!(
            "[package]\nname = {package:?}\nversion = {version:?}\nedition = \"2024\"\n\n[dependencies]\nlightflow = {{ path = {:?} }}\n",
            std::env::current_dir().expect("cwd").display()
        ),
    )
    .expect("manifest");
    fs::write(
        crate_dir.join("src/lib.rs"),
        r#"use lightflow::preload::*;
pub fn define() -> WorkflowSpec {
workflow! {
    name: "Fixture",
    input "value": "json" {
        required: true,
    }
    output "value": "json",
}
.build()
}
"#,
    )
    .expect("source");
}

fn host_workspace(root: &std::path::Path, members: &str) {
    fs::create_dir_all(root.join(".lightflow")).expect("host");
    fs::write(
        root.join("Cargo.toml"),
        format!(
            r#"[package]
name = "contract-host"
version = "0.0.0"
edition = "2024"
publish = false
[lib]
path = ".lightflow/workspace.rs"
[workspace]
resolver = "3"
members = [{members}]
"#
        ),
    )
    .expect("root manifest");
    fs::write(root.join(".lightflow/workspace.rs"), "pub fn host() {}\n").expect("workspace lib");
}

#[test]
fn execute_rejects_missing_required_inputs() {
    let root = tempfile::tempdir().expect("tempdir");
    host_workspace(root.path(), "\".lightflow/workflows/*\"");
    write_required_input_crate(root.path(), "lightflow-required-input", "0.1.0");

    let service = ApiService::new(root.path());
    let error = service
        .execute_workflow(
            "lightflow.required_input",
            WorkflowExecutionOptions::default(),
        )
        .expect_err("missing required input");
    assert!(
        error.message().contains("required input `value`"),
        "unexpected error: {}",
        error.message()
    );
}

#[test]
fn execution_options_enforce_types_ranges_and_paths() {
    let mut workflow = workflow_with_identity("lightflow.schema_contract", "0.1.0")
        .name("Schema contract")
        .input("count", "integer")
        .input("source", "path")
        .input("files", "path[]")
        .build();
    workflow.inputs[0].min = Some(1.0);
    workflow.inputs[0].max = Some(4.0);
    let options = WorkflowExecutionOptions {
        inputs: serde_json::Map::from_iter([
            ("count".to_owned(), serde_json::json!(9)),
            ("source".to_owned(), serde_json::json!("")),
            ("files".to_owned(), serde_json::json!(["ok", "bad\0path"])),
            ("extra".to_owned(), serde_json::json!(true)),
        ]),
        ..WorkflowExecutionOptions::default()
    };

    let error = validate_execution_options(
        &workflow,
        &BTreeMap::from([(workflow.id.clone(), workflow.clone())]),
        &options,
    )
    .expect_err("invalid values");
    assert!(error.message().contains("at most 4"));
    assert!(error.message().contains("type `path`"));
    assert!(error.message().contains("type `path[]`"));
    assert!(error.message().contains("unknown input `extra`"));
}

#[test]
fn graph_runner_rejects_missing_required_child_input() {
    let mut child = workflow_with_identity("lightflow.child", "0.1.0")
        .name("Child")
        .input("value", "text")
        .output("value", "text")
        .build();
    child.inputs[0].required = Some(true);
    let mut composite = workflow_with_identity("lightflow.parent", "0.1.0")
        .name("Parent")
        .output("value", "text")
        .build();
    composite.nodes.push(WorkflowNode {
        id: "child".to_owned(),
        kind: WorkflowNodeKind::Workflow,
        workflow_id: child.id.clone(),
        condition: None,
        then_workflow_id: None,
        else_workflow_id: None,
        title: None,
        disabled: false,
        position: WorkflowPosition::default(),
        config: serde_json::Value::Null,
    });
    let workflows = BTreeMap::from([
        (child.id.clone(), child),
        (composite.id.clone(), composite.clone()),
    ]);

    let error = execute_workflow_spec_impl(
        std::path::Path::new("."),
        &composite,
        &workflows,
        &BTreeMap::new(),
        WorkflowExecutionOptions::default(),
    )
    .expect_err("required child input");
    assert!(error.message().contains("required input `value`"));
}

#[test]
fn graph_runner_rejects_invalid_leaf_output_type() {
    let workflow = workflow_with_identity("lightflow.bad_output", "0.1.0")
        .name("Bad output")
        .input("value", "json")
        .output("value", "integer")
        .build();
    let options = WorkflowExecutionOptions {
        inputs: serde_json::Map::from_iter([(
            "value".to_owned(),
            serde_json::Value::String("not an integer".to_owned()),
        )]),
        ..WorkflowExecutionOptions::default()
    };

    let error = execute_workflow_spec_impl(
        std::path::Path::new("."),
        &workflow,
        &BTreeMap::from([(workflow.id.clone(), workflow.clone())]),
        &BTreeMap::new(),
        options,
    )
    .expect_err("invalid output");
    assert!(
        error
            .message()
            .contains("output `value` must have type `integer`")
    );
}

#[test]
fn workflow_catalog_rejects_duplicate_ids() {
    let root = tempfile::tempdir().expect("tempdir");
    host_workspace(root.path(), "\".lightflow/workflows/*\"");
    write_required_input_crate(root.path(), "lightflow-dup-flow", "0.1.0");

    // Second collection outside the host workspace with the same package → same workflow id.
    let extra = root.path().join("extra-home/workflows/lightflow-dup-flow");
    fs::create_dir_all(extra.join("src")).expect("extra dir");
    fs::write(
        extra.join("Cargo.toml"),
        format!(
            "[package]\nname = \"lightflow-dup-flow\"\nversion = \"0.2.0\"\nedition = \"2024\"\n\n[dependencies]\nlightflow = {{ path = {:?} }}\n",
            std::env::current_dir().expect("cwd").display()
        ),
    )
    .expect("extra manifest");
    fs::write(
        extra.join("src/lib.rs"),
        r#"use lightflow::preload::*;
pub fn define() -> WorkflowSpec {
workflow! { name: "Other", }.build()
}
"#,
    )
    .expect("extra source");

    let service = ApiService::new(root.path())
        .with_workflow_paths(vec![root.path().join("extra-home/workflows")]);
    let error = service.list_workflows().expect_err("duplicate id");
    assert!(
        error.message().contains("duplicate workflow id"),
        "unexpected error: {}",
        error.message()
    );
}

#[test]
fn composite_output_rejects_ambiguous_producers() {
    let leaf_a = workflow_with_identity("lightflow.leaf_a", "0.1.0")
        .name("Leaf A")
        .output("result", "text")
        .build();
    let leaf_b = workflow_with_identity("lightflow.leaf_b", "0.1.0")
        .name("Leaf B")
        .output("result", "text")
        .build();
    let mut composite = workflow_with_identity("lightflow.composite", "0.1.0")
        .name("Composite")
        .output("result", "text")
        .build();
    composite.nodes = vec![
        WorkflowNode {
            id: "a".to_owned(),
            kind: WorkflowNodeKind::Workflow,
            workflow_id: "lightflow.leaf_a".to_owned(),
            condition: None,
            then_workflow_id: None,
            else_workflow_id: None,
            title: None,
            disabled: false,
            position: WorkflowPosition::default(),
            config: serde_json::Value::Null,
        },
        WorkflowNode {
            id: "b".to_owned(),
            kind: WorkflowNodeKind::Workflow,
            workflow_id: "lightflow.leaf_b".to_owned(),
            condition: None,
            then_workflow_id: None,
            else_workflow_id: None,
            title: None,
            disabled: false,
            position: WorkflowPosition::default(),
            config: serde_json::Value::Null,
        },
    ];
    let mut workflows = BTreeMap::new();
    workflows.insert(leaf_a.id.clone(), leaf_a);
    workflows.insert(leaf_b.id.clone(), leaf_b);
    workflows.insert(composite.id.clone(), composite.clone());

    let validation = validation::validate_workflow_spec(&composite, &workflows);
    assert!(
        validation
            .issues
            .iter()
            .any(|issue| issue.contains("produced by multiple nodes")),
        "issues: {:?}",
        validation.issues
    );
}
