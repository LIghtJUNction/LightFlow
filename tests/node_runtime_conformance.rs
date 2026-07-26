mod support;

use std::fs;
use std::path::Path;
use std::process::Output;
use support::*;

#[test]
fn unsupported_explicit_flux_engine_is_a_plan_error() -> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    fs::create_dir_all(&root)?;
    write_workflow_crate(
        &root,
        "lightflow.flux_invalid_engine",
        &flux_source("lightflow.flux_invalid_engine", Some("builtin.llm.mock.v1")),
    )?;

    let output = lfw_command(&root)
        .args(["plan", "lightflow.flux_invalid_engine"])
        .env_remove("LIGHTFLOW_FLUX_BACKEND")
        .env_remove("LIGHTFLOW_FLUX_RUNNER")
        .output()?;
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("builtin.llm.mock.v1"), "stderr: {stderr}");

    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn explicit_non_flux_engine_must_exist_match_capability_and_report_availability()
-> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    fs::create_dir_all(&root)?;
    write_workflow_crate(
        &root,
        "lightflow.load_bogus",
        &image_load_source("lightflow.load_bogus", "bogus.image.engine"),
    )?;
    write_workflow_crate(
        &root,
        "lightflow.load_builtin",
        &image_load_source("lightflow.load_builtin", "builtin.image.load.v1"),
    )?;
    write_skill(&root, "lightflow.load_bogus")?;
    write_skill(&root, "lightflow.load_builtin")?;

    let plan = lfw_command(&root)
        .args(["plan", "lightflow.load_bogus"])
        .output()?;
    assert!(!plan.status.success());
    assert!(
        String::from_utf8_lossy(&plan.stderr).contains("bogus.image.engine"),
        "stderr: {}",
        String::from_utf8_lossy(&plan.stderr)
    );

    let rejected = lfw_command(&root)
        .args(["node", "test", "lightflow.load_bogus"])
        .output()?;
    assert!(!rejected.status.success());
    assert_eq!(runtime_check(&rejected)?["status"], "failed");

    let known = lfw(&root, ["plan", "lightflow.load_builtin"])?;
    assert_eq!(known["runtime"]["executor_id"], "builtin.image.load.v1");
    assert_eq!(known["runtime"]["executor_available"], false);
    assert_eq!(known["runtime"]["recipe"], "unavailable");

    let unavailable = lfw_command(&root)
        .args(["node", "test", "lightflow.load_builtin"])
        .output()?;
    assert!(
        !unavailable.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&unavailable.stderr)
    );
    assert_eq!(runtime_check(&unavailable)?["status"], "failed");
    assert!(String::from_utf8_lossy(&unavailable.stderr).contains("reserved executor contract"));

    let run = lfw_command(&root)
        .args([
            "run",
            "lightflow.load_builtin",
            "--input",
            "image_path=\"missing.png\"",
        ])
        .output()?;
    assert!(!run.status.success());
    assert!(String::from_utf8_lossy(&run.stderr).contains("reserved engine"));

    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn conditional_node_test_checks_every_candidate_runtime() -> Result<(), Box<dyn std::error::Error>>
{
    let root = unique_temp_root();
    fs::create_dir_all(&root)?;
    write_workflow_crate(
        &root,
        "lightflow.preview_a",
        &preview_source("lightflow.preview_a"),
    )?;
    write_workflow_crate(
        &root,
        "lightflow.preview_b",
        &preview_source("lightflow.preview_b"),
    )?;
    write_workflow_crate(
        &root,
        "lightflow.abstract_branch",
        &abstract_source("lightflow.abstract_branch"),
    )?;
    write_workflow_crate(
        &root,
        "lightflow.conditional_bad",
        &conditional_source(
            "lightflow.conditional_bad",
            "lightflow.preview_a",
            "lightflow.abstract_branch",
        ),
    )?;
    write_workflow_crate(
        &root,
        "lightflow.conditional_good",
        &conditional_source(
            "lightflow.conditional_good",
            "lightflow.preview_a",
            "lightflow.preview_b",
        ),
    )?;
    write_skill(&root, "lightflow.conditional_bad")?;
    write_skill(&root, "lightflow.conditional_good")?;

    let failed = lfw_command(&root)
        .args(["node", "test", "lightflow.conditional_bad"])
        .env_remove("LIGHTFLOW_FLUX_BACKEND")
        .env_remove("LIGHTFLOW_FLUX_RUNNER")
        .output()?;
    assert!(!failed.status.success());
    let failed_check = runtime_check(&failed)?;
    assert_eq!(failed_check["status"], "failed");
    assert!(
        failed_check["message"]
            .as_str()
            .expect("runtime message")
            .contains("lightflow.abstract_branch")
    );

    let passed = lfw_command(&root)
        .args(["node", "test", "lightflow.conditional_good"])
        .env_remove("LIGHTFLOW_FLUX_BACKEND")
        .env_remove("LIGHTFLOW_FLUX_RUNNER")
        .output()?;
    assert!(
        passed.status.success(),
        "stderr:\n{}",
        String::from_utf8_lossy(&passed.stderr)
    );
    let passed_check = runtime_check(&passed)?;
    assert_eq!(passed_check["status"], "passed");
    assert!(
        passed_check["message"]
            .as_str()
            .expect("runtime message")
            .contains("2 reachable leaf executor(s)")
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
        .join(".lightflow/workflows")
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

fn flux_source(_workflow_id: &str, engine: Option<&str>) -> String {
    let runtime = match engine {
        Some(engine) => format!(
            ".builtin_runtime(\"image_runtime\", \"lightflow.image.generate\", \"{engine}\")"
        ),
        None => ".runtime(\"image_runtime\", \"lightflow.image.generate\")".to_owned(),
    };
    format!(
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {{
    workflow!()
        .name("FLUX Test")
        .description("Tests physical FLUX backend planning.")
        .input("prompt", "text")
        .input_description("prompt", "Prompt to render.")
        .input_required("prompt", true)
        .output("image", "artifact")
        .output_description("image", "Generated image metadata.")
        .output_artifact_kind("image", "image")
        .output("image_path", "path")
        .output_description("image_path", "Generated image path.")
        {runtime}
        .hf_model("flux_model", "flux", "image-generation", "gguf", "owner/flux", "flux.gguf")
        .hf_model("llm_model", "llm", "text-encoder", "gguf", "owner/llm", "llm.gguf")
        .hf_model("vae_model", "vae", "vae", "safetensors", "owner/vae", "vae.safetensors")
        .build()
}}
"#
    )
}

fn preview_source(_workflow_id: &str) -> String {
    String::from(
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow!()
        .name("Preview Branch")
        .description("Runnable preview branch.")
        .input("flag", "boolean")
        .input_description("flag", "Conditional flag.")
        .output("image", "artifact")
        .output_description("image", "Preview image metadata.")
        .output_artifact_kind("image", "image")
        .builtin_runtime("image_runtime", "lightflow.image.generate", "builtin.preview.v1")
        .build()
}
"#,
    )
}

fn abstract_source(workflow_id: &str) -> String {
    preview_source(workflow_id).replace(
        ".builtin_runtime(\"image_runtime\", \"lightflow.image.generate\", \"builtin.preview.v1\")",
        ".runtime(\"image_runtime\", \"lightflow.image.generate\")",
    )
}

fn conditional_source(_workflow_id: &str, then_id: &str, else_id: &str) -> String {
    format!(
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {{
    workflow!()
        .name("Conditional Root")
        .description("Checks every conditional runtime candidate.")
        .input("flag", "boolean")
        .input_description("flag", "Selects a conditional branch.")
        .input_required("flag", true)
        .output("image", "artifact")
        .output_description("image", "Selected image metadata.")
        .output_artifact_kind("image", "image")
        .if_node("gate", "flag", true, "{then_id}", "{else_id}")
        .build()
}}
"#
    )
}

fn image_load_source(_workflow_id: &str, engine: &str) -> String {
    format!(
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {{
    workflow!()
        .name("Image Load")
        .description("Tests an explicit non-FLUX engine.")
        .input("image_path", "path")
        .input_description("image_path", "Image path to load.")
        .input_required("image_path", true)
        .output("image", "artifact")
        .output_description("image", "Loaded image metadata.")
        .output_artifact_kind("image", "image")
        .builtin_runtime("image_runtime", "lightflow.image.load", "{engine}")
        .build()
}}
"#
    )
}
