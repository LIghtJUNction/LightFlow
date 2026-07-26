mod support;

use std::fs;
use std::process::Command;
use support::*;

#[test]
fn node_test_rejects_model_free_abstract_image_runtime() -> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    fs::create_dir_all(&root)?;
    write_workflow_crate(
        &root,
        "lightflow.abstract_image",
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow!()
        .name("Abstract Image")
        .description("A model-free abstract image runtime used for conformance testing.")
        .input("prompt", "text")
        .input_description("prompt", "Prompt to render.")
        .input_required("prompt", true)
        .output("image", "artifact")
        .output_description("image", "Generated image metadata.")
        .output_artifact_kind("image", "image")
        .output("image_path", "path")
        .output_description("image_path", "Generated image path.")
        .runtime("image_runtime", "lightflow.image.generate")
        .build()
}
"#,
    )?;

    let output = lfw_command(&root)
        .args(["node", "test", "lightflow.abstract_image"])
        .output()?;
    assert!(!output.status.success());
    let report: serde_json::Value = serde_json::from_slice(&output.stderr)?;
    let runtime_check = report["checks"]
        .as_array()
        .expect("node checks")
        .iter()
        .find(|check| check["id"] == "node.runtime")
        .expect("node.runtime check");
    assert_eq!(runtime_check["status"], "failed");
    assert!(
        runtime_check["message"]
            .as_str()
            .expect("runtime message")
            .contains("has no executor"),
        "expected actual plan failure, report:\n{report}"
    );

    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn image_generate_runtime_without_builtin_preview_is_rejected()
-> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    fs::create_dir_all(&root)?;
    write_workflow_crate(
        &root,
        "lightflow.external_image",
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow!()
        .name("External Image")
        .input("prompt", "text")
        .output("image", "artifact")
        .output("image_path", "path")
        .runtime("image_runtime", "lightflow.image.generate")
        .build()
}
"#,
    )?;

    let output = Command::new(env!("CARGO_BIN_EXE_lfw"))
        .args(["run", "lightflow.external_image", "-i", "prompt=cat"])
        .current_dir(&root)
        .env("HOME", &root)
        .env("SHELL", "/bin/zsh")
        .env("XDG_CONFIG_HOME", root.join(".test-xdg/config"))
        .env("XDG_DATA_HOME", root.join(".test-xdg/data"))
        .env_remove("LFW_PATH")
        .output()?;
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("has no executor"));
    assert!(stderr.contains("lightflow.image.generate"));

    let _ = fs::remove_dir_all(root);
    Ok(())
}
