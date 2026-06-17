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
            "lightflow.workflow.run",
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

    let execution = mcp_tool(
        &service,
        "lightflow.workflow.run",
        serde_json::json!({
            "workflow_id": "lightflow.parent",
            "inputs": { "in": "hello" },
            "disabled_nodes": ["sink"]
        }),
    );
    assert_eq!(execution["outputs"]["out"], "hello");
    assert_eq!(execution["nodes"][1]["status"], "skipped");

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

#[test]
fn repository_text_plan_dogfoods_std_workflow() -> Result<(), Box<dyn std::error::Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let service = ApiService::new(root);

    let workflow = service.get_workflow("lightflow.text_plan")?;
    assert_eq!(
        workflow
            .dependencies
            .iter()
            .map(|dependency| (
                dependency.workflow_id.as_str(),
                dependency.version.as_deref()
            ))
            .collect::<Vec<_>>(),
        vec![
            ("lightflow.std", Some("0.1.0")),
            ("lightflow.text_prompt", Some("0.1.0")),
            ("lightflow.text_result", Some("0.1.0")),
        ]
    );
    assert!(
        workflow
            .nodes
            .iter()
            .any(|node| node.id == "identity" && node.workflow_id == "lightflow.std")
    );

    let detail = lfw(root, ["ls", "--detail"])?;
    let text_plan = detail["workflows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|workflow| workflow["id"] == "lightflow.text_plan")
        .expect("detailed list includes lightflow.text_plan");
    assert_eq!(text_plan["nodes"][0]["workflow_id"], "lightflow.std");

    let deps = lfw(root, ["deps", "lightflow.text_plan"])?;
    assert_eq!(deps["complete"], true);
    assert_eq!(
        deps["workflows"],
        serde_json::json!([
            "lightflow.std",
            "lightflow.text_plan",
            "lightflow.text_prompt",
            "lightflow.text_result"
        ])
    );
    assert_eq!(
        deps["workflow_order"],
        serde_json::json!([
            "lightflow.std",
            "lightflow.text_prompt",
            "lightflow.text_result",
            "lightflow.text_plan"
        ])
    );

    Ok(())
}

#[test]
fn lfwx_runs_workflow_and_temporarily_toggles_nodes() -> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    write_project_specs(&root)?;

    let run = lfwx(&root, ["lightflow.parent", "--input", "in=hello"])?;
    assert_eq!(run["workflow_id"], "lightflow.parent");
    assert_eq!(run["inputs"]["in"], "hello");
    assert_eq!(run["outputs"]["out"], "hello");
    assert_eq!(run["nodes"][0]["node_id"], "nested");
    assert_eq!(run["nodes"][0]["status"], "completed");
    assert_eq!(run["nodes"][1]["node_id"], "sink");
    assert_eq!(run["nodes"][1]["status"], "completed");

    let disabled = lfwx(
        &root,
        [
            "lightflow.parent",
            "--input",
            "in=hello",
            "--disable",
            "nested",
        ],
    )?;
    assert_eq!(disabled["nodes"][0]["node_id"], "nested");
    assert_eq!(disabled["nodes"][0]["status"], "skipped");
    assert_eq!(disabled["outputs"]["out"], Value::Null);

    let enabled = lfw(
        &root,
        [
            "run",
            "lightflow.parent",
            "--input",
            "in=hello",
            "--disable",
            "nested",
            "--enable",
            "nested",
        ],
    )?;
    assert_eq!(enabled["nodes"][0]["status"], "completed");
    assert_eq!(enabled["outputs"]["out"], "hello");

    write_workflow_crate(
        &root,
        "lightflow.parent",
        r#"use lightflow::workflow::*;

pub fn define() -> WorkflowSpec {
    workflow("lightflow.parent")
        .version("0.1.0")
        .name("Parent")
        .input("in", "json")
        .output("out", "json")
        .depends_on("lightflow.child", "0.1.0")
        .disabled_node("nested", "lightflow.child")
        .node("sink", "lightflow.sink")
        .edge("nested", "out", "sink", "in")
        .build()
}
"#,
    )?;
    let default_disabled = lfwx(&root, ["lightflow.parent", "--input", "in=hello"])?;
    assert_eq!(default_disabled["nodes"][0]["status"], "skipped");
    assert_eq!(default_disabled["outputs"]["out"], Value::Null);

    let enabled_from_source = lfwx(
        &root,
        [
            "lightflow.parent",
            "--input",
            "in=hello",
            "--enable",
            "nested",
        ],
    )?;
    assert_eq!(enabled_from_source["nodes"][0]["status"], "completed");
    assert_eq!(enabled_from_source["outputs"]["out"], "hello");

    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn cargo_path_dependency_installs_workflow_for_dependency_resolution()
-> Result<(), Box<dyn std::error::Error>> {
    let base = unique_temp_root();
    let project = base.join("project");
    let std_dep = base.join("lightflow-std");
    fs::create_dir_all(&project)?;
    write_external_std_crate(&std_dep)?;

    fs::write(
        project.join("Cargo.toml"),
        format!(
            r#"[workspace]
resolver = "3"
members = ["lightflow/workflows/*"]

[workspace.dependencies]
lightflow = {{ path = {:?} }}
"#,
            env!("CARGO_MANIFEST_DIR")
        ),
    )?;
    let added_dep = lfw(
        &project,
        ["add-dep", "lightflow-std", "--path", "../lightflow-std"],
    )?;
    assert_eq!(added_dep["dependency"], "lightflow-std");
    assert_eq!(added_dep["source"]["path"], "../lightflow-std");
    let manifest = fs::read_to_string(project.join("Cargo.toml"))?;
    assert!(manifest.contains("lightflow-std = { path = \"../lightflow-std\" }"));

    write_workflow_crate(
        &project,
        "lightflow.image_prompt",
        r#"use lightflow::workflow::*;

pub fn define() -> WorkflowSpec {
    workflow("lightflow.image_prompt")
        .version("0.1.0")
        .name("Image Prompt")
        .input("positive", "text")
        .input("negative", "text")
        .output("prompt", "json")
        .depends_on("lightflow.std", "0.1.0")
        .hf_model(
            "image_model",
            "flux2-safetensors",
            "text-to-image",
            "safetensors",
            "black-forest-labs/FLUX.2-dev",
            "flux2-dev.safetensors"
        )
        .hf_model(
            "image_model",
            "flux2-gguf",
            "text-to-image",
            "gguf",
            "city96/FLUX.2-dev-gguf",
            "flux2-dev-q4.gguf"
        )
        .node("passthrough", "lightflow.std")
        .build()
}
"#,
    )?;

    let list = lfw(&project, ["list"])?;
    let ids = list["workflows"]
        .as_array()
        .expect("workflows list returns an array")
        .iter()
        .map(|workflow| workflow["id"].as_str().unwrap_or_default())
        .collect::<Vec<_>>();
    assert_eq!(ids, vec!["lightflow.image_prompt", "lightflow.std"]);

    let deps = lfw(&project, ["deps", "lightflow.image_prompt"])?;
    assert_eq!(deps["complete"], true);
    assert_eq!(
        deps["workflows"],
        serde_json::json!(["lightflow.image_prompt", "lightflow.std"])
    );
    assert_eq!(
        deps["workflow_order"],
        serde_json::json!(["lightflow.std", "lightflow.image_prompt"])
    );

    let sync = lfw(&project, ["sync", "lightflow.image_prompt", "--dry-run"])?;
    assert_eq!(sync["dry_run"], true);
    assert_eq!(sync["hf_downloads"], serde_json::json!([]));
    assert_eq!(sync["unresolved_models"][0]["id"], "image_model");
    assert_eq!(
        sync["unresolved_models"][0]["variants"][0]["id"],
        "flux2-safetensors"
    );
    assert_eq!(
        sync["unresolved_models"][0]["variants"][1]["id"],
        "flux2-gguf"
    );

    let selected = lfw(
        &project,
        [
            "sync",
            "lightflow.image_prompt",
            "--model",
            "image_model=flux2-gguf",
        ],
    )?;
    assert_eq!(selected["unresolved_models"], serde_json::json!([]));
    assert_eq!(selected["hf_downloads"][0]["format"], "gguf");
    assert_eq!(
        selected["hf_downloads"][0]["command"],
        serde_json::json!([
            "hf",
            "download",
            "city96/FLUX.2-dev-gguf",
            "flux2-dev-q4.gguf"
        ])
    );

    let _ = fs::remove_dir_all(base);
    Ok(())
}

#[test]
fn add_dep_writes_git_workflow_dependency() -> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    fs::create_dir_all(&root)?;
    lfw(&root, ["init"])?;

    let output = lfw(
        &root,
        [
            "add-dep",
            "lightflow-std",
            "--git",
            "https://github.com/lightjunction/LightFlow",
            "--package",
            "lightflow-std",
        ],
    )?;
    assert_eq!(output["dependency"], "lightflow-std");
    assert_eq!(
        output["source"]["git"],
        "https://github.com/lightjunction/LightFlow"
    );
    assert_eq!(output["package"], "lightflow-std");

    let manifest = fs::read_to_string(root.join("Cargo.toml"))?;
    assert!(manifest.contains(
        "lightflow-std = { git = \"https://github.com/lightjunction/LightFlow\", package = \"lightflow-std\" }"
    ));

    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn workflow_versions_use_exact_semver_requirements() -> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    write_project_specs(&root)?;

    write_workflow_crate(
        &root,
        "lightflow.parent",
        r#"use lightflow::workflow::*;

pub fn define() -> WorkflowSpec {
    workflow("lightflow.parent")
        .version("0.1.0")
        .name("Parent")
        .input("in", "json")
        .output("out", "json")
        .depends_on("lightflow.child", "9.9.9")
        .node("nested", "lightflow.child")
        .build()
}
"#,
    )?;

    let deps = lfw(&root, ["deps", "lightflow.parent"])?;
    assert_eq!(deps["complete"], false);
    assert_eq!(
        deps["version_mismatches"][0]["workflow_id"],
        "lightflow.child"
    );
    assert_eq!(deps["version_mismatches"][0]["required"], "9.9.9");
    assert_eq!(deps["version_mismatches"][0]["found"], "0.1.0");
    assert_eq!(
        deps["version_mismatches"][0]["required_by"],
        "lightflow.parent"
    );

    let validation = lightflow(
        &root,
        [
            "workflows",
            "validate",
            r#"{
              "id": "lightflow.invalid_version",
              "version": "not-semver",
              "name": "Invalid Version",
              "nodes": [],
              "edges": []
            }"#,
        ],
    )?;
    assert_eq!(validation["valid"], false);
    assert!(
        validation["issues"][0]
            .as_str()
            .unwrap()
            .contains("must be semantic version")
    );

    let _ = fs::remove_dir_all(root);
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

fn lfwx<const N: usize>(root: &Path, args: [&str; N]) -> Result<Value, Box<dyn std::error::Error>> {
    run_json(env!("CARGO_BIN_EXE_lfwx"), root, &args)
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

fn write_external_std_crate(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    fs::create_dir_all(root.join("src"))?;
    fs::write(
        root.join("Cargo.toml"),
        format!(
            r#"[package]
name = "lightflow-std"
version = "0.1.0"
edition = "2024"

[dependencies]
lightflow = {{ path = {:?} }}
"#,
            env!("CARGO_MANIFEST_DIR")
        ),
    )?;
    fs::write(
        root.join("src/lib.rs"),
        r#"use lightflow::workflow::*;

pub fn define() -> WorkflowSpec {
    workflow("lightflow.std")
        .version("0.1.0")
        .name("LightFlow Std Identity")
        .input("value", "json")
        .output("value", "json")
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
