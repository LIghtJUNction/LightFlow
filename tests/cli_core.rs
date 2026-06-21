mod support;

use lightflow::api::ApiService;
use serde_json::Value;
use std::fs;
use support::*;

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
    assert_eq!(brief["workflows"][0]["category"], "tests");
    assert!(brief["workflows"][0].get("nodes").is_none());
    assert!(brief["workflows"][0].get("inputs").is_none());
    assert!(brief["workflows"][0].get("description").is_none());

    let categories = lfw(&root, ["list", "--categories"])?;
    assert_eq!(
        categories["categories"],
        serde_json::json!([{ "category": "tests", "workflows": 3 }])
    );
    let filtered = lfw(&root, ["list", "--category", "tests"])?;
    assert_eq!(filtered["workflows"].as_array().unwrap().len(), 3);

    let detail = lfw(&root, ["ls", "--detail"])?;
    assert_eq!(detail["workflows"][1]["id"], "lightflow.parent");
    assert_eq!(detail["workflows"][1]["category"], "tests");
    assert_eq!(detail["workflows"][1]["nodes"][0]["id"], "nested");
    assert_eq!(detail["workflows"][1]["edges"][0]["from"]["node"], "nested");

    let info = lfw(&root, ["info"])?;
    assert_eq!(info["package"]["name"], "lightflow");
    assert_eq!(info["workflows"]["total"], 3);
    assert_eq!(info["workflows"]["leaf"], 2);
    assert_eq!(info["workflows"]["composite"], 1);
    assert_eq!(
        info["workflows"]["categories"],
        serde_json::json!([{ "category": "tests", "workflows": 3 }])
    );
    assert!(
        info["executors"]
            .as_array()
            .unwrap()
            .iter()
            .any(|executor| executor["id"] == "passthrough" && executor["available"] == true)
    );
    assert!(
        info["executors"]
            .as_array()
            .unwrap()
            .iter()
            .any(|executor| {
                executor["id"] == "lightflow.command.executor.v1"
                    && executor["kind"] == "reserved"
                    && executor["available"] == false
            })
    );
    assert!(
        info["executors"]
            .as_array()
            .unwrap()
            .iter()
            .any(|executor| {
                executor["id"] == "lightflow.python.node.executor.v1"
                    && executor["capabilities"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|capability| capability == "lightflow.python.node")
            })
    );
    let arch = lfw(&root, ["arch"])?;
    assert_eq!(arch["workflows"]["total"], 3);

    let workflow_help = lfw(&root, ["help", "lightflow.parent"])?;
    assert_eq!(workflow_help["workflow"]["id"], "lightflow.parent");
    assert_eq!(workflow_help["workflow"]["kind"], "composite");
    assert_eq!(
        workflow_help["ports"]["inputs"],
        serde_json::json!([
            {
                "name": "in",
                "type": "json",
                "cli_flag": "-i in={}",
                "value_hint": "{}"
            }
        ])
    );
    assert_eq!(
        workflow_help["ports"]["outputs"][0],
        serde_json::json!({
            "name": "out",
            "type": "json",
            "value_hint": "{}"
        })
    );
    assert_eq!(workflow_help["dependencies"]["complete"], true);
    assert_eq!(
        workflow_help["usage"]["command"],
        serde_json::json!(["lfw", "run", "lightflow.parent", "-i in={}"])
    );
    assert_eq!(
        workflow_help["usage"]["inputs_json_shape"],
        serde_json::json!({ "in": {} })
    );

    let workflows_help = lfw(&root, ["workflows", "help", "lightflow.child"])?;
    assert_eq!(workflows_help["workflow"]["id"], "lightflow.child");
    assert_eq!(workflows_help["workflow"]["kind"], "leaf");

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

    let cli_tools = lfw(
        &root,
        ["mcp", r#"{"jsonrpc":"2.0","id":7,"method":"tools/list"}"#],
    )?;
    assert_eq!(cli_tools["jsonrpc"], "2.0");
    assert_eq!(cli_tools["id"], 7);
    assert_eq!(
        cli_tools["result"]["tools"][0]["name"],
        "lightflow.workflow.list"
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
fn workflow_versions_use_exact_semver_requirements() -> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    write_project_specs(&root)?;

    write_workflow_crate(
        &root,
        "lightflow.parent",
        r#"use lightflow::preload::*;

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

    let service = ApiService::new(&root);
    let execution_error = service
        .execute_workflow("lightflow.parent", Default::default())
        .expect_err("execution should reject dependency version mismatches")
        .to_string();
    assert!(execution_error.contains("lightflow.child requires version 9.9.9"));

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

#[test]
fn execution_rejects_recursive_workflow_dependency_cycles() -> Result<(), Box<dyn std::error::Error>>
{
    let root = unique_temp_root();
    fs::create_dir_all(&root)?;

    write_workflow_crate(
        &root,
        "lightflow.a",
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow("lightflow.a")
        .version("0.1.0")
        .name("A")
        .input("in", "json")
        .output("out", "json")
        .node("b", "lightflow.b")
        .build()
}
"#,
    )?;
    write_workflow_crate(
        &root,
        "lightflow.b",
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow("lightflow.b")
        .version("0.1.0")
        .name("B")
        .input("in", "json")
        .output("out", "json")
        .node("a", "lightflow.a")
        .build()
}
"#,
    )?;

    let service = ApiService::new(&root);
    let execution_error = service
        .execute_workflow("lightflow.a", Default::default())
        .expect_err("execution should reject recursive workflow cycles")
        .to_string();
    assert!(execution_error.contains("workflow dependency cycle"));

    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn runs_rm_rejects_run_id_path_traversal() -> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    let outside = root.join("outside-run-dir");
    fs::create_dir_all(&outside)?;

    let output = lfw_command(&root)
        .args(["runs", "rm", "../../outside-run-dir"])
        .output()?;

    assert!(!output.status.success());
    assert!(outside.exists());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("invalid run id path segment"));

    let _ = fs::remove_dir_all(root);
    Ok(())
}
