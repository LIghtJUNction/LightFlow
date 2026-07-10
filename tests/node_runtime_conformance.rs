mod support;

use std::fs;
use std::path::Path;
use std::process::Output;
use support::*;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

const EXTERNAL_ENGINE: &str = "flux2-klein.gguf.runner.v1";
const NATIVE_ENGINE: &str = "diffusion-rs.native.v1";

#[cfg(not(feature = "flux-native"))]
#[test]
fn flux_node_test_reports_default_external_backend_unavailable()
-> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    fs::create_dir_all(&root)?;
    write_workflow_crate(
        &root,
        "lightflow.flux_default",
        &flux_source("lightflow.flux_default", None),
    )?;
    write_skill(&root, "lightflow.flux_default")?;

    let output = lfw_command(&root)
        .args(["node", "test", "lightflow.flux_default"])
        .env_remove("LIGHTFLOW_FLUX_BACKEND")
        .env_remove("LIGHTFLOW_FLUX_RUNNER")
        .output()?;
    assert!(!output.status.success());
    let check = runtime_check(&output)?;
    assert_eq!(check["status"], "failed");
    let message = check["message"].as_str().expect("runtime message");
    assert!(message.contains(EXTERNAL_ENGINE), "message: {message}");
    assert!(
        message.contains("set LIGHTFLOW_FLUX_RUNNER"),
        "message: {message}"
    );

    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn explicit_external_flux_backend_is_available_when_runner_is_set()
-> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    fs::create_dir_all(&root)?;
    write_workflow_crate(
        &root,
        "lightflow.flux_external",
        &flux_source("lightflow.flux_external", Some(EXTERNAL_ENGINE)),
    )?;
    write_skill(&root, "lightflow.flux_external")?;

    let output = lfw_command(&root)
        .args(["node", "test", "lightflow.flux_external"])
        .env_remove("LIGHTFLOW_FLUX_BACKEND")
        .env("LIGHTFLOW_FLUX_RUNNER", "/bin/true")
        .output()?;
    assert!(
        output.status.success(),
        "stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(runtime_check(&output)?["status"], "passed");

    let plan_output = lfw_command(&root)
        .args(["plan", "lightflow.flux_external"])
        .env_remove("LIGHTFLOW_FLUX_BACKEND")
        .env("LIGHTFLOW_FLUX_RUNNER", "/bin/true")
        .output()?;
    assert!(plan_output.status.success());
    let plan: serde_json::Value = serde_json::from_slice(&plan_output.stdout)?;
    assert_eq!(plan["runtime"]["executor_id"], EXTERNAL_ENGINE);
    assert_eq!(plan["runtime"]["executor_available"], true);

    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[cfg(unix)]
#[test]
fn external_flux_runner_requires_an_executable_regular_file()
-> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    fs::create_dir_all(&root)?;
    write_workflow_crate(
        &root,
        "lightflow.flux_runner_validation",
        &flux_source("lightflow.flux_runner_validation", Some(EXTERNAL_ENGINE)),
    )?;
    write_skill(&root, "lightflow.flux_runner_validation")?;

    let missing = root.join("missing-runner");
    let directory = root.join("runner-directory");
    let non_executable = root.join("runner-file");
    fs::create_dir_all(&directory)?;
    fs::write(&non_executable, "#!/bin/sh\nexit 0\n")?;
    fs::set_permissions(&non_executable, fs::Permissions::from_mode(0o644))?;

    assert_runner_unavailable(&root, "", "LIGHTFLOW_FLUX_RUNNER is empty")?;
    assert_runner_unavailable(
        &root,
        missing.to_str().expect("missing path"),
        "does not point to a file",
    )?;
    assert_runner_unavailable(
        &root,
        directory.to_str().expect("directory path"),
        "does not point to a file",
    )?;
    assert_runner_unavailable(
        &root,
        non_executable.to_str().expect("runner path"),
        "is not executable",
    )?;

    let _ = fs::remove_dir_all(root);
    Ok(())
}

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
fn explicit_non_flux_engine_must_exist_and_match_capability()
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

    let accepted = lfw_command(&root)
        .args(["node", "test", "lightflow.load_builtin"])
        .output()?;
    assert!(
        accepted.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&accepted.stderr)
    );
    assert_eq!(runtime_check(&accepted)?["status"], "passed");

    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[cfg(not(feature = "flux-native"))]
#[test]
fn explicit_native_flux_backend_is_unavailable_without_native_feature()
-> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    fs::create_dir_all(&root)?;
    write_workflow_crate(
        &root,
        "lightflow.flux_native",
        &flux_source("lightflow.flux_native", Some(NATIVE_ENGINE)),
    )?;
    write_skill(&root, "lightflow.flux_native")?;

    let output = lfw_command(&root)
        .args(["node", "test", "lightflow.flux_native"])
        .env_remove("LIGHTFLOW_FLUX_BACKEND")
        .env_remove("LIGHTFLOW_FLUX_RUNNER")
        .output()?;
    assert!(!output.status.success());
    let check = runtime_check(&output)?;
    assert_eq!(check["status"], "failed");
    let message = check["message"].as_str().expect("runtime message");
    assert!(message.contains(NATIVE_ENGINE), "message: {message}");
    assert!(
        message.contains("build with --features flux-native"),
        "message: {message}"
    );

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

fn assert_runner_unavailable(
    root: &Path,
    runner: &str,
    expected_reason: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let plan_output = lfw_command(root)
        .args(["plan", "lightflow.flux_runner_validation"])
        .env_remove("LIGHTFLOW_FLUX_BACKEND")
        .env("LIGHTFLOW_FLUX_RUNNER", runner)
        .output()?;
    assert!(plan_output.status.success());
    let plan: serde_json::Value = serde_json::from_slice(&plan_output.stdout)?;
    assert_eq!(plan["runtime"]["executor_available"], false);
    assert!(
        plan["runtime"]["executor_status_reason"]
            .as_str()
            .expect("status reason")
            .contains(expected_reason),
        "plan: {plan}"
    );

    let node_test = lfw_command(root)
        .args(["node", "test", "lightflow.flux_runner_validation"])
        .env_remove("LIGHTFLOW_FLUX_BACKEND")
        .env("LIGHTFLOW_FLUX_RUNNER", runner)
        .output()?;
    assert!(!node_test.status.success());
    assert!(
        runtime_check(&node_test)?["message"]
            .as_str()
            .expect("runtime message")
            .contains(expected_reason)
    );
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
