mod support;

use lightflow::api::ApiService;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;
use support::*;

#[test]
fn repository_text_to_image_declares_runtime_and_gguf_model()
-> Result<(), Box<dyn std::error::Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let service = ApiService::new(root);
    let workflow = service.get_workflow("lightflow.text_to_image")?;

    assert_eq!(workflow.category.as_deref(), Some("std"));
    assert_eq!(workflow.runtimes[0].capability, "lightflow.image.generate");
    assert_eq!(
        workflow.runtimes[0].engine.as_deref(),
        Some("builtin.preview.v1")
    );
    assert_eq!(workflow.models[0].capability, "text-to-image");
    assert_eq!(workflow.models[0].variants[0].format, "gguf");
    let prompt = workflow
        .inputs
        .iter()
        .find(|port| port.name == "prompt")
        .expect("prompt input exists");
    assert_eq!(prompt.required, Some(true));
    assert_eq!(prompt.widget.as_deref(), Some("prompt"));
    let width = workflow
        .inputs
        .iter()
        .find(|port| port.name == "width")
        .expect("width input exists");
    assert_eq!(width.default, Some(serde_json::json!(512)));
    assert_eq!(width.min, Some(64.0));
    assert_eq!(width.max, Some(2048.0));
    assert_eq!(width.step, Some(8.0));
    let model = workflow
        .inputs
        .iter()
        .find(|port| port.name == "model")
        .expect("model input exists");
    assert_eq!(model.widget.as_deref(), Some("model_select"));
    assert_eq!(model.model_requirement.as_deref(), Some("image_model"));
    assert_eq!(
        model.enum_values,
        vec![
            serde_json::json!("sdxl-gguf-q4"),
            serde_json::json!("sdxl-safetensors")
        ]
    );
    let image = workflow
        .outputs
        .iter()
        .find(|port| port.name == "image")
        .expect("image output exists");
    assert_eq!(image.artifact_kind.as_deref(), Some("image"));

    let help = lfw(root, ["help", "lightflow.text_to_image"])?;
    assert_eq!(help["ports"]["inputs"][0]["name"], "prompt");
    assert_eq!(help["ports"]["inputs"][0]["required"], true);
    assert_eq!(help["ports"]["inputs"][0]["widget"], "prompt");
    assert_eq!(help["ports"]["inputs"][2]["default"], 512);
    assert_eq!(help["ports"]["inputs"][2]["min"], 64.0);
    assert_eq!(
        help["ports"]["inputs"][6]["model_requirement"],
        "image_model"
    );
    assert_eq!(
        help["usage"]["inputs_json_shape"]["width"],
        serde_json::json!(512)
    );

    Ok(())
}

#[test]
fn lfx_runs_text_to_image_and_writes_png_artifact() -> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    fs::create_dir_all(&root)?;
    let output_path = root.join("out/image.png");

    let execution = lfx(
        Path::new(env!("CARGO_MANIFEST_DIR")),
        [
            "lightflow.text_to_image",
            "--prompt",
            "a quiet lake",
            "--input",
            "width=96",
            "--input",
            "height=64",
            "--output",
            output_path.to_str().unwrap(),
        ],
    )?;

    assert_eq!(execution["workflow_id"], "lightflow.text_to_image");
    assert_eq!(execution["runtime"]["executor_id"], "builtin.preview.v1");
    assert_eq!(execution["runtime"]["data_policy"], "artifact_handles");
    assert_eq!(
        execution["runtime"]["declared"][0]["capability"],
        "lightflow.image.generate"
    );
    assert_eq!(
        execution["outputs"]["image_path"],
        output_path.to_str().unwrap()
    );
    assert_eq!(execution["artifacts"][0]["kind"], "image");
    assert_eq!(execution["artifacts"][0]["mime_type"], "image/png");
    assert_eq!(
        execution["artifacts"][0]["metadata"]["capability"],
        "lightflow.image.generate"
    );
    assert_eq!(
        execution["artifacts"][0]["metadata"]["model"]["format"],
        "gguf"
    );

    let bytes = fs::read(&output_path)?;
    assert!(bytes.starts_with(b"\x89PNG\r\n\x1a\n"));

    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn text_to_image_defaults_output_to_xdg_pictures_lightflow()
-> Result<(), Box<dyn std::error::Error>> {
    let home = unique_temp_root();
    fs::create_dir_all(home.join(".config"))?;
    fs::write(
        home.join(".config/user-dirs.dirs"),
        r#"XDG_PICTURES_DIR="$HOME/Images"
"#,
    )?;

    let output = Command::new(env!("CARGO_BIN_EXE_lfx"))
        .args([
            "lightflow.text_to_image",
            "--prompt",
            "a quiet lake",
            "--input",
            "width=96",
            "--input",
            "height=64",
            "--input",
            "seed=7",
        ])
        .current_dir(Path::new(env!("CARGO_MANIFEST_DIR")))
        .env("HOME", &home)
        .env("SHELL", "/bin/zsh")
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("XDG_DATA_HOME", home.join(".local/share"))
        .env_remove("LFW_PATH")
        .output()?;
    assert!(
        output.status.success(),
        "lfx failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let execution: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    let expected_path = home
        .join("Images/lightflow/lightflow_text_to_image/7.png")
        .display()
        .to_string();
    assert_eq!(execution["outputs"]["image_path"], expected_path);
    let bytes = fs::read(home.join("Images/lightflow/lightflow_text_to_image/7.png"))?;
    assert!(bytes.starts_with(b"\x89PNG\r\n\x1a\n"));

    let _ = fs::remove_dir_all(home);
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

#[test]
fn flux_text_to_image_uses_external_runner_contract() -> Result<(), Box<dyn std::error::Error>> {
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
        "lightflow.test_flux",
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow("lightflow.test_flux")
        .version("0.1.0")
        .name("Test FLUX")
        .input("prompt", "text")
        .input("negative", "text")
        .input("width", "integer")
        .input("height", "integer")
        .input("seed", "integer")
        .input("steps", "integer")
        .input("guidance", "number")
        .input("output_path", "path")
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

    let flux_model = root.join("models/flux.gguf");
    let llm_model = root.join("models/llm.gguf");
    let vae_model = root.join("models/vae.safetensors");
    fs::write(&flux_model, b"flux")?;
    fs::write(&llm_model, b"llm")?;
    fs::write(&vae_model, b"vae")?;
    fs::write(
        root.join("lfw.lock"),
        serde_json::json!({
            "version": 1,
            "models": {
                "lightflow.test_flux::flux_model": { "local_paths": [flux_model] },
                "lightflow.test_flux::llm_model": { "local_paths": [llm_model] },
                "lightflow.test_flux::vae_model": { "local_paths": [vae_model] }
            }
        })
        .to_string(),
    )?;

    let fixture = root.join("runner-source.png");
    fs::write(
        &fixture,
        [
            0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48,
            0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x04, 0x00, 0x00,
            0x00, 0xb5, 0x1c, 0x0c, 0x02, 0x00, 0x00, 0x00, 0x0b, 0x49, 0x44, 0x41, 0x54, 0x78,
            0xda, 0x63, 0xfc, 0xff, 0x1f, 0x00, 0x03, 0x03, 0x02, 0x00, 0xef, 0xbf, 0xa7, 0xdb,
            0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
        ],
    )?;
    let runner_log = root.join("runner-args.txt");
    let runner = root.join("flux-runner.sh");
    fs::write(
        &runner,
        format!(
            r#"#!/bin/sh
set -eu
out=""
while [ "$#" -gt 0 ]; do
  printf '%s\n' "$1" >> {log:?}
  case "$1" in
    --output)
      shift
      out="$1"
      printf '%s\n' "$1" >> {log:?}
      ;;
    *)
      ;;
  esac
  shift || true
done
test -n "$out"
mkdir -p "$(dirname "$out")"
cp {fixture:?} "$out"
"#,
            log = runner_log,
            fixture = fixture,
        ),
    )?;
    let mut permissions = fs::metadata(&runner)?.permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&runner, permissions)?;

    let output_path = root.join("out/flux.png");
    let execution = lfw_with_env_values(
        &root,
        [
            "run",
            "lightflow.test_flux",
            "--prompt",
            "a red cabin",
            "-i",
            "negative=\"blur\"",
            "-i",
            "width=128",
            "-i",
            "height=96",
            "-i",
            "seed=77",
            "-i",
            "steps=2",
            "-i",
            "guidance=3.25",
            "--output",
            output_path.to_str().unwrap(),
        ],
        [
            ("LIGHTFLOW_FLUX_BACKEND", "external"),
            ("LIGHTFLOW_FLUX_RUNNER", runner.to_str().unwrap()),
        ],
    )?;

    assert_eq!(execution["workflow_id"], "lightflow.test_flux");
    assert_eq!(
        execution["outputs"]["image_path"],
        output_path.to_str().unwrap()
    );
    assert_eq!(
        execution["artifacts"][0]["metadata"]["engine"],
        "flux2-klein.gguf.runner.v1"
    );
    assert!(fs::read(&output_path)?.starts_with(b"\x89PNG\r\n\x1a\n"));
    let runner_args = fs::read_to_string(&runner_log)?;
    assert!(runner_args.contains("--task\ntext-to-image\n"));
    assert!(runner_args.contains("--prompt\na red cabin\n"));
    assert!(runner_args.contains("--width\n128\n"));
    assert!(runner_args.contains("--height\n96\n"));
    assert!(runner_args.contains("--seed\n77\n"));
    assert!(runner_args.contains("--steps\n2\n"));
    assert!(runner_args.contains("--guidance\n3.25\n"));
    assert!(runner_args.contains("--flux-model\n"));
    assert!(runner_args.contains("models/flux.gguf"));
    assert!(runner_args.contains("--llm-model\n"));
    assert!(runner_args.contains("models/llm.gguf"));
    assert!(runner_args.contains("--vae-model\n"));
    assert!(runner_args.contains("models/vae.safetensors"));

    let templated = lfw_with_env_values(
        &root,
        [
            "run",
            "lightflow.test_flux",
            "--prompt",
            "five cats",
            "-i",
            "seed=80",
            "-i",
            "count=3",
            "-i",
            "output_template=\"out/cat-{index:03}-{seed}.png\"",
        ],
        [
            ("LIGHTFLOW_FLUX_BACKEND", "external"),
            ("LIGHTFLOW_FLUX_RUNNER", runner.to_str().unwrap()),
        ],
    )?;
    assert_eq!(templated["artifacts"].as_array().unwrap().len(), 3);
    assert_eq!(
        templated["outputs"]["image_paths"],
        serde_json::json!([
            "out/cat-001-80.png",
            "out/cat-002-81.png",
            "out/cat-003-82.png"
        ])
    );
    assert_eq!(templated["artifacts"][2]["metadata"]["index"], 3);
    assert_eq!(templated["artifacts"][2]["metadata"]["count"], 3);
    for path in [
        "out/cat-001-80.png",
        "out/cat-002-81.png",
        "out/cat-003-82.png",
    ] {
        assert!(fs::read(root.join(path))?.starts_with(b"\x89PNG\r\n\x1a\n"));
    }
    let runner_args = fs::read_to_string(&runner_log)?;
    assert!(runner_args.contains("--seed\n80\n"));
    assert!(runner_args.contains("--seed\n81\n"));
    assert!(runner_args.contains("--seed\n82\n"));

    fs::create_dir_all(root.join(".test-xdg/config"))?;
    fs::write(
        root.join(".test-xdg/config/user-dirs.dirs"),
        r#"XDG_PICTURES_DIR="$HOME/Images"
"#,
    )?;
    let xdg_default = lfw_with_env_values(
        &root,
        [
            "run",
            "lightflow.test_flux",
            "--prompt",
            "two cats",
            "-i",
            "seed=90",
            "-i",
            "count=2",
        ],
        [
            ("LIGHTFLOW_FLUX_BACKEND", "external"),
            ("LIGHTFLOW_FLUX_RUNNER", runner.to_str().unwrap()),
        ],
    )?;
    let expected_default_paths = [
        root.join("Images/lightflow/lightflow_test_flux/90-001.png"),
        root.join("Images/lightflow/lightflow_test_flux/91-002.png"),
    ];
    assert_eq!(
        xdg_default["outputs"]["image_paths"],
        serde_json::Value::Array(
            expected_default_paths
                .iter()
                .map(|path| path.display().to_string().into())
                .collect()
        )
    );
    for path in expected_default_paths {
        assert!(fs::read(path)?.starts_with(b"\x89PNG\r\n\x1a\n"));
    }

    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn flux_edit_and_inpaint_use_external_runner_contracts() -> Result<(), Box<dyn std::error::Error>> {
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
        "lightflow.test_flux_edit",
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow("lightflow.test_flux_edit")
        .version("0.1.0")
        .name("Test FLUX Edit")
        .input("image_path", "path")
        .input("prompt", "text")
        .input("negative", "text")
        .input("strength", "number")
        .input("seed", "integer")
        .input("steps", "integer")
        .input("guidance", "number")
        .input("output_path", "path")
        .output("image", "artifact")
        .output("image_path", "path")
        .runtime("flux_runtime", "lightflow.image.edit")
        .hf_model("flux_model", "flux-test", "image-edit", "gguf", "local/flux", "flux.gguf")
        .hf_model("llm_model", "llm-test", "language-model", "gguf", "local/llm", "llm.gguf")
        .hf_model("vae_model", "vae-test", "vae", "safetensors", "local/vae", "vae.safetensors")
        .build()
}
"#,
    )?;
    write_workflow_crate(
        &root,
        "lightflow.test_flux_inpaint",
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow("lightflow.test_flux_inpaint")
        .version("0.1.0")
        .name("Test FLUX Inpaint")
        .input("image_path", "path")
        .input("mask_path", "path")
        .input("prompt", "text")
        .input("negative", "text")
        .input("strength", "number")
        .input("seed", "integer")
        .input("steps", "integer")
        .input("guidance", "number")
        .input("output_path", "path")
        .output("image", "artifact")
        .output("image_path", "path")
        .runtime("flux_runtime", "lightflow.image.inpaint")
        .hf_model("flux_model", "flux-test", "image-inpaint", "gguf", "local/flux", "flux.gguf")
        .hf_model("llm_model", "llm-test", "language-model", "gguf", "local/llm", "llm.gguf")
        .hf_model("vae_model", "vae-test", "vae", "safetensors", "local/vae", "vae.safetensors")
        .build()
}
"#,
    )?;

    let flux_model = root.join("models/flux.gguf");
    let llm_model = root.join("models/llm.gguf");
    let vae_model = root.join("models/vae.safetensors");
    fs::write(&flux_model, b"flux")?;
    fs::write(&llm_model, b"llm")?;
    fs::write(&vae_model, b"vae")?;
    fs::write(
        root.join("lfw.lock"),
        serde_json::json!({
            "version": 1,
            "models": {
                "lightflow.test_flux_edit::flux_model": { "local_paths": [flux_model] },
                "lightflow.test_flux_edit::llm_model": { "local_paths": [llm_model] },
                "lightflow.test_flux_edit::vae_model": { "local_paths": [vae_model] },
                "lightflow.test_flux_inpaint::flux_model": { "local_paths": [flux_model] },
                "lightflow.test_flux_inpaint::llm_model": { "local_paths": [llm_model] },
                "lightflow.test_flux_inpaint::vae_model": { "local_paths": [vae_model] }
            }
        })
        .to_string(),
    )?;

    let fixture = root.join("runner-source.png");
    let input = root.join("input.png");
    let mask = root.join("mask.png");
    let png = [
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x04, 0x00, 0x00, 0x00, 0xb5,
        0x1c, 0x0c, 0x02, 0x00, 0x00, 0x00, 0x0b, 0x49, 0x44, 0x41, 0x54, 0x78, 0xda, 0x63, 0xfc,
        0xff, 0x1f, 0x00, 0x03, 0x03, 0x02, 0x00, 0xef, 0xbf, 0xa7, 0xdb, 0x00, 0x00, 0x00, 0x00,
        0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
    ];
    fs::write(&fixture, png)?;
    fs::write(&input, png)?;
    fs::write(&mask, png)?;

    let runner_log = root.join("runner-args.txt");
    let runner = root.join("flux-runner.sh");
    fs::write(
        &runner,
        format!(
            r#"#!/bin/sh
set -eu
out=""
while [ "$#" -gt 0 ]; do
  printf '%s\n' "$1" >> {log:?}
  case "$1" in
    --output)
      shift
      out="$1"
      printf '%s\n' "$1" >> {log:?}
      ;;
    *)
      ;;
  esac
  shift || true
done
test -n "$out"
mkdir -p "$(dirname "$out")"
cp {fixture:?} "$out"
"#,
            log = runner_log,
            fixture = fixture,
        ),
    )?;
    let mut permissions = fs::metadata(&runner)?.permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&runner, permissions)?;

    let edit_output = root.join("out/edit.png");
    let edit = lfw_with_env_values(
        &root,
        [
            "run",
            "lightflow.test_flux_edit",
            "-i",
            &format!("image_path={:?}", input.display().to_string()),
            "--prompt",
            "make it dusk",
            "-i",
            "strength=0.55",
            "--output",
            edit_output.to_str().unwrap(),
        ],
        [
            ("LIGHTFLOW_FLUX_BACKEND", "external"),
            ("LIGHTFLOW_FLUX_RUNNER", runner.to_str().unwrap()),
        ],
    )?;
    assert_eq!(
        edit["artifacts"][0]["metadata"]["capability"],
        "lightflow.image.edit"
    );
    assert_eq!(edit["artifacts"][0]["metadata"]["task"], "image-edit");

    let inpaint_output = root.join("out/inpaint.png");
    let inpaint = lfw_with_env_values(
        &root,
        [
            "run",
            "lightflow.test_flux_inpaint",
            "-i",
            &format!("image_path={:?}", input.display().to_string()),
            "-i",
            &format!("mask_path={:?}", mask.display().to_string()),
            "--prompt",
            "repair the center",
            "-i",
            "strength=0.8",
            "--output",
            inpaint_output.to_str().unwrap(),
        ],
        [
            ("LIGHTFLOW_FLUX_BACKEND", "external"),
            ("LIGHTFLOW_FLUX_RUNNER", runner.to_str().unwrap()),
        ],
    )?;
    assert_eq!(
        inpaint["artifacts"][0]["metadata"]["capability"],
        "lightflow.image.inpaint"
    );
    assert_eq!(inpaint["artifacts"][0]["metadata"]["task"], "inpaint");
    assert!(fs::read(&edit_output)?.starts_with(b"\x89PNG\r\n\x1a\n"));
    assert!(fs::read(&inpaint_output)?.starts_with(b"\x89PNG\r\n\x1a\n"));

    let runner_args = fs::read_to_string(&runner_log)?;
    assert!(runner_args.contains("--task\nimage-edit\n"));
    assert!(runner_args.contains("--image\n"));
    assert!(runner_args.contains("input.png"));
    assert!(runner_args.contains("--strength\n0.55\n"));
    assert!(runner_args.contains("--task\ninpaint\n"));
    assert!(runner_args.contains("--mask\n"));
    assert!(runner_args.contains("mask.png"));
    assert!(runner_args.contains("--strength\n0.8\n"));

    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn lfw_runs_text_to_image_through_invert_pipeline() -> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    fs::create_dir_all(&root)?;
    let generated_path = root.join("out/cat.png");
    let inverted_path = root.join("out/cat-inverted.png");

    let execution = lfw(
        Path::new(env!("CARGO_MANIFEST_DIR")),
        [
            "run",
            "lightflow.text_to_image",
            "--prompt",
            "a small cat photo",
            "--input",
            "width=64",
            "--input",
            "height=64",
            "--output",
            generated_path.to_str().unwrap(),
            "|",
            "lightflow.image.invert",
            "--output",
            inverted_path.to_str().unwrap(),
        ],
    )?;

    assert_eq!(execution["pipeline"], true);
    assert_eq!(
        execution["outputs"]["image_path"],
        inverted_path.to_str().unwrap()
    );
    assert_eq!(
        execution["stages"][1]["artifacts"][0]["metadata"]["capability"],
        "lightflow.image.invert"
    );
    let generated = fs::read(&generated_path)?;
    let inverted = fs::read(&inverted_path)?;
    assert!(generated.starts_with(b"\x89PNG\r\n\x1a\n"));
    assert!(inverted.starts_with(b"\x89PNG\r\n\x1a\n"));
    assert_ne!(generated, inverted);

    let _ = fs::remove_dir_all(root);
    Ok(())
}
