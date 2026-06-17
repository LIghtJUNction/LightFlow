use lightflow::api::ApiService;
use lightflow::mcp;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn cli_reads_rust_workflows_and_resolves_dependencies() -> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    write_project_specs(&root)?;

    let list = lightflow(&root, ["workflows", "list"])?;
    let ids = list["workflows"]
        .as_array()
        .expect("workflows list returns an array")
        .iter()
        .map(|workflow| workflow["id"].as_str().unwrap_or_default())
        .collect::<Vec<_>>();
    assert_eq!(
        ids,
        vec!["lightflow.child", "lightflow.parent", "lightflow.sink"]
    );

    let child = lightflow(&root, ["workflows", "get", "lightflow.child"])?;
    assert_eq!(child["id"], Value::String("lightflow.child".to_owned()));
    assert_eq!(child["version"], Value::String("0.1.0".to_owned()));

    let deps = lfw(&root, ["deps", "lightflow.parent"])?;
    assert_eq!(
        deps["workflow_id"],
        Value::String("lightflow.parent".to_owned())
    );
    assert_eq!(deps["complete"], Value::Bool(true));
    assert_eq!(
        deps["workflows"],
        serde_json::json!(["lightflow.child", "lightflow.parent", "lightflow.sink"])
    );
    assert_eq!(
        deps["workflow_order"],
        serde_json::json!(["lightflow.child", "lightflow.sink", "lightflow.parent"])
    );

    let brief = lfw(&root, ["list"])?;
    assert_eq!(brief["workflows"][0]["id"], "lightflow.child");
    assert!(brief["workflows"][0].get("nodes").is_some());
    assert!(brief["workflows"][0].get("description").is_none());

    let detail = lfw(&root, ["ls", "--detail"])?;
    assert_eq!(detail["workflows"][1]["id"], "lightflow.parent");
    assert_eq!(detail["workflows"][1]["nodes"][0]["id"], "nested");
    assert_eq!(detail["workflows"][1]["edges"][0]["from"]["node"], "nested");

    let validation = lightflow(
        &root,
        [
            "workflows",
            "validate",
            r#"{
              "id": "lightflow.invalid",
              "version": "0.1.0",
              "name": "Invalid",
              "nodes": [{ "id": "missing", "workflow_id": "lightflow.missing" }],
              "edges": []
            }"#,
        ],
    )?;
    assert_eq!(validation["valid"], Value::Bool(false));
    assert!(
        validation["issues"][0]
            .as_str()
            .unwrap()
            .contains("missing workflow")
    );

    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn lfw_init_and_add_create_rust_workflow_files() -> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    fs::create_dir_all(&root)?;

    let init = lfw(&root, ["init"])?;
    assert!(
        init["created"]
            .as_array()
            .unwrap()
            .iter()
            .any(|path| path.as_str().unwrap().ends_with("Cargo.toml"))
    );
    assert!(init["created"].as_array().unwrap().iter().any(|path| {
        path.as_str()
            .unwrap()
            .ends_with("lightflow.example/src/lib.rs")
    }));

    let added = lfw(&root, ["add", "extra", "--name", "Extra Workflow"])?;
    assert_eq!(added["workflow_id"], "lightflow.extra");
    let manifest = fs::read_to_string(root.join("lightflow/workflows/lightflow.extra/Cargo.toml"))?;
    assert!(manifest.contains("name = \"lightflow-extra\""));
    let path = root.join("lightflow/workflows/lightflow.extra/src/lib.rs");
    let source = fs::read_to_string(path)?;
    assert!(source.contains("workflow(\"lightflow.extra\")"));
    assert!(source.contains(".name(\"Extra Workflow\")"));
    assert!(
        !root
            .join("lightflow/workflows/lightflow.extra/src/main.rs")
            .exists()
    );

    let workflow = lightflow(&root, ["workflows", "get", "lightflow.extra"])?;
    assert_eq!(workflow["id"], "lightflow.extra");

    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn mcp_exposes_workflow_only_tools() -> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    write_project_specs(&root)?;
    let service = ApiService::new(&root);

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
    assert_eq!(
        tool_names,
        vec![
            "lightflow.workflow.list",
            "lightflow.workflow.get",
            "lightflow.workflow.dependencies",
            "lightflow.workflow.validate",
            "lightflow.workflow.save"
        ]
    );

    let workflow = mcp_tool(
        &service,
        "lightflow.workflow.get",
        serde_json::json!({ "workflow_id": "lightflow.parent" }),
    );
    assert_eq!(workflow["id"], "lightflow.parent");
    assert_eq!(workflow["nodes"][0]["workflow_id"], "lightflow.child");

    let dependencies = mcp_tool(
        &service,
        "lightflow.workflow.dependencies",
        serde_json::json!({ "workflow_id": "lightflow.parent" }),
    );
    assert_eq!(dependencies["complete"], true);

    let resources = mcp_result(
        &service,
        serde_json::json!({ "id": 2, "method": "resources/list" }),
    );
    let uris = resources["resources"]
        .as_array()
        .expect("resources/list returns an array")
        .iter()
        .map(|resource| resource["uri"].as_str().unwrap_or_default())
        .collect::<Vec<_>>();
    assert_eq!(uris, vec!["lightflow://workflows", "lightflow://mcp"]);

    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn repository_std_workflow_is_library_only_and_abstract() -> Result<(), Box<dyn std::error::Error>>
{
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let service = ApiService::new(root);
    let workflow = service.get_workflow("lightflow.std")?;

    assert_eq!(workflow.id, "lightflow.std");
    assert_eq!(workflow.version, "0.1.0");
    assert_eq!(workflow.name, "LightFlow Std Identity");
    assert_eq!(workflow.inputs.len(), 1);
    assert_eq!(workflow.outputs.len(), 1);
    assert!(workflow.dependencies.is_empty());
    assert!(workflow.nodes.is_empty());
    assert!(workflow.edges.is_empty());

    let crate_dir = root.join("lightflow/workflows/lightflow.std");
    assert!(crate_dir.join("src/lib.rs").exists());
    assert!(!crate_dir.join("src/main.rs").exists());

    let manifest = fs::read_to_string(crate_dir.join("Cargo.toml"))?;
    assert!(manifest.contains("name = \"lightflow-std\""));
    assert!(!manifest.contains("publish = false"));

    Ok(())
}

fn lightflow<const N: usize>(
    root: &Path,
    args: [&str; N],
) -> Result<Value, Box<dyn std::error::Error>> {
    run_json(env!("CARGO_BIN_EXE_lightflow"), root, &args)
}

fn lfw<const N: usize>(root: &Path, args: [&str; N]) -> Result<Value, Box<dyn std::error::Error>> {
    run_json(env!("CARGO_BIN_EXE_lfw"), root, &args)
}

fn run_json(binary: &str, root: &Path, args: &[&str]) -> Result<Value, Box<dyn std::error::Error>> {
    let output = Command::new(binary).args(args).current_dir(root).output()?;

    if !output.status.success() {
        return Err(format!(
            "{binary} failed with status {}\nstdout:\n{}\nstderr:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }

    Ok(serde_json::from_slice(&output.stdout)?)
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

fn write_project_specs(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    fs::create_dir_all(root.join("lightflow/workflows"))?;
    fs::write(
        root.join("Cargo.toml"),
        r#"[workspace]
resolver = "3"
members = ["lightflow/workflows/*"]

[workspace.dependencies]
lightflow = { path = "." }
"#,
    )?;
    write_workflow_crate(
        root,
        "lightflow.child",
        r#"use lightflow::workflow::*;

pub fn define() -> WorkflowSpec {
    workflow("lightflow.child")
        .version("0.1.0")
        .name("Child")
        .input("in", "json")
        .output("out", "json")
        .build()
}
"#,
    )?;
    write_workflow_crate(
        root,
        "lightflow.sink",
        r#"use lightflow::workflow::*;

pub fn define() -> WorkflowSpec {
    workflow("lightflow.sink")
        .version("0.1.0")
        .name("Sink")
        .input("in", "json")
        .build()
}
"#,
    )?;
    write_workflow_crate(
        root,
        "lightflow.parent",
        r#"use lightflow::workflow::*;

pub fn define() -> WorkflowSpec {
    workflow("lightflow.parent")
        .version("0.1.0")
        .name("Parent")
        .input("in", "json")
        .output("out", "json")
        .depends_on("lightflow.child", "0.1.0")
        .node("nested", "lightflow.child")
        .node("sink", "lightflow.sink")
        .edge("nested", "out", "sink", "in")
        .build()
}
"#,
    )?;
    Ok(())
}

fn write_workflow_crate(
    root: &Path,
    workflow_id: &str,
    source: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let crate_dir = root.join("lightflow/workflows").join(workflow_id);
    fs::create_dir_all(crate_dir.join("src"))?;
    fs::write(
        crate_dir.join("Cargo.toml"),
        format!(
            r#"[package]
name = "{}"
version = "0.1.0"
edition = "2024"
publish = false

[dependencies]
lightflow = {{ workspace = true }}
"#,
            workflow_id.replace('.', "-")
        ),
    )?;
    fs::write(crate_dir.join("src/lib.rs"), source)?;
    Ok(())
}

fn unique_temp_root() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock must be after unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("lightflow-cli-test-{}-{nanos}", std::process::id()))
}
