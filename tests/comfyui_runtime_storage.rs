mod cli_project_support;
mod comfyui_runtime_support;
mod support;

use std::fs;
use std::path::Path;

use cli_project_support::use_local_lightflow_dependency;
use comfyui_runtime_support::{MockComfyUi, MockResponse};
use serde_json::{Value, json};
use support::{lfw, lfw_command, unique_temp_root};

#[test]
fn repeated_remote_target_never_overwrites_existing_artifact()
-> Result<(), Box<dyn std::error::Error>> {
    let root = generated_comfy_project()?;
    let server = MockComfyUi::start(
        [
            b"original bytes".as_slice(),
            b"replacement bytes".as_slice(),
        ]
        .into_iter()
        .flat_map(|bytes| completed_download_cycle("same-prompt", "same.png", bytes))
        .collect(),
    )?;
    let inputs = inputs(&server.url, "safe-output");

    let first = run_success(&root, "first.json", &inputs)?;
    let path = first["artifacts"][0]["path"].as_str().expect("path");
    assert!(
        Path::new(path)
            .file_name()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.starts_with("same-prompt_"))
    );
    assert_eq!(fs::read(path)?, b"original bytes");

    let error = run_failure(&root, "second.json", &inputs)?;
    assert!(
        error.contains("refusing to overwrite existing artifact"),
        "{error}"
    );
    assert_eq!(fs::read(path)?, b"original bytes");
    assert_eq!(server.finish().len(), 6);
    fs::remove_dir_all(root)?;
    Ok(())
}

#[cfg(unix)]
#[test]
fn final_and_legacy_partial_symlinks_are_never_followed_or_replaced()
-> Result<(), Box<dyn std::error::Error>> {
    let root = generated_comfy_project()?;
    let output_dir = root.join("safe-output");
    fs::create_dir_all(&output_dir)?;
    let outside_final = root.with_extension("outside-final");
    let outside_partial = root.with_extension("outside-partial");
    fs::write(&outside_final, b"outside final")?;
    fs::write(&outside_partial, b"outside partial")?;
    let target = output_dir.join("symlink-prompt_0000_9_images_0__outside.png");
    let legacy_partial = target.with_extension("png.part");
    std::os::unix::fs::symlink(&outside_final, &target)?;
    std::os::unix::fs::symlink(&outside_partial, &legacy_partial)?;
    let server = MockComfyUi::start(completed_download_cycle(
        "symlink-prompt",
        "outside.png",
        b"attacker bytes",
    ))?;
    let error = run_failure(&root, "symlink.json", &inputs(&server.url, "safe-output"))?;
    assert!(
        error.contains("refusing to overwrite existing artifact"),
        "{error}"
    );
    assert_eq!(fs::read(&outside_final)?, b"outside final");
    assert_eq!(fs::read(&outside_partial)?, b"outside partial");
    assert!(target.symlink_metadata()?.file_type().is_symlink());
    assert!(legacy_partial.symlink_metadata()?.file_type().is_symlink());
    assert_eq!(server.finish().len(), 3);
    let _ = fs::remove_file(outside_final);
    let _ = fs::remove_file(outside_partial);
    fs::remove_dir_all(root)?;
    Ok(())
}

fn completed_download_cycle(prompt_id: &str, filename: &str, bytes: &[u8]) -> Vec<MockResponse> {
    let mut history = serde_json::Map::new();
    history.insert(
        prompt_id.to_owned(),
        json!({
            "status":{"completed":true,"status_str":"success"},
            "outputs":{"9":{"images":[{"filename":filename,"subfolder":"","type":"output"}]}}
        }),
    );
    vec![
        MockResponse::json(json!({"prompt_id":prompt_id})),
        MockResponse::json(Value::Object(history)),
        MockResponse::bytes("image/png", bytes),
    ]
}

fn inputs(server_url: &str, output_dir: &str) -> Value {
    json!({
        "workflow":{"1":{"class_type":"Node","inputs":{}}},
        "server_url":server_url,
        "output_dir":output_dir,
        "poll_interval_ms":1
    })
}

fn generated_comfy_project() -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    fs::create_dir_all(&root)?;
    lfw(&root, ["init"])?;
    use_local_lightflow_dependency(&root)?;
    lfw(
        &root,
        [
            "new",
            "comfy_run",
            "--runtime",
            "lightflow.comfyui.workflow",
        ],
    )?;
    Ok(root)
}

fn run_success(
    root: &Path,
    name: &str,
    inputs: &Value,
) -> Result<Value, Box<dyn std::error::Error>> {
    let output = run(root, name, inputs)?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned().into());
    }
    Ok(serde_json::from_slice(&output.stdout)?)
}

fn run_failure(
    root: &Path,
    name: &str,
    inputs: &Value,
) -> Result<String, Box<dyn std::error::Error>> {
    let output = run(root, name, inputs)?;
    assert!(!output.status.success(), "run unexpectedly succeeded");
    Ok(String::from_utf8_lossy(&output.stderr).into_owned())
}

fn run(root: &Path, name: &str, inputs: &Value) -> std::io::Result<std::process::Output> {
    fs::write(root.join(name), serde_json::to_vec(inputs)?)?;
    lfw_command(root)
        .args([
            "run",
            "lightflow.comfy_run",
            "--inputs",
            &format!("@{name}"),
        ])
        .output()
}
