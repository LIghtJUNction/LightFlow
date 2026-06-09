use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use lightflow::api::ApiService;
use lightflow::mcp;
use lightflow::runs::{RunStore, RuntimeDirs};

#[test]
fn cli_lists_assets_and_runs_text_plan_lifecycle() -> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"));

    let workflows = lightflow(&root, repo, ["assets", "workflows"])?;
    assert_eq!(
        workflows["assets"][0]["meta"]["id"],
        Value::String("workflow.text_plan".to_owned())
    );

    let preview = lightflow(
        &root,
        repo,
        [
            "run",
            "preview",
            "workflow.text_plan",
            "--id",
            "cli-run",
            "--inputs",
            r#"{"prompt":"draft a CLI test plan"}"#,
        ],
    )?;
    assert_eq!(preview["run_id"], Value::String("cli-run".to_owned()));
    assert_eq!(preview["ready"], Value::Bool(true));
    assert_eq!(
        preview["steps"][0]["rendered_request"],
        serde_json::json!({
            "messages": [
                {
                    "role": "user",
                    "content": "draft a CLI test plan"
                }
            ]
        })
    );
    assert!(!root.join("state/lightflow/runs/cli-run").exists());

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

    let duplicate_create = lightflow_failure(
        &root,
        repo,
        [
            "run",
            "create",
            "workflow.text_plan",
            "--id",
            "cli-run",
            "--inputs",
            r#"{"prompt":"should not overwrite"}"#,
        ],
    )?;
    assert_eq!(duplicate_create.status_code, 2);
    assert!(duplicate_create.stdout.is_empty());
    assert!(
        duplicate_create
            .stderr
            .contains("conflict: run cli-run already exists")
    );
    let request = fs::read_to_string(root.join("state/lightflow/runs/cli-run/request.json"))?;
    assert!(request.contains("draft a CLI test plan"));
    assert!(!request.contains("should not overwrite"));

    let runs = lightflow(&root, repo, ["run", "list"])?;
    assert_eq!(
        runs["runs"][0]["run_id"],
        Value::String("cli-run".to_owned())
    );
    let planned_status = lightflow(&root, repo, ["run", "status", "cli-run"])?;
    assert_eq!(
        planned_status["status"],
        Value::String("planned".to_owned())
    );
    assert_eq!(planned_status["planned_steps"], Value::Number(1.into()));

    let stored_request = lightflow(&root, repo, ["run", "request", "cli-run"])?;
    assert_eq!(
        stored_request["inputs"]["prompt"],
        Value::String("draft a CLI test plan".to_owned())
    );
    let resolved_workflow = lightflow(&root, repo, ["run", "workflow", "cli-run"])?;
    assert_eq!(
        resolved_workflow["id"],
        Value::String("workflow.text_plan".to_owned())
    );
    assert_eq!(
        resolved_workflow["steps"][0]["step_id"],
        Value::String("draft".to_owned())
    );

    let submitted = lightflow(&root, repo, ["run", "submit", "cli-run", "draft"])?;
    assert_eq!(
        submitted["steps"][0]["status"],
        Value::String("submitted".to_owned())
    );
    let running_status = lightflow(&root, repo, ["run", "status", "cli-run"])?;
    assert_eq!(
        running_status["status"],
        Value::String("running".to_owned())
    );
    assert_eq!(running_status["submitted_steps"], Value::Number(1.into()));

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

    let duplicate_submit = lightflow_failure(
        &root,
        repo,
        [
            "run",
            "submit",
            "cli-run",
            "draft",
            r#"{"messages":[{"role":"user","content":"should not overwrite"}]}"#,
        ],
    )?;
    assert_eq!(duplicate_submit.status_code, 2);
    assert!(duplicate_submit.stdout.is_empty());
    assert!(
        duplicate_submit
            .stderr
            .contains("conflict: run step draft is already submitted")
    );
    assert_eq!(fs::read_to_string(&request_path)?, request_body);

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
    let succeeded_status = lightflow(&root, repo, ["run", "status", "cli-run"])?;
    assert_eq!(
        succeeded_status["status"],
        Value::String("succeeded".to_owned())
    );
    assert_eq!(succeeded_status["succeeded_steps"], Value::Number(1.into()));

    let events = lightflow_text(&root, repo, ["run", "events", "cli-run"])?;
    assert!(events.contains("\"event\":\"run.created\""));
    assert!(events.contains("\"event\":\"step.submitted\""));
    assert!(events.contains("\"event\":\"step.succeeded\""));

    let abi = lightflow(&root, repo, ["ctx", "abi"])?;
    assert_eq!(abi["kernel"], Value::String("fuse".to_owned()));
    assert_eq!(abi["kernel_tree"], Value::Bool(false));
    assert_eq!(
        abi["upstream"],
        Value::String("generic kernel primitives only".to_owned())
    );

    let channel = lightflow(&root, repo, ["ctx", "chan", "fengying"])?;
    assert_eq!(
        channel["url"],
        Value::String(
            root.join("ctx/chan/fengying/url")
                .to_string_lossy()
                .into_owned()
        )
    );
    assert_eq!(
        channel["model_filter"],
        Value::String(
            root.join("ctx/chan/fengying/mod")
                .to_string_lossy()
                .into_owned()
        )
    );

    let job = lightflow(&root, repo, ["ctx", "job", "translate.zh"])?;
    assert_eq!(
        job["spec"],
        Value::String(
            root.join("ctx/home/1000/job/translate.zh/spec")
                .to_string_lossy()
                .into_owned()
        )
    );
    assert_eq!(
        job["request"],
        Value::String(
            root.join("ctx/home/1000/job/translate.zh/req")
                .to_string_lossy()
                .into_owned()
        )
    );

    let hook = lightflow(&root, repo, ["ctx", "hook", "daily-translate"])?;
    assert_eq!(
        hook["trigger"],
        Value::String(
            root.join("ctx/home/1000/hook/daily-translate/trigger")
                .to_string_lossy()
                .into_owned()
        )
    );
    assert_eq!(
        hook["request"],
        Value::String(
            root.join("ctx/home/1000/hook/daily-translate/req")
                .to_string_lossy()
                .into_owned()
        )
    );
    assert_eq!(
        hook["log"],
        Value::String(
            root.join("ctx/home/1000/hook/daily-translate/log.jsonl")
                .to_string_lossy()
                .into_owned()
        )
    );

    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn cli_rejects_ambiguous_or_extra_arguments() -> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"));

    let extra_asset_arg = lightflow_failure(&root, repo, ["assets", "workflows", "extra"])?;
    assert_eq!(extra_asset_arg.status_code, 2);
    assert!(extra_asset_arg.stdout.is_empty());
    assert!(
        extra_asset_arg
            .stderr
            .contains("unexpected argument for assets: extra")
    );

    let missing_id_value = lightflow_failure(
        &root,
        repo,
        [
            "run",
            "preview",
            "workflow.text_plan",
            "--id",
            "--inputs",
            r#"{"prompt":"hello"}"#,
        ],
    )?;
    assert_eq!(missing_id_value.status_code, 2);
    assert!(missing_id_value.stderr.contains("missing value for --id"));

    let unknown_flag = lightflow_failure(
        &root,
        repo,
        ["run", "preview", "workflow.text_plan", "--unknown", "value"],
    )?;
    assert_eq!(unknown_flag.status_code, 2);
    assert!(
        unknown_flag
            .stderr
            .contains("unexpected argument for run preview: --unknown")
    );

    let duplicate_flag = lightflow_failure(
        &root,
        repo,
        [
            "run",
            "preview",
            "workflow.text_plan",
            "--id",
            "first",
            "--id",
            "second",
        ],
    )?;
    assert_eq!(duplicate_flag.status_code, 2);
    assert!(duplicate_flag.stderr.contains("duplicate flag --id"));

    let extra_submit_arg = lightflow_failure(
        &root,
        repo,
        [
            "run",
            "submit",
            "run-001",
            "draft",
            r#"{"model":"demo"}"#,
            "extra",
        ],
    )?;
    assert_eq!(extra_submit_arg.status_code, 2);
    assert!(
        extra_submit_arg
            .stderr
            .contains("unexpected argument for run submit: extra")
    );

    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn mcp_exposes_runtime_gateway_contract() -> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    let repo = root.join("repo");
    fs::create_dir_all(&repo)?;
    let service = ApiService::new(
        &repo,
        RunStore::new(RuntimeDirs::new(
            root.join("cfg"),
            root.join("state"),
            root.join("cache"),
            root.join("runtime"),
        )),
    );

    let tools = mcp_result(
        &service,
        serde_json::json!({ "id": 1, "method": "tools/list" }),
    );
    let tool_names = tools["tools"]
        .as_array()
        .expect("tools/list returns an array")
        .iter()
        .map(|tool| tool["name"].as_str().unwrap_or_default())
        .collect::<Vec<_>>();
    for required in [
        "lightflow.list_workflows",
        "lightflow.workflow.list",
        "lightflow.get_workflow",
        "lightflow.workflow.open",
        "lightflow.workflow.read_region",
        "lightflow.validate_workflow",
        "lightflow.workflow.validate",
        "lightflow.save_workflow",
        "lightflow.workflow.apply_patch",
        "lightflow.preview_run",
        "lightflow.create_run",
        "lightflow.list_runs",
        "lightflow.get_run",
        "lightflow.run_status",
        "lightflow.cancel_run",
        "lightflow.run_events",
        "lightflow.run_trace",
    ] {
        assert!(
            tool_names.contains(&required),
            "missing MCP tool {required}"
        );
    }

    let resources = mcp_result(
        &service,
        serde_json::json!({ "id": 2, "method": "resources/list" }),
    );
    let resource_uris = resources["resources"]
        .as_array()
        .expect("resources/list returns an array")
        .iter()
        .map(|resource| resource["uri"].as_str().unwrap_or_default())
        .collect::<Vec<_>>();
    for required in [
        "lightflow://workflows",
        "lightflow://nodes",
        "lightflow://runs",
        "lightflow://mcp",
        "lightflow://runtime",
        "lightflow://ctx-abi",
    ] {
        assert!(
            resource_uris.contains(&required),
            "missing MCP resource {required}"
        );
    }

    let runtime_info = mcp_result(
        &service,
        serde_json::json!({
            "id": 4,
            "method": "resources/read",
            "params": { "uri": "lightflow://runtime" }
        }),
    );
    let runtime_info: Value =
        serde_json::from_str(runtime_info["contents"][0]["text"].as_str().unwrap())?;
    assert_eq!(runtime_info["frame"]["encoding"], "flatbuffers");
    assert_eq!(runtime_info["frame"]["file_identifier"], "LFRS");
    assert_eq!(runtime_info["transports"][1]["name"], "webtransport");
    assert_eq!(runtime_info["transports"][1]["status"], "available");
    assert_eq!(
        runtime_info["transports"][1]["command"],
        "lightflow stream serve-webtransport --port 4433"
    );
    assert_eq!(
        runtime_info["transports"][1]["tls"]["certificate_hash_option"],
        "serverCertificateHashes"
    );

    let abi = mcp_result(
        &service,
        serde_json::json!({
            "id": 5,
            "method": "resources/read",
            "params": { "uri": "lightflow://ctx-abi" }
        }),
    );
    let abi: Value = serde_json::from_str(abi["contents"][0]["text"].as_str().unwrap())?;
    assert_eq!(abi["kernel"], "fuse");
    assert_eq!(abi["kernel_tree"], false);
    assert_eq!(abi["upstream"], "generic kernel primitives only");

    let default_workflow = mcp_tool(
        &service,
        "lightflow.get_workflow",
        serde_json::json!({ "workflow_id": "workflow.default" }),
    );
    assert_eq!(default_workflow["id"], "workflow.default");
    assert_eq!(default_workflow["nodes"][0]["id"], "input");
    assert_eq!(default_workflow["edges"][0]["from"]["node"], "input");
    assert_eq!(default_workflow["edges"][0]["to"]["node"], "tool");

    let ui_open = mcp_tool(
        &service,
        "lightflow.workflow.open",
        serde_json::json!({
            "workflow_id": "workflow.default",
            "mode": "metadata_only"
        }),
    );
    assert_eq!(ui_open["workflow_id"], "workflow.default");
    assert_eq!(ui_open["workflow"]["id"], "workflow.default");

    let ui_region = mcp_tool(
        &service,
        "lightflow.workflow.read_region",
        serde_json::json!({
            "workflow_id": "workflow.default",
            "region": {
                "x": 0,
                "y": 0,
                "width": 1800,
                "height": 1200,
                "zoom": 1,
                "limit": 500,
                "cursor": null
            }
        }),
    );
    assert_eq!(ui_region["workflow_id"], "workflow.default");
    assert_eq!(ui_region["nodes"][0]["title"], "Workflow Input");
    assert_eq!(ui_region["edges"][0]["id"], "edge-1");

    let validation = mcp_tool(
        &service,
        "lightflow.validate_workflow",
        serde_json::json!({ "workflow": default_workflow.clone() }),
    );
    assert_eq!(validation["valid"], true);
    assert_eq!(
        validation["topological_order"],
        serde_json::json!(["input", "tool", "output"])
    );

    let ui_validation = mcp_tool(
        &service,
        "lightflow.workflow.validate",
        serde_json::json!({
            "workflow_id": "workflow.default",
            "base_revision": ui_region["revision"],
            "visible_region": {
                "x": 0,
                "y": 0,
                "width": 1800,
                "height": 1200,
                "zoom": 1,
                "limit": 500,
                "cursor": null
            },
            "local_patch": {
                "workflow_id": "workflow.default",
                "base_revision": ui_region["revision"],
                "ops": []
            }
        }),
    );
    assert_eq!(ui_validation["valid"], true);

    let cyclic = serde_json::json!({
        "id": "workflow.cyclic",
        "name": "Cyclic Workflow",
        "nodes": [
            {
                "id": "a",
                "kind": "task",
                "position": { "x": 0, "y": 0 },
                "inputs": [{ "name": "in", "type": "flow" }],
                "outputs": [{ "name": "out", "type": "flow" }]
            },
            {
                "id": "b",
                "kind": "task",
                "position": { "x": 200, "y": 0 },
                "inputs": [{ "name": "in", "type": "flow" }],
                "outputs": [{ "name": "out", "type": "flow" }]
            }
        ],
        "edges": [
            {
                "from": { "node": "a", "port": "out" },
                "to": { "node": "b", "port": "in" }
            },
            {
                "from": { "node": "b", "port": "out" },
                "to": { "node": "a", "port": "in" }
            }
        ]
    });
    let cyclic_validation = mcp_tool(
        &service,
        "lightflow.validate_workflow",
        serde_json::json!({ "workflow": cyclic }),
    );
    assert_eq!(cyclic_validation["valid"], false);
    assert!(
        cyclic_validation["issues"][0]
            .as_str()
            .unwrap()
            .contains("cycle")
    );
    assert_eq!(
        cyclic_validation["topological_order"],
        serde_json::json!([])
    );

    let mut saved_workflow = default_workflow;
    saved_workflow["id"] = Value::String("workflow.saved".to_owned());
    saved_workflow["name"] = Value::String("Saved Workflow".to_owned());
    let saved = mcp_tool(
        &service,
        "lightflow.save_workflow",
        serde_json::json!({ "workflow": saved_workflow }),
    );
    assert_eq!(saved["workflow"]["id"], "workflow.saved");
    assert!(
        repo.join("lightflow/workflows/workflow.saved.json")
            .is_file()
    );

    let patched = mcp_tool(
        &service,
        "lightflow.workflow.apply_patch",
        serde_json::json!({
            "patch": {
                "workflow_id": "workflow.saved",
                "base_revision": saved["revision"],
                "ops": [
                    {
                        "op": "move_node",
                        "node_id": "input",
                        "position": { "x": 100.5, "y": 220.25 }
                    }
                ]
            }
        }),
    );
    assert_eq!(patched["workflow"]["nodes"][0]["position"]["x"], 101);
    assert_eq!(patched["workflow"]["nodes"][0]["position"]["y"], 220);
    assert!(patched["revision"].as_str().unwrap().starts_with("rev-"));

    let run = mcp_tool(
        &service,
        "lightflow.create_run",
        serde_json::json!({
            "workflow_id": "workflow.saved",
            "run_id": "mcp-run",
            "inputs": { "prompt": "hello" }
        }),
    );
    assert_eq!(run["run_id"], "mcp-run");
    assert_eq!(run["workflow_asset_id"], "workflow.saved");
    assert_eq!(run["cancelled"], false);

    let cancelled = mcp_tool(
        &service,
        "lightflow.cancel_run",
        serde_json::json!({ "run_id": "mcp-run" }),
    );
    assert_eq!(cancelled["cancelled"], true);
    let status = mcp_tool(
        &service,
        "lightflow.run_status",
        serde_json::json!({ "run_id": "mcp-run" }),
    );
    assert_eq!(status["status"], "cancelled");

    let mcp_info = mcp_result(
        &service,
        serde_json::json!({
            "id": 3,
            "method": "resources/read",
            "params": { "uri": "lightflow://mcp" }
        }),
    );
    let mcp_info: Value = serde_json::from_str(mcp_info["contents"][0]["text"].as_str().unwrap())?;
    assert_eq!(mcp_info["endpoint"], "http://127.0.0.1:5174/mcp");

    let _ = fs::remove_dir_all(root);
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

fn lightflow_failure<const N: usize>(
    root: &Path,
    repo: &Path,
    args: [&str; N],
) -> Result<CliFailure, Box<dyn std::error::Error>> {
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

    if output.status.success() {
        return Err(format!(
            "lightflow unexpectedly succeeded\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }

    Ok(CliFailure {
        status_code: output.status.code().unwrap_or_default(),
        stdout: String::from_utf8(output.stdout)?,
        stderr: String::from_utf8(output.stderr)?,
    })
}

fn mcp_tool(service: &ApiService, name: &str, arguments: Value) -> Value {
    mcp_result(
        service,
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": 99,
            "method": "tools/call",
            "params": {
                "name": name,
                "arguments": arguments
            }
        }),
    )["structuredContent"]
        .clone()
}

fn mcp_result(service: &ApiService, request: Value) -> Value {
    let response = mcp::handle_request(service, request);
    assert!(response.get("error").is_none(), "MCP error: {response}");
    response["result"].clone()
}

#[derive(Debug)]
struct CliFailure {
    status_code: i32,
    stdout: String,
    stderr: String,
}

fn unique_temp_root() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock must be after unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("lightflow-cli-test-{}-{nanos}", std::process::id()))
}
