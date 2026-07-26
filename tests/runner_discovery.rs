use lightflow::api::ApiService;
use lightflow::workflow::WorkflowExecutionOptions;
use serde_json::{Map, json};
use std::fs;

fn write_workflow(root: &std::path::Path, metadata: &str, body: &str) {
    let workflow = root.join(".lightflow/workflows/fixture");
    fs::create_dir_all(workflow.join("src/bin")).expect("workflow dirs");
    fs::write(
        workflow.join("Cargo.toml"),
        format!(
            "[package]\nname = \"lightflow-fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\
             {metadata}\n[dependencies]\nlightflow = \"0.1.4\"\n"
        ),
    )
    .expect("manifest");
    fs::write(
        workflow.join("src/lib.rs"),
        "use lightflow::preload::*;\n\
         pub fn define() -> WorkflowSpec {\n\
         workflow! { name: \"Fixture\", input \"value\": \"json\", output \"value\": \"json\" }\n\
         .build()\n}\n",
    )
    .expect("library");
    fs::write(workflow.join("src/bin/runner.rs"), body).expect("runner");
}

#[test]
fn list_get_and_plan_do_not_compile_or_execute_runner() {
    let root = tempfile::tempdir().expect("tempdir");
    write_workflow(
        root.path(),
        "[package.metadata.lightflow]\nrunner = \"runner\"\n",
        "compile_error!(\"catalog operations must not compile this target\");\n",
    );
    let service = ApiService::new(root.path());

    let list = service.list_workflows().expect("list");
    assert!(
        list.workflows
            .iter()
            .any(|workflow| workflow.id == "lightflow.fixture")
    );
    service.get_workflow("lightflow.fixture").expect("get");
    let plan = service.plan_workflow("lightflow.fixture").expect("plan");
    assert_eq!(plan.runtime.expect("runtime").executor_id, "runner.v1");
}

#[test]
fn explicit_runner_engine_without_runner_metadata_fails_closed_at_run() {
    let root = tempfile::tempdir().expect("tempdir");
    write_workflow(root.path(), "", "fn main() {}\n");
    let lib = root.path().join(".lightflow/workflows/fixture/src/lib.rs");
    fs::write(
        lib,
        "use lightflow::preload::*;\n\
         pub fn define() -> WorkflowSpec {\n\
         workflow! { name: \"Fixture\", output \"value\": \"json\" }\n\
         .builtin_runtime(\"runner\", \"lightflow.runner\", \"runner.v1\")\n\
         .build()\n}\n",
    )
    .expect("library");
    let service = ApiService::new(root.path());

    let error = service
        .execute_workflow("lightflow.fixture", WorkflowExecutionOptions::default())
        .expect_err("missing runner metadata");
    assert!(
        error
            .to_string()
            .contains("has no [package.metadata.lightflow] runner")
    );
}

#[test]
fn explicit_top_level_spec_cannot_inherit_catalog_runner_by_reused_id() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let service = ApiService::new(root);
    let workflow = service
        .get_workflow("lightflow.text_regex")
        .expect("catalog workflow");
    let options = WorkflowExecutionOptions {
        inputs: Map::from_iter([
            ("text".to_owned(), json!("cat 42")),
            ("pattern".to_owned(), json!(r"\d+")),
        ]),
        ..WorkflowExecutionOptions::default()
    };

    let error = service
        .execute_workflow_spec(&workflow, options)
        .expect_err("explicit spec must not inherit origin");
    assert!(
        error
            .to_string()
            .contains("without a discovered runner origin")
    );
}

#[test]
fn explicit_composite_spec_can_execute_discovered_runner_child() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let service = ApiService::new(root);
    let mut workflow = lightflow::workflow!()
        .name("Explicit Composite")
        .input("text", "text")
        .input("pattern", "text")
        .node("regex", "lightflow.text_regex")
        .build();
    workflow.id = "lightflow.explicit_composite".to_owned();
    workflow.version = "0.1.0".to_owned();
    let options = WorkflowExecutionOptions {
        inputs: Map::from_iter([
            ("text".to_owned(), json!("cat 42")),
            ("pattern".to_owned(), json!(r"\d+")),
        ]),
        ..WorkflowExecutionOptions::default()
    };

    let execution = service
        .execute_workflow_spec(&workflow, options)
        .expect("explicit composite");
    assert_eq!(execution.nodes.len(), 1);
    assert_eq!(execution.nodes[0].outputs["first_match"], "42");
    assert_eq!(
        execution.nodes[0]
            .runtime
            .as_ref()
            .expect("child runtime")
            .executor_id,
        "runner.v1"
    );
}
