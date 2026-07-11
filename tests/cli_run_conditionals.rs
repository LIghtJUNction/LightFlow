mod support;

use std::fs;
use support::*;

#[test]
fn lfw_run_executes_if_node_selected_branch() -> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    fs::create_dir_all(&root)?;
    fs::write(
        root.join("Cargo.toml"),
        r#"[workspace]
resolver = "3"
members = ["workflows/*"]

[workspace.dependencies]
lightflow = { path = "." }
"#,
    )?;
    write_workflow_crate(
        &root,
        "lightflow.then_branch",
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow!()
        .name("Then Branch")
        .input("flag", "boolean")
        .input("value", "text")
        .output("value", "text")
        .build()
}
"#,
    )?;
    write_workflow_crate(
        &root,
        "lightflow.else_branch",
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow!()
        .name("Else Branch")
        .input("flag", "boolean")
        .input("value", "text")
        .output("value", "text")
        .build()
}
"#,
    )?;
    write_workflow_crate(
        &root,
        "lightflow.conditional",
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow!()
        .name("Conditional")
        .input("flag", "boolean")
        .input("value", "text")
        .output("value", "text")
        .if_node("gate", "flag", true, "lightflow.then_branch", "lightflow.else_branch")
        .build()
}
"#,
    )?;

    let then_run = lfw(
        &root,
        [
            "run",
            "lightflow.conditional",
            "-i",
            "flag=true",
            "-i",
            "value=then",
        ],
    )?;
    assert_eq!(then_run["outputs"]["value"], "then");
    assert_eq!(
        then_run["nodes"][0]["selected_workflow_id"],
        "lightflow.then_branch"
    );

    let else_run = lfw(
        &root,
        [
            "run",
            "lightflow.conditional",
            "-i",
            "flag=false",
            "-i",
            "value=else",
        ],
    )?;
    assert_eq!(else_run["outputs"]["value"], "else");
    assert_eq!(
        else_run["nodes"][0]["selected_workflow_id"],
        "lightflow.else_branch"
    );

    let _ = fs::remove_dir_all(root);
    Ok(())
}
