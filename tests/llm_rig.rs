mod support;

use std::fs;
use std::io;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::thread;
use support::{lfw_command, lfw_with_env_values, unique_temp_root};

type TestServer = (String, thread::JoinHandle<Result<String, String>>);

#[test]
fn rig_package_runner_runs_mock_provider() -> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    fs::create_dir_all(&root)?;
    let execution = run_rig(
        &root,
        [
            "run",
            "lightflow.rig_llm",
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
    assert_eq!(execution["runtime"]["executor_id"], "runner.v1");
    assert!(
        execution["runtime"]["replay_fingerprint"]["runner"]["implementation"]
            .as_str()
            .is_some_and(|identity| !identity.is_empty())
    );

    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn rig_rejects_api_key_input_without_persisting_secret() -> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    fs::create_dir_all(&root)?;
    let rig_project = Path::new(env!("CARGO_MANIFEST_DIR")).join("projects/lightflow-rig");
    let secret = "history-secret-must-not-appear";
    let secret_input = format!("api_key=\"{secret}\"");
    let output = lfw_command(&root)
        .args([
            "run",
            "lightflow.rig_llm",
            "-i",
            "provider=\"mock\"",
            "-i",
            "model=\"fake-llm\"",
            "-i",
            "prompt=\"hello\"",
            "-i",
            &secret_input,
        ])
        .env("LFW_PATH", rig_project)
        .output()?;
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("unknown input"));
    let history_root = root.join(".lightflow/runs");
    if history_root.exists() {
        assert_history_omits(&history_root, secret)?;
    }

    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn rig_rejects_user_endpoint_without_sending_official_key() -> Result<(), Box<dyn std::error::Error>>
{
    let root = unique_temp_root();
    fs::create_dir_all(&root)?;
    let listener = TcpListener::bind("127.0.0.1:0")?;
    listener.set_nonblocking(true)?;
    let malicious_url = format!("base_url=\"http://{}\"", listener.local_addr()?);
    let official_secret = "official-key-must-not-leave";
    let rig_project = Path::new(env!("CARGO_MANIFEST_DIR")).join("projects/lightflow-rig");
    let output = lfw_command(&root)
        .args([
            "run",
            "lightflow.rig_llm",
            "-i",
            "provider=\"openai\"",
            "-i",
            "model=\"fake-llm\"",
            "-i",
            "prompt=\"hello\"",
            "-i",
            &malicious_url,
        ])
        .env("LFW_PATH", rig_project)
        .env("OPENAI_API_KEY", official_secret)
        .output()?;
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("unknown input"));
    assert!(matches!(
        listener.accept(),
        Err(error) if error.kind() == io::ErrorKind::WouldBlock
    ));
    let history_root = root.join(".lightflow/runs");
    if history_root.exists() {
        assert_history_omits(&history_root, official_secret)?;
    }

    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn rig_package_runner_runs_openai_compatible_provider() -> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    fs::create_dir_all(&root)?;
    let (base_url, server) = match start_openai_compatible_server() {
        Ok(server) => server,
        Err(error)
            if error
                .downcast_ref::<io::Error>()
                .is_some_and(|error| error.kind() == io::ErrorKind::PermissionDenied) =>
        {
            eprintln!("skipping local OpenAI-compatible socket: {error}");
            return Ok(());
        }
        Err(error) => return Err(error),
    };
    let rig_project = Path::new(env!("CARGO_MANIFEST_DIR")).join("projects/lightflow-rig");
    let compatible_base_url = format!("{base_url}/v1");
    let execution = lfw_with_env_values(
        &root,
        [
            "run",
            "lightflow.rig_llm",
            "-i",
            "provider=\"openai-compatible\"",
            "-i",
            "model=\"fake-llm\"",
            "-i",
            "prompt=\"hello\"",
        ],
        [
            ("LFW_PATH", rig_project.to_str().unwrap()),
            ("OPENAI_COMPATIBLE_API_KEY", "test-key"),
            ("OPENAI_COMPATIBLE_BASE_URL", compatible_base_url.as_str()),
        ],
    )?;

    assert_eq!(execution["outputs"]["text"], "external:hello");
    assert_eq!(execution["outputs"]["response"], "external:hello");
    assert_eq!(execution["outputs"]["provider"], "openai-compatible");
    assert_eq!(execution["outputs"]["model"], "fake-llm");
    assert_eq!(execution["runtime"]["executor_id"], "runner.v1");
    let request_line = server
        .join()
        .map_err(|_| "OpenAI-compatible test server panicked")??;
    assert_eq!(request_line, "POST /v1/chat/completions HTTP/1.1");

    let _ = fs::remove_dir_all(root);
    Ok(())
}

fn run_rig<const N: usize>(
    root: &Path,
    args: [&str; N],
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let rig_project = Path::new(env!("CARGO_MANIFEST_DIR")).join("projects/lightflow-rig");
    lfw_with_env_values(root, args, [("LFW_PATH", rig_project.to_str().unwrap())])
}

fn assert_history_omits(path: &Path, secret: &str) -> Result<(), Box<dyn std::error::Error>> {
    for entry in fs::read_dir(path)? {
        let path = entry?.path();
        if path.is_dir() {
            assert_history_omits(&path, secret)?;
        } else {
            let contents = fs::read_to_string(&path)?;
            assert!(
                !contents.contains(secret),
                "secret leaked to {}",
                path.display()
            );
            assert!(
                !contents.contains("\"api_key\""),
                "secret field leaked to {}",
                path.display()
            );
            assert!(
                !contents.contains("\"base_url\""),
                "endpoint field leaked to {}",
                path.display()
            );
        }
    }
    Ok(())
}

fn start_openai_compatible_server() -> Result<TestServer, Box<dyn std::error::Error>> {
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
