mod support;

use std::fs;
#[cfg(feature = "rig")]
use std::io::{Read, Write};
#[cfg(feature = "rig")]
use std::net::TcpListener;
#[cfg(not(feature = "rig"))]
use std::process::Command;
#[cfg(feature = "rig")]
use std::thread;
#[cfg(feature = "rig")]
use support::{lfw, lfw_command};
use support::{unique_temp_root, write_workflow_crate};

#[cfg(feature = "rig")]
fn write_rig_workflow(root: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    fs::write(
        root.join("Cargo.toml"),
        format!(
            r#"[workspace]
resolver = "3"
members = ["workflows/*/*"]

[workspace.dependencies]
lightflow = {{ path = {:?}, features = ["rig"] }}
"#,
            env!("CARGO_MANIFEST_DIR")
        ),
    )?;
    write_workflow_crate(
        root,
        "lightflow.test_rig_llm",
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow!()
        .name("Test RIG LLM")
        .description("Generate text through the LightFlow RIG LLM runtime.")
        .input("prompt", "text")
        .input("system", "text")
        .input("provider", "text")
        .input("model", "text")
        .input("api_key", "text")
        .input("base_url", "text")
        .input("temperature", "number")
        .input("max_tokens", "integer")
        .input("additional_params", "json")
        .output("text", "text")
        .output("response", "text")
        .output("provider", "text")
        .output("model", "text")
        .runtime("rig_runtime", "lightflow.llm.generate")
        .build()
}
"#,
    )?;
    Ok(())
}

#[test]
#[cfg(feature = "rig")]
fn rig_llm_runtime_runs_mock_provider() -> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    fs::create_dir_all(&root)?;
    write_rig_workflow(&root)?;

    let execution = lfw(
        &root,
        [
            "run",
            "lightflow.test_rig_llm",
            "-i",
            "provider=\"mock\"",
            "-i",
            "model=\"fake-llm\"",
            "-i",
            "prompt=\"hello\"",
        ],
    )?;
    assert_eq!(execution["outputs"]["text"], "mock:fake-llm:hello");
    assert_eq!(execution["outputs"]["response"], "mock:fake-llm:hello");
    assert_eq!(execution["outputs"]["provider"], "mock");
    assert_eq!(execution["outputs"]["model"], "fake-llm");

    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
#[cfg(feature = "rig")]
fn rig_llm_runtime_runs_openai_compatible_provider() -> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    fs::create_dir_all(&root)?;
    write_rig_workflow(&root)?;
    let (base_url, server) = start_openai_compatible_server()?;
    let base_url_input = format!("base_url=\"{base_url}/v1\"");

    let output = lfw_command(&root)
        .args([
            "run",
            "lightflow.test_rig_llm",
            "-i",
            "provider=\"openai-compatible\"",
            "-i",
            "model=\"fake-llm\"",
            "-i",
            "prompt=\"hello\"",
            "-i",
            "api_key=\"test-key\"",
            "-i",
            &base_url_input,
        ])
        .output()?;
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let execution: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(execution["outputs"]["text"], "external:hello");
    assert_eq!(execution["outputs"]["response"], "external:hello");
    assert_eq!(execution["outputs"]["provider"], "openai-compatible");
    assert_eq!(execution["outputs"]["model"], "fake-llm");

    let request_line = server
        .join()
        .map_err(|_| "OpenAI-compatible test server panicked")??;
    assert_eq!(request_line, "POST /v1/chat/completions HTTP/1.1");

    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
#[cfg(not(feature = "rig"))]
fn rig_llm_runtime_requires_rig_feature() -> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    fs::create_dir_all(&root)?;
    fs::write(
        root.join("Cargo.toml"),
        format!(
            r#"[workspace]
resolver = "3"
members = ["workflows/*/*"]

[workspace.dependencies]
lightflow = {{ path = {:?} }}
"#,
            env!("CARGO_MANIFEST_DIR")
        ),
    )?;
    write_workflow_crate(
        &root,
        "lightflow.test_rig_llm",
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow!()
        .name("Test RIG LLM")
        .input("prompt", "text")
        .input("provider", "text")
        .input("model", "text")
        .output("text", "text")
        .runtime("rig_runtime", "lightflow.llm.generate")
        .build()
}
"#,
    )?;

    let output = Command::new(env!("CARGO_BIN_EXE_lfw"))
        .args([
            "run",
            "lightflow.test_rig_llm",
            "-i",
            "provider=\"mock\"",
            "-i",
            "model=\"fake-llm\"",
            "-i",
            "prompt=\"hello\"",
        ])
        .current_dir(&root)
        .env("HOME", &root)
        .env("SHELL", "/bin/zsh")
        .env("XDG_CONFIG_HOME", root.join(".test-xdg/config"))
        .env("XDG_DATA_HOME", root.join(".test-xdg/data"))
        .env_remove("LFW_PATH")
        .output()?;
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("--features rig"),
        "stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[cfg(feature = "rig")]
fn start_openai_compatible_server()
-> Result<(String, thread::JoinHandle<Result<String, String>>), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let addr = listener.local_addr()?;
    let handle = thread::spawn(move || -> Result<String, String> {
        let (mut stream, _) = listener.accept().map_err(|error| error.to_string())?;
        let mut buffer = [0_u8; 8192];
        let read = stream
            .read(&mut buffer)
            .map_err(|error| error.to_string())?;
        let request = String::from_utf8_lossy(&buffer[..read]).to_string();
        let request_line = request
            .lines()
            .next()
            .ok_or_else(|| "empty request".to_owned())?
            .to_owned();
        let body = r#"{
  "id": "chatcmpl-lightflow-test",
  "object": "chat.completion",
  "created": 0,
  "model": "fake-llm",
  "choices": [
    {
      "index": 0,
      "message": {
        "role": "assistant",
        "content": "external:hello"
      },
      "finish_reason": "stop"
    }
  ],
  "usage": {
    "prompt_tokens": 1,
    "completion_tokens": 1,
    "total_tokens": 2
  }
}"#;
        write!(
            stream,
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            body.len(),
            body
        )
        .map_err(|error| error.to_string())?;
        Ok(request_line)
    });
    Ok((format!("http://{addr}"), handle))
}
