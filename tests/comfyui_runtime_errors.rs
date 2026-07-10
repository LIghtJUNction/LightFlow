mod comfyui_runtime_support;
mod support;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Output;
use std::time::{Duration, Instant};

use comfyui_runtime_support::{MockComfyUi, MockResponse};
use lightflow::api::ApiService;
use lightflow::workflow::WorkflowExecutionOptions;
use serde_json::{Value, json};
use support::{lfw, lfw_command, unique_temp_root};

#[test]
fn workflow_path_is_rejected_to_preserve_inline_replay_input()
-> Result<(), Box<dyn std::error::Error>> {
    let root = generated_comfy_project()?;
    let inputs = json!({
        "workflow": {"1":{"class_type":"Node","inputs":{}}},
        "workflow_path": "mutable-api.json"
    });
    let error = run_failure(&root, "workflow-path.json", &inputs, None)?;
    assert!(error.contains("workflow_path is not supported"));
    assert!(error.contains("inline ComfyUI API Format workflow"));
    fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn upload_paths_outside_project_fail_before_network() -> Result<(), Box<dyn std::error::Error>> {
    let root = generated_comfy_project()?;
    let outside = root.with_extension("outside.png");
    fs::write(&outside, b"outside")?;
    let server = MockComfyUi::start(vec![MockResponse::json(json!({"unexpected":true}))])?;
    let traversal = format!("../{}", outside.file_name().unwrap().to_string_lossy());
    for path in ["/etc/hosts", traversal.as_str()] {
        let mut inputs = base_inputs(&server.url);
        inputs["uploads"] = json!([{"path":path}]);
        let error = direct_error(&root, inputs)?;
        assert!(error.contains("safe project-relative path"), "{error}");
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&outside, root.join("escape.png"))?;
        let mut inputs = base_inputs(&server.url);
        inputs["uploads"] = json!([{"path":"escape.png"}]);
        let error = direct_error(&root, inputs)?;
        assert!(error.contains("escapes project root"), "{error}");
    }
    assert!(server.finish().is_empty());
    let _ = fs::remove_file(outside);
    fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn output_directories_outside_project_fail_before_network() -> Result<(), Box<dyn std::error::Error>>
{
    let root = generated_comfy_project()?;
    let outside = root.with_extension("outside-dir");
    fs::create_dir_all(&outside)?;
    let server = MockComfyUi::start(vec![MockResponse::json(json!({"unexpected":true}))])?;
    for path in [
        outside.to_string_lossy().into_owned(),
        "../outside-dir".to_owned(),
    ] {
        let mut inputs = base_inputs(&server.url);
        inputs["output_dir"] = path.into();
        let error = direct_error(&root, inputs)?;
        assert!(error.contains("safe project-relative path"), "{error}");
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&outside, root.join("output-link"))?;
        let mut inputs = base_inputs(&server.url);
        inputs["output_dir"] = "output-link/files".into();
        let error = direct_error(&root, inputs)?;
        assert!(error.contains("symlink"), "{error}");
    }
    assert!(server.finish().is_empty());
    let _ = fs::remove_dir_all(outside);
    fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn unknown_node_override_fails_before_network() -> Result<(), Box<dyn std::error::Error>> {
    let root = generated_comfy_project()?;
    let inputs = json!({
        "workflow": {"1":{"class_type":"Node","inputs":{}}},
        "node_inputs": {"missing":{"seed":1}}
    });
    let error = run_failure(&root, "unknown-override.json", &inputs, None)?;
    assert!(error.contains("node_inputs references unknown node or inputs container missing"));
    fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn unknown_upload_bind_node_fails_before_network() -> Result<(), Box<dyn std::error::Error>> {
    let root = generated_comfy_project()?;
    fs::write(root.join("input.png"), b"image")?;
    let inputs = json!({
        "workflow": {"1":{"class_type":"Node","inputs":{}}},
        "uploads": [{"path":"input.png","bind":[{"node_id":"missing","input":"image"}]}]
    });
    let error = run_failure(&root, "unknown-bind.json", &inputs, None)?;
    assert!(error.contains("upload bind references unknown node or inputs container missing"));
    fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn quoted_upload_names_fail_before_network() -> Result<(), Box<dyn std::error::Error>> {
    let root = generated_comfy_project()?;
    fs::write(root.join("quoted\"file.png"), b"image")?;
    fs::write(root.join("plain.png"), b"image")?;
    let server = MockComfyUi::start(vec![MockResponse::json(json!({"unexpected":true}))])?;
    for upload in [
        json!({"path":"quoted\"file.png"}),
        json!({"path":"plain.png","name":"quoted\"name.png"}),
    ] {
        let mut inputs = base_inputs(&server.url);
        inputs["uploads"] = Value::Array(vec![upload]);
        let error = direct_error(&root, inputs)?;
        assert!(
            error.contains("without slashes, quotes, or line breaks"),
            "{error}"
        );
    }
    assert!(server.finish().is_empty());
    fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn prompt_http_rejection_names_action_without_leaking_authorization()
-> Result<(), Box<dyn std::error::Error>> {
    let root = generated_comfy_project()?;
    let server = MockComfyUi::start(vec![MockResponse::status(
        400,
        json!({
            "error": {"type":"prompt_outputs_failed_validation","message":"Bearer test-secret missing class"},
            "node_errors": {"42":{"errors":[{"message":"required input is missing"}]}}
        }),
    )])?;
    let inputs = base_inputs(&server.url);
    let error = run_failure(
        &root,
        "prompt-rejection.json",
        &inputs,
        Some("Bearer test-secret"),
    )?;
    assert!(error.contains("submit prompt"));
    assert!(error.contains("/prompt"));
    assert!(error.contains("HTTP 400"));
    assert!(error.contains("prompt_outputs_failed_validation"));
    assert!(error.contains("node_errors"));
    assert!(error.contains("42"));
    assert!(error.contains("[redacted]"));
    assert!(!error.contains("test-secret"));
    let requests = server.finish();
    assert_eq!(
        requests[0].header("authorization"),
        Some("Bearer test-secret")
    );
    fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn oversized_http_error_body_is_bounded_and_marked_truncated()
-> Result<(), Box<dyn std::error::Error>> {
    let root = generated_comfy_project()?;
    let server = MockComfyUi::start(vec![MockResponse::status(
        500,
        json!({"error":"x".repeat(32 * 1024)}),
    )])?;
    let error = run_failure(&root, "long-error.json", &base_inputs(&server.url), None)?;
    assert!(error.contains("[truncated]"));
    assert!(error.len() < 18 * 1024, "error length: {}", error.len());
    assert_eq!(server.finish().len(), 1);
    fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn authorization_is_not_sent_to_mismatched_input_origin() -> Result<(), Box<dyn std::error::Error>>
{
    let root = generated_comfy_project()?;
    let attacker = MockComfyUi::start(vec![MockResponse::json(json!({"prompt_id":"stolen"}))])?;
    let inputs = base_inputs(&attacker.url);
    fs::write(root.join("auth-origin.json"), serde_json::to_vec(&inputs)?)?;
    let output = lfw_command(&root)
        .args([
            "run",
            "lightflow.comfy_run",
            "--inputs",
            "@auth-origin.json",
        ])
        .env("LIGHTFLOW_COMFYUI_AUTHORIZATION", "Bearer origin-secret")
        .env("LIGHTFLOW_COMFYUI_URL", "http://127.0.0.1:9/trusted")
        .output()?;
    assert!(!output.status.success());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(error.contains("refusing to send Authorization across origin"));
    assert!(!error.contains("origin-secret"));
    assert!(attacker.finish().is_empty());
    fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn history_execution_error_fails_immediately() -> Result<(), Box<dyn std::error::Error>> {
    let root = generated_comfy_project()?;
    let server = MockComfyUi::start(vec![
        MockResponse::json(json!({"prompt_id":"failed-prompt"})),
        MockResponse::json(json!({
            "failed-prompt": {
                "status": {
                    "completed": false,
                    "status_str": "error",
                    "messages": [["execution_error", {"node_id":"4"}]]
                },
                "outputs": {}
            }
        })),
    ])?;
    let error = run_failure(&root, "history-error.json", &base_inputs(&server.url), None)?;
    assert!(error.contains("history poll"));
    assert!(error.contains("execution_error"));
    assert_eq!(server.finish().len(), 2);
    fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn history_poll_respects_total_timeout() -> Result<(), Box<dyn std::error::Error>> {
    let root = generated_comfy_project()?;
    let mut responses = vec![MockResponse::json(json!({"prompt_id":"slow-prompt"}))];
    responses.extend((0..100).map(|_| MockResponse::json(json!({}))));
    let server = MockComfyUi::start(responses)?;
    let mut inputs = base_inputs(&server.url);
    inputs["timeout_ms"] = 15.into();
    inputs["poll_interval_ms"] = 1.into();
    let error = run_failure(&root, "timeout.json", &inputs, None)?;
    assert!(
        error.contains("history poll exceeded total timeout of 15ms"),
        "{error}"
    );
    assert!(server.finish().len() >= 2);
    fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn slow_upload_consumes_the_single_total_deadline() -> Result<(), Box<dyn std::error::Error>> {
    let root = generated_comfy_project()?;
    fs::write(root.join("slow.png"), b"slow upload")?;
    let server = MockComfyUi::start(vec![
        MockResponse::json(json!({"name":"slow.png","subfolder":"lightflow","type":"input"}))
            .delayed(Duration::from_millis(80)),
    ])?;
    let mut inputs = base_inputs(&server.url);
    inputs["uploads"] = json!([{"path":"slow.png"}]);
    inputs["timeout_ms"] = 20.into();
    let started = Instant::now();
    let error = run_failure(&root, "slow-upload.json", &inputs, None)?;
    assert!(
        error.contains("upload image exceeded total timeout of 20ms"),
        "{error}"
    );
    assert!(started.elapsed() < Duration::from_millis(200));
    let _ = server.finish();
    fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn slow_download_consumes_the_single_total_deadline() -> Result<(), Box<dyn std::error::Error>> {
    let root = generated_comfy_project()?;
    let server = MockComfyUi::start(vec![
        MockResponse::json(json!({"prompt_id":"slow-download"})),
        MockResponse::json(json!({
            "slow-download": {
                "status":{"completed":true,"status_str":"success"},
                "outputs":{"9":{"images":[{"filename":"slow.png","subfolder":"","type":"output"}]}}
            }
        })),
        MockResponse::bytes("image/png", b"late").delayed(Duration::from_millis(80)),
    ])?;
    let mut inputs = base_inputs(&server.url);
    inputs["timeout_ms"] = 20.into();
    let started = Instant::now();
    let error = run_failure(&root, "slow-download.json", &inputs, None)?;
    assert!(
        error.contains("download output exceeded total timeout of 20ms"),
        "{error}"
    );
    assert!(started.elapsed() < Duration::from_millis(200));
    let _ = server.finish();
    fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn remote_traversal_filename_is_downloaded_to_safe_basename()
-> Result<(), Box<dyn std::error::Error>> {
    let root = generated_comfy_project()?;
    let server = MockComfyUi::start(vec![
        MockResponse::json(json!({"prompt_id":"safe-path"})),
        MockResponse::json(json!({
            "safe-path": {
                "status": {"completed": true, "status_str":"success"},
                "outputs": {"7":{"images":[{"filename":"../../evil.png","subfolder":"","type":"output"}]}}
            }
        })),
        MockResponse::bytes("image/png", b"safe"),
    ])?;
    let execution = run_success(&root, "safe-path.json", &base_inputs(&server.url))?;
    let artifact_path = PathBuf::from(
        execution["artifacts"][0]["path"]
            .as_str()
            .expect("artifact path"),
    );
    assert!(artifact_path.starts_with(root.join(".lightflow/artifacts/comfyui")));
    assert!(
        !artifact_path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .contains("..")
    );
    assert_eq!(fs::read(&artifact_path)?, b"safe");
    assert!(
        server.finish()[2]
            .target
            .contains("filename=..%2F..%2Fevil.png")
    );
    fs::remove_dir_all(root)?;
    Ok(())
}

fn base_inputs(server_url: &str) -> Value {
    json!({
        "workflow": {"1":{"class_type":"TestNode","inputs":{"value":1}}},
        "server_url": server_url,
        "poll_interval_ms": 1
    })
}

fn generated_comfy_project() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    fs::create_dir_all(&root)?;
    lfw(&root, ["init"])?;
    lfw(
        &root,
        [
            "new",
            "comfy_run",
            "--category",
            "image",
            "--runtime",
            "lightflow.comfyui.workflow",
        ],
    )?;
    Ok(root)
}

fn run_failure(
    root: &Path,
    name: &str,
    inputs: &Value,
    authorization: Option<&str>,
) -> Result<String, Box<dyn std::error::Error>> {
    let output = run(root, name, inputs, authorization)?;
    assert!(!output.status.success(), "run unexpectedly succeeded");
    Ok(String::from_utf8_lossy(&output.stderr).into_owned())
}

fn run_success(
    root: &Path,
    name: &str,
    inputs: &Value,
) -> Result<Value, Box<dyn std::error::Error>> {
    let output = run(root, name, inputs, None)?;
    if !output.status.success() {
        return Err(format!(
            "run failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(serde_json::from_slice(&output.stdout)?)
}

fn run(
    root: &Path,
    name: &str,
    inputs: &Value,
    authorization: Option<&str>,
) -> Result<Output, Box<dyn std::error::Error>> {
    fs::write(root.join(name), serde_json::to_vec(inputs)?)?;
    let mut command = lfw_command(root);
    command.args([
        "run",
        "lightflow.comfy_run",
        "--inputs",
        &format!("@{name}"),
    ]);
    if let Some(authorization) = authorization {
        command.env("LIGHTFLOW_COMFYUI_AUTHORIZATION", authorization);
        command.env(
            "LIGHTFLOW_COMFYUI_URL",
            inputs["server_url"].as_str().expect("server URL"),
        );
    }
    Ok(command.output()?)
}

fn direct_error(root: &Path, inputs: Value) -> Result<String, Box<dyn std::error::Error>> {
    let service = ApiService::new(root);
    let error = service
        .execute_workflow(
            "lightflow.comfy_run",
            WorkflowExecutionOptions {
                inputs: inputs.as_object().expect("input object").clone(),
                ..WorkflowExecutionOptions::default()
            },
        )
        .expect_err("unsafe path must fail");
    Ok(error.to_string())
}
