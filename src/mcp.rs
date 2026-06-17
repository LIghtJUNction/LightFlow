//! Minimal MCP surface for external editors and agents.

use crate::api::{ApiError, ApiService};
use crate::component::ComponentSpec;
use crate::workflow::WorkflowSpec;
use serde_json::{Value, json};

const PROTOCOL_VERSION: &str = "2024-11-05";

/// Handle one JSON-RPC request for the `/mcp` endpoint.
#[must_use]
pub fn handle_request(service: &ApiService, request: Value) -> Value {
    let id = request.get("id").cloned().unwrap_or(Value::Null);
    let Some(method) = request.get("method").and_then(Value::as_str) else {
        return error(id, -32600, "invalid JSON-RPC request");
    };
    let params = request.get("params").cloned().unwrap_or(Value::Null);

    let result = match method {
        "initialize" => Ok(initialize_result()),
        "tools/list" => Ok(json!({ "tools": tools() })),
        "tools/call" => call_tool(service, &params),
        "resources/list" => Ok(json!({ "resources": resources() })),
        "resources/read" => read_resource(service, &params),
        "ping" => Ok(json!({})),
        _ => Err(McpError::new(
            -32601,
            format!("unknown MCP method: {method}"),
        )),
    };

    match result {
        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        Err(error_value) => error(id, error_value.code, &error_value.message),
    }
}

fn initialize_result() -> Value {
    json!({
        "protocolVersion": PROTOCOL_VERSION,
        "serverInfo": {
            "name": "lightflow",
            "version": env!("CARGO_PKG_VERSION")
        },
        "capabilities": {
            "tools": { "listChanged": false },
            "resources": { "subscribe": false, "listChanged": false }
        }
    })
}

fn tools() -> Value {
    json!([
        tool(
            "lightflow.component.list",
            "List LightFlow components.",
            json!({ "type": "object", "properties": {} })
        ),
        tool(
            "lightflow.component.get",
            "Read one LightFlow component.",
            id_schema("component_id")
        ),
        tool(
            "lightflow.component.save",
            "Save one LightFlow component.",
            component_schema()
        ),
        tool(
            "lightflow.workflow.list",
            "List LightFlow workflows.",
            json!({ "type": "object", "properties": {} })
        ),
        tool(
            "lightflow.workflow.get",
            "Read one LightFlow workflow.",
            id_schema("workflow_id")
        ),
        tool(
            "lightflow.workflow.validate",
            "Validate one LightFlow workflow.",
            workflow_schema()
        ),
        tool(
            "lightflow.workflow.save",
            "Save one LightFlow workflow.",
            workflow_schema()
        )
    ])
}

fn tool(name: &str, description: &str, input_schema: Value) -> Value {
    json!({
        "name": name,
        "description": description,
        "inputSchema": input_schema
    })
}

fn id_schema(id_name: &str) -> Value {
    json!({
        "type": "object",
        "required": [id_name],
        "properties": {
            id_name: { "type": "string" }
        }
    })
}

fn component_schema() -> Value {
    json!({
        "type": "object",
        "required": ["component"],
        "properties": {
            "component": { "type": "object", "additionalProperties": true }
        }
    })
}

fn workflow_schema() -> Value {
    json!({
        "type": "object",
        "required": ["workflow"],
        "properties": {
            "workflow": { "type": "object", "additionalProperties": true }
        }
    })
}

fn resources() -> Value {
    json!([
        resource("lightflow://components", "Components", "application/json"),
        resource("lightflow://workflows", "Workflows", "application/json"),
        resource("lightflow://mcp", "MCP Endpoint", "application/json")
    ])
}

fn resource(uri: &str, name: &str, mime_type: &str) -> Value {
    json!({
        "uri": uri,
        "name": name,
        "mimeType": mime_type
    })
}

fn call_tool(service: &ApiService, params: &Value) -> Result<Value, McpError> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| McpError::new(-32602, "tools/call requires params.name"))?;
    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let value = match name {
        "lightflow.component.list" => serde_json::to_value(service.list_components()?)?,
        "lightflow.component.get" => {
            serde_json::to_value(service.get_component(required_str(&arguments, "component_id")?)?)?
        }
        "lightflow.component.save" => {
            serde_json::to_value(service.save_component(component_arg(&arguments)?)?)?
        }
        "lightflow.workflow.list" => serde_json::to_value(service.list_workflows()?)?,
        "lightflow.workflow.get" => {
            serde_json::to_value(service.get_workflow(required_str(&arguments, "workflow_id")?)?)?
        }
        "lightflow.workflow.validate" => {
            serde_json::to_value(service.validate_workflow(&workflow_arg(&arguments)?))?
        }
        "lightflow.workflow.save" => {
            serde_json::to_value(service.save_workflow(workflow_arg(&arguments)?)?)?
        }
        _ => return Err(McpError::new(-32602, format!("unknown tool: {name}"))),
    };

    Ok(json!({
        "content": [
            {
                "type": "text",
                "text": serde_json::to_string_pretty(&value)?
            }
        ],
        "structuredContent": value
    }))
}

fn read_resource(service: &ApiService, params: &Value) -> Result<Value, McpError> {
    let uri = params
        .get("uri")
        .and_then(Value::as_str)
        .ok_or_else(|| McpError::new(-32602, "resources/read requires params.uri"))?;
    let value = match uri {
        "lightflow://components" => serde_json::to_value(service.list_components()?)?,
        "lightflow://workflows" => serde_json::to_value(service.list_workflows()?)?,
        "lightflow://mcp" => mcp_resource(),
        _ => return Err(McpError::new(-32602, format!("unknown resource: {uri}"))),
    };

    Ok(json!({
        "contents": [
            {
                "uri": uri,
                "mimeType": "application/json",
                "text": serde_json::to_string_pretty(&value)?
            }
        ]
    }))
}

fn component_arg(arguments: &Value) -> Result<ComponentSpec, McpError> {
    let component = arguments
        .get("component")
        .ok_or_else(|| McpError::new(-32602, "missing object argument: component"))?;
    serde_json::from_value(component.clone()).map_err(McpError::from)
}

fn workflow_arg(arguments: &Value) -> Result<WorkflowSpec, McpError> {
    let workflow = arguments
        .get("workflow")
        .ok_or_else(|| McpError::new(-32602, "missing object argument: workflow"))?;
    serde_json::from_value(workflow.clone()).map_err(McpError::from)
}

fn required_str<'a>(value: &'a Value, key: &str) -> Result<&'a str, McpError> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| McpError::new(-32602, format!("missing string argument: {key}")))
}

fn error(id: Value, code: i64, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": code,
            "message": message
        }
    })
}

#[derive(Debug)]
struct McpError {
    code: i64,
    message: String,
}

impl McpError {
    fn new(code: i64, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl From<ApiError> for McpError {
    fn from(error: ApiError) -> Self {
        Self::new(-32000, error.to_string())
    }
}

impl From<serde_json::Error> for McpError {
    fn from(error: serde_json::Error) -> Self {
        Self::new(-32603, error.to_string())
    }
}

fn mcp_resource() -> Value {
    json!({
        "endpoint": "http://127.0.0.1:5174/mcp",
        "transport": "http",
        "jsonrpc": "2.0",
        "methods": [
            "initialize",
            "ping",
            "tools/list",
            "tools/call",
            "resources/list",
            "resources/read"
        ],
        "tools": [
            "lightflow.component.list",
            "lightflow.component.get",
            "lightflow.component.save",
            "lightflow.workflow.list",
            "lightflow.workflow.get",
            "lightflow.workflow.validate",
            "lightflow.workflow.save"
        ],
        "resources": [
            "lightflow://components",
            "lightflow://workflows",
            "lightflow://mcp"
        ]
    })
}
