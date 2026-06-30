mod support;

use std::fs;
use std::process::Command;
use support::*;

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
    workflow("lightflow.external_image")
        .version("0.1.0")
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

#[test]
fn flux_runtime_rejects_missing_locked_model_before_runner()
-> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    fs::create_dir_all(root.join("models"))?;
    fs::write(
        root.join("Cargo.toml"),
        r#"[workspace]
resolver = "3"
members = ["workflows/*/*"]

[workspace.dependencies]
lightflow = { path = "." }
"#,
    )?;
    write_workflow_crate(
        &root,
        "lightflow.test_flux_missing_lock",
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow("lightflow.test_flux_missing_lock")
        .version("0.1.0")
        .name("Test FLUX Missing Lock")
        .input("prompt", "text")
        .output("image", "artifact")
        .output("image_path", "path")
        .runtime("flux_runtime", "lightflow.image.generate")
        .hf_model("flux_model", "flux-test", "text-to-image", "gguf", "local/flux", "flux.gguf")
        .hf_model("llm_model", "llm-test", "language-model", "gguf", "local/llm", "llm.gguf")
        .hf_model("vae_model", "vae-test", "vae", "safetensors", "local/vae", "vae.safetensors")
        .build()
}
"#,
    )?;

    let missing_flux = root.join("models/missing-flux.gguf");
    let llm_model = root.join("models/llm.gguf");
    let vae_model = root.join("models/vae.safetensors");
    fs::write(&llm_model, b"llm")?;
    fs::write(&vae_model, b"vae")?;
    fs::write(
        root.join("lfw.lock"),
        serde_json::json!({
            "version": 1,
            "models": {
                "lightflow.test_flux_missing_lock::flux_model": {
                    "format": "gguf",
                    "local_paths": [missing_flux]
                },
                "lightflow.test_flux_missing_lock::llm_model": {
                    "format": "gguf",
                    "local_paths": [llm_model]
                },
                "lightflow.test_flux_missing_lock::vae_model": {
                    "format": "safetensors",
                    "local_paths": [vae_model]
                }
            }
        })
        .to_string(),
    )?;

    let output = lfw_command(&root)
        .args(["run", "lightflow.test_flux_missing_lock", "--prompt", "cat"])
        .env("LIGHTFLOW_FLUX_BACKEND", "external")
        .env("LIGHTFLOW_FLUX_RUNNER", "/bin/false")
        .output()?;
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("model file for lightflow.test_flux_missing_lock::flux_model is missing"),
        "stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("lfw sync lightflow.test_flux_missing_lock --locked --apply"),
        "stderr:\n{stderr}"
    );

    let _ = fs::remove_dir_all(root);
    Ok(())
}
