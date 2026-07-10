mod support;

use std::fs;
use std::path::Path;
use std::process::Output;
use support::*;

#[test]
fn disabled_nested_runtime_is_not_reachable_but_enabled_nested_runtime_is()
-> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    fs::create_dir_all(&root)?;
    write_workflow_crate(
        &root,
        "lightflow.preview_shared",
        &preview_source("lightflow.preview_shared"),
    )?;
    write_workflow_crate(
        &root,
        "lightflow.abstract_nested_leaf",
        &abstract_source("lightflow.abstract_nested_leaf"),
    )?;
    write_workflow_crate(
        &root,
        "lightflow.nested_bad",
        &nested_source("lightflow.nested_bad", "lightflow.abstract_nested_leaf"),
    )?;
    write_workflow_crate(
        &root,
        "lightflow.disabled_nested_root",
        &nested_root_source("lightflow.disabled_nested_root", true),
    )?;
    write_workflow_crate(
        &root,
        "lightflow.enabled_nested_root",
        &nested_root_source("lightflow.enabled_nested_root", false),
    )?;
    write_skill(&root, "lightflow.disabled_nested_root")?;
    write_skill(&root, "lightflow.enabled_nested_root")?;

    let disabled = lfw_command(&root)
        .args(["node", "test", "lightflow.disabled_nested_root"])
        .output()?;
    assert!(
        disabled.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&disabled.stderr)
    );
    let disabled_check = runtime_check(&disabled)?;
    assert_eq!(disabled_check["status"], "passed");
    assert!(
        disabled_check["message"]
            .as_str()
            .expect("runtime message")
            .contains("1 reachable leaf executor(s)")
    );

    let enabled = lfw_command(&root)
        .args(["node", "test", "lightflow.enabled_nested_root"])
        .output()?;
    assert!(!enabled.status.success());
    let enabled_check = runtime_check(&enabled)?;
    assert_eq!(enabled_check["status"], "failed");
    assert!(
        enabled_check["message"]
            .as_str()
            .expect("runtime message")
            .contains("lightflow.abstract_nested_leaf")
    );

    let _ = fs::remove_dir_all(root);
    Ok(())
}

fn runtime_check(output: &Output) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let bytes = if output.status.success() {
        &output.stdout
    } else {
        &output.stderr
    };
    let report: serde_json::Value = serde_json::from_slice(bytes)?;
    Ok(report["checks"]
        .as_array()
        .expect("node checks")
        .iter()
        .find(|check| check["id"] == "node.runtime")
        .expect("node.runtime check")
        .clone())
}

fn write_skill(root: &Path, workflow_id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let crate_name = workflow_id
        .strip_prefix("lightflow.")
        .unwrap_or(workflow_id)
        .replace('.', "_");
    let skill_dir = root
        .join(".lightflow/workflows/tests")
        .join(crate_name)
        .join(".agent/skills/test-node");
    fs::create_dir_all(&skill_dir)?;
    fs::write(
        skill_dir.join("SKILL.md"),
        format!(
            "---\nname: test-node\ndescription: Test workflow node.\nversion: 0.1.0\n---\n\n`lfw run {workflow_id}`\n\nPOST `/workflows/{workflow_id}/run`\n"
        ),
    )?;
    Ok(())
}

fn preview_source(workflow_id: &str) -> String {
    format!(
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {{
    workflow("{workflow_id}")
        .version("0.1.0")
        .name("Preview Branch")
        .description("Runnable preview branch.")
        .input("flag", "boolean")
        .input_description("flag", "Conditional flag.")
        .output("image", "artifact")
        .output_description("image", "Preview image metadata.")
        .output_artifact_kind("image", "image")
        .builtin_runtime("image_runtime", "lightflow.image.generate", "builtin.preview.v1")
        .build()
}}
"#
    )
}

fn abstract_source(workflow_id: &str) -> String {
    preview_source(workflow_id).replace(
        ".builtin_runtime(\"image_runtime\", \"lightflow.image.generate\", \"builtin.preview.v1\")",
        ".runtime(\"image_runtime\", \"lightflow.image.generate\")",
    )
}

fn nested_source(workflow_id: &str, child_id: &str) -> String {
    format!(
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {{
    workflow("{workflow_id}")
        .version("0.1.0")
        .name("Nested Runtime")
        .description("Contains another composite runtime candidate.")
        .input("flag", "boolean")
        .input_description("flag", "Nested input flag.")
        .output("image", "artifact")
        .output_description("image", "Nested image metadata.")
        .output_artifact_kind("image", "image")
        .node("child", "{child_id}")
        .build()
}}
"#
    )
}

fn nested_root_source(workflow_id: &str, disabled: bool) -> String {
    let nested_node = if disabled {
        ".disabled_node(\"nested\", \"lightflow.nested_bad\")"
    } else {
        ".node(\"nested\", \"lightflow.nested_bad\")"
    };
    format!(
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {{
    workflow("{workflow_id}")
        .version("0.1.0")
        .name("Nested Root")
        .description("Checks disabled nested runtime reachability.")
        .input("flag", "boolean")
        .input_description("flag", "Root input flag.")
        .output("image", "artifact")
        .output_description("image", "Root image metadata.")
        .output_artifact_kind("image", "image")
        {nested_node}
        .node("preview", "lightflow.preview_shared")
        .build()
}}
"#
    )
}
