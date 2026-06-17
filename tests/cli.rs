use lightflow::api::ApiService;
use lightflow::mcp;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn cli_lists_components_and_validates_nested_workflows() -> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    write_project_specs(&root)?;

    let components = lightflow(&root, ["components", "list"])?;
    assert_eq!(
        components["components"][0]["id"],
        Value::String("component.echo".to_owned())
    );

    let child = lightflow(&root, ["workflows", "get", "workflow.child"])?;
    assert_eq!(child["id"], Value::String("workflow.child".to_owned()));

    let parent = fs::read_to_string(root.join("lightflow/workflows/workflow.parent.json"))?;
    let validation = lightflow(&root, ["workflows", "validate", &parent])?;
    assert_eq!(validation["valid"], Value::Bool(true));
    assert_eq!(
        validation["topological_order"],
        serde_json::json!(["nested", "output"])
    );

    let invalid = serde_json::json!({
        "id": "workflow.invalid",
        "name": "Invalid",
        "nodes": [
            {
                "id": "missing",
                "uses": "component",
                "component_id": "component.missing"
            }
        ],
        "edges": []
    });
    let invalid = lightflow(&root, ["workflows", "validate", &invalid.to_string()])?;
    assert_eq!(invalid["valid"], Value::Bool(false));
    assert!(
        invalid["issues"][0]
            .as_str()
            .unwrap()
            .contains("missing component")
    );

    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn mcp_exposes_component_and_workflow_tools() -> Result<(), Box<dyn std::error::Error>> {
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
    for required in [
        "lightflow.component.list",
        "lightflow.component.get",
        "lightflow.component.save",
        "lightflow.workflow.list",
        "lightflow.workflow.get",
        "lightflow.workflow.validate",
        "lightflow.workflow.save",
    ] {
        assert!(
            tool_names.contains(&required),
            "missing MCP tool {required}"
        );
    }

    let workflow = mcp_tool(
        &service,
        "lightflow.workflow.get",
        serde_json::json!({ "workflow_id": "workflow.parent" }),
    );
    assert_eq!(workflow["id"], "workflow.parent");
    assert_eq!(workflow["nodes"][0]["uses"], "workflow");

    let validation = mcp_tool(
        &service,
        "lightflow.workflow.validate",
        serde_json::json!({ "workflow": workflow }),
    );
    assert_eq!(validation["valid"], true);

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
    assert_eq!(
        uris,
        vec![
            "lightflow://components",
            "lightflow://workflows",
            "lightflow://mcp"
        ]
    );

    let _ = fs::remove_dir_all(root);
    Ok(())
}

fn lightflow<const N: usize>(
    root: &Path,
    args: [&str; N],
) -> Result<Value, Box<dyn std::error::Error>> {
    let output = Command::new(env!("CARGO_BIN_EXE_lightflow"))
        .args(args)
        .current_dir(root)
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
    fs::create_dir_all(root.join("lightflow/components"))?;
    fs::create_dir_all(root.join("lightflow/workflows"))?;
    fs::write(
        root.join("lightflow/components/component.echo.json"),
        r#"{
  "id": "component.echo",
  "name": "Echo",
  "inputs": [{ "name": "in", "type": "json" }],
  "outputs": [{ "name": "out", "type": "json" }]
}
"#,
    )?;
    fs::write(
        root.join("lightflow/workflows/workflow.child.json"),
        r#"{
  "id": "workflow.child",
  "name": "Child",
  "inputs": [{ "name": "in", "type": "json" }],
  "outputs": [{ "name": "out", "type": "json" }],
  "nodes": [
    {
      "id": "echo",
      "uses": "component",
      "component_id": "component.echo"
    }
  ],
  "edges": []
}
"#,
    )?;
    fs::write(
        root.join("lightflow/workflows/workflow.parent.json"),
        r#"{
  "id": "workflow.parent",
  "name": "Parent",
  "inputs": [{ "name": "in", "type": "json" }],
  "outputs": [{ "name": "out", "type": "json" }],
  "nodes": [
    {
      "id": "nested",
      "uses": "workflow",
      "workflow_id": "workflow.child"
    },
    {
      "id": "output",
      "uses": "component",
      "component_id": "component.output"
    }
  ],
  "edges": [
    {
      "from": { "node": "nested", "port": "out" },
      "to": { "node": "output", "port": "value" }
    }
  ]
}
"#,
    )?;
    Ok(())
}

fn unique_temp_root() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock must be after unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("lightflow-cli-test-{}-{nanos}", std::process::id()))
}
