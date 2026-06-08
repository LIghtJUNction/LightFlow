use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn cli_lists_assets_and_runs_text_plan_lifecycle() -> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"));

    let workflows = lightflow(&root, repo, ["assets", "workflows"])?;
    assert_eq!(
        workflows["assets"][0]["meta"]["id"],
        Value::String("workflow.text_plan".to_owned())
    );

    let created = lightflow(
        &root,
        repo,
        [
            "run",
            "create",
            "workflow.text_plan",
            "--id",
            "cli-run",
            "--inputs",
            r#"{"prompt":"draft a CLI test plan"}"#,
        ],
    )?;
    assert_eq!(created["run_id"], Value::String("cli-run".to_owned()));
    assert_eq!(
        created["steps"][0]["status"],
        Value::String("planned".to_owned())
    );

    let submitted = lightflow(&root, repo, ["run", "submit", "cli-run", "draft"])?;
    assert_eq!(
        submitted["steps"][0]["status"],
        Value::String("submitted".to_owned())
    );

    let request_path = root.join("ctx/home/1000/api/openai.chat/inbox/draft.req.json");
    let request_body = fs::read_to_string(&request_path)?;
    assert_eq!(
        serde_json::from_str::<Value>(&request_body)?,
        serde_json::json!({
            "messages": [
                {
                    "role": "user",
                    "content": "draft a CLI test plan"
                }
            ]
        })
    );

    let outbox = root.join("ctx/home/1000/api/openai.chat/outbox");
    fs::create_dir_all(&outbox)?;
    fs::write(outbox.join("draft.resp.json"), "{\"ok\":true}\n")?;
    fs::write(outbox.join("draft.fingerprint"), "fnv1a64:cli\n")?;
    fs::write(
        outbox.join("draft.route.json"),
        "{\"provider\":\"local\",\"model\":\"test-model\",\"reason\":\"integration_test\"}\n",
    )?;

    let refreshed = lightflow(&root, repo, ["run", "refresh", "cli-run"])?;
    let step = &refreshed["steps"][0];
    assert_eq!(step["status"], Value::String("succeeded".to_owned()));
    assert_eq!(step["provider_id"], Value::String("local".to_owned()));
    assert_eq!(step["model_id"], Value::String("test-model".to_owned()));
    assert_eq!(
        step["route_decision"],
        Value::String("integration_test".to_owned())
    );
    assert_eq!(step["fingerprint"], Value::String("fnv1a64:cli".to_owned()));

    let events = lightflow_text(&root, repo, ["run", "events", "cli-run"])?;
    assert!(events.contains("\"event\":\"run.created\""));
    assert!(events.contains("\"event\":\"step.submitted\""));
    assert!(events.contains("\"event\":\"step.succeeded\""));

    fs::remove_dir_all(root)?;
    Ok(())
}

fn lightflow<const N: usize>(
    root: &Path,
    repo: &Path,
    args: [&str; N],
) -> Result<Value, Box<dyn std::error::Error>> {
    let stdout = lightflow_text(root, repo, args)?;
    Ok(serde_json::from_str(&stdout)?)
}

fn lightflow_text<const N: usize>(
    root: &Path,
    repo: &Path,
    args: [&str; N],
) -> Result<String, Box<dyn std::error::Error>> {
    let output = Command::new(env!("CARGO_BIN_EXE_lightflow"))
        .args(args)
        .current_dir(repo)
        .env("XDG_CONFIG_HOME", root.join("cfg"))
        .env("XDG_STATE_HOME", root.join("state"))
        .env("XDG_CACHE_HOME", root.join("cache"))
        .env("XDG_RUNTIME_DIR", root.join("runtime"))
        .env("LIGHTFLOW_CTX_MOUNT", root.join("ctx"))
        .env("LIGHTFLOW_CTX_UID", "1000")
        .output()?;

    if !output.status.success() {
        return Err(format!(
            "lightflow failed with status {}\nstdout:\n{}\nstderr:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }

    Ok(String::from_utf8(output.stdout)?)
}

fn unique_temp_root() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock must be after unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("lightflow-cli-test-{}-{nanos}", std::process::id()))
}
