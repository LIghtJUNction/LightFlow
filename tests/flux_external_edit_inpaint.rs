mod support;

use std::fs;
use std::os::unix::fs::PermissionsExt;
use support::*;

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
    workflow!()
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
    workflow!()
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
    fs::write(&fixture, PNG_FIXTURE)?;
    fs::write(&input, PNG_FIXTURE)?;
    fs::write(&mask, PNG_FIXTURE)?;

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

const PNG_FIXTURE: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x04, 0x00, 0x00, 0x00, 0xb5, 0x1c, 0x0c,
    0x02, 0x00, 0x00, 0x00, 0x0b, 0x49, 0x44, 0x41, 0x54, 0x78, 0xda, 0x63, 0xfc, 0xff, 0x1f, 0x00,
    0x03, 0x03, 0x02, 0x00, 0xef, 0xbf, 0xa7, 0xdb, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44,
    0xae, 0x42, 0x60, 0x82,
];
