use super::test_support::{std_project_path, temp_root, write_test_workflow_crate};
use super::{ApiService, LocalLoopStatus};
use std::fs;

#[test]
fn loop_check_warns_when_model_locks_are_not_ready() -> Result<(), Box<dyn std::error::Error>> {
    let root = temp_root();
    fs::create_dir_all(&root)?;
    let service = ApiService::new(&root).with_workflow_paths(vec![std_project_path()]);

    let report = service.local_loop_check(Some("lightflow.text_to_image"))?;

    assert!(report.checks.iter().any(|check| {
        check.id == "loop.models.readiness"
            && check.status == LocalLoopStatus::Warning
            && check.message.contains("model requirement")
            && check.message.contains("missing_lock")
            && check
                .message
                .contains("lfw sync <workflow_id> --auto-model --apply")
            && check.message.contains("lfw models requirements")
    }));
    assert!(report.checks.iter().any(|check| {
        check.id == "loop.selected.models"
            && check.status == LocalLoopStatus::Warning
            && check
                .message
                .contains("lightflow.text_to_image::image_model: model lock is missing_lock")
            && check
                .message
                .contains("lfw models requirements lightflow.text_to_image --blocked")
            && check
                .message
                .contains("lfw sync lightflow.text_to_image --auto-model --apply")
    }));

    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn selected_model_check_includes_child_workflow_models() -> Result<(), Box<dyn std::error::Error>> {
    let root = temp_root();
    fs::create_dir_all(&root)?;
    write_test_workflow_crate(
        &root,
        "lightflow.child_model",
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
workflow("lightflow.child_model")
    .version("0.1.0")
    .name("Child Model")
    .input("model", "text")
    .output("value", "json")
    .model("child_weights", "text-to-image")
    .input_model_requirement("model", "child_weights")
    .build()
}
"#,
    )?;
    write_test_workflow_crate(
        &root,
        "lightflow.parent_model_graph",
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
workflow("lightflow.parent_model_graph")
    .version("0.1.0")
    .name("Parent Model Graph")
    .output("value", "json")
    .node("child", "lightflow.child_model")
    .build()
}
"#,
    )?;
    let service = ApiService::new(&root);

    let report = service.local_loop_check(Some("lightflow.parent_model_graph"))?;

    assert!(report.checks.iter().any(|check| {
        check.id == "loop.selected.models"
            && check.status == LocalLoopStatus::Warning
            && check.message.contains("1 of 1 selected model requirement")
            && check
                .message
                .contains("lightflow.child_model::child_weights: model lock is missing_lock")
    }));

    let _ = fs::remove_dir_all(root);
    Ok(())
}
