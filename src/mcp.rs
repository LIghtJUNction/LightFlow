//! Minimal HTTP-friendly MCP surface for external agents.
//!
//! LightFlow exposes tools and resources over JSON-RPC. It does not embed an
//! agent loop, planner, model client, or policy engine here; any MCP-capable
//! agent remains an external client.

use crate::api::{
    ApiError, ApiService, RuntimeEndpoint, RuntimePort, RuntimePosition, RuntimeRunRequest,
    RuntimeWorkflow, RuntimeWorkflowEdge, RuntimeWorkflowNode,
};
use crate::stream;
use serde::Deserialize;
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
            "lightflow.list_workflows",
            "List LightFlow runtime workflow DAGs.",
            json!({ "type": "object", "properties": {} })
        ),
        tool(
            "lightflow.workflow.list",
            "List LightFlow workflow DAGs for UI clients.",
            json!({ "type": "object", "properties": {} })
        ),
        tool(
            "lightflow.get_workflow",
            "Read one LightFlow runtime workflow DAG.",
            workflow_id_schema()
        ),
        tool(
            "lightflow.workflow.open",
            "Open a LightFlow workflow for UI clients.",
            workflow_open_schema()
        ),
        tool(
            "lightflow.workflow.read_region",
            "Read a viewport region from a LightFlow workflow.",
            workflow_region_schema()
        ),
        tool(
            "lightflow.validate_workflow",
            "Validate a LightFlow runtime workflow DAG.",
            workflow_schema()
        ),
        tool(
            "lightflow.workflow.validate",
            "Validate a LightFlow workflow plus a local UI patch.",
            workflow_validate_schema()
        ),
        tool(
            "lightflow.save_workflow",
            "Save a LightFlow runtime workflow DAG.",
            workflow_schema()
        ),
        tool(
            "lightflow.workflow.apply_patch",
            "Apply and persist a LightFlow UI workflow patch.",
            workflow_patch_schema()
        ),
        tool(
            "lightflow.preview_run",
            "Preview a workflow run without writing state or invoking an agent.",
            create_run_schema()
        ),
        tool(
            "lightflow.create_run",
            "Create a run manifest under XDG state without invoking an agent.",
            create_run_schema()
        ),
        tool(
            "lightflow.list_runs",
            "List stored run manifests.",
            json!({ "type": "object", "properties": {} })
        ),
        tool(
            "lightflow.get_run",
            "Read one run manifest.",
            run_id_schema()
        ),
        tool(
            "lightflow.run_status",
            "Read a derived run status summary.",
            run_id_schema()
        ),
        tool(
            "lightflow.cancel_run",
            "Cancel a run record.",
            run_id_schema()
        ),
        tool(
            "lightflow.run_events",
            "Read the run event JSONL stream.",
            run_id_schema()
        ),
        tool(
            "lightflow.run_trace",
            "Read the run trace JSONL stream.",
            run_id_schema()
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

fn create_run_schema() -> Value {
    json!({
        "type": "object",
        "required": ["workflow_id"],
        "properties": {
            "workflow_id": { "type": "string" },
            "run_id": { "type": "string" },
            "inputs": { "type": "object", "additionalProperties": true }
        }
    })
}

fn workflow_id_schema() -> Value {
    json!({
        "type": "object",
        "required": ["workflow_id"],
        "properties": {
            "workflow_id": { "type": "string" }
        }
    })
}

fn workflow_open_schema() -> Value {
    json!({
        "type": "object",
        "required": ["workflow_id"],
        "properties": {
            "workflow_id": { "type": "string" },
            "mode": { "type": "string" }
        }
    })
}

fn workflow_region_schema() -> Value {
    json!({
        "type": "object",
        "required": ["workflow_id", "region"],
        "properties": {
            "workflow_id": { "type": "string" },
            "region": { "type": "object", "additionalProperties": true }
        }
    })
}

fn workflow_validate_schema() -> Value {
    json!({
        "type": "object",
        "required": ["workflow_id"],
        "properties": {
            "workflow_id": { "type": "string" },
            "base_revision": { "type": "string" },
            "visible_region": { "type": "object", "additionalProperties": true },
            "local_patch": { "type": "object", "additionalProperties": true },
            "workflow": {
                "type": "object",
                "required": ["id", "name", "nodes", "edges"],
                "additionalProperties": true
            }
        }
    })
}

fn workflow_patch_schema() -> Value {
    json!({
        "type": "object",
        "required": ["patch"],
        "properties": {
            "patch": {
                "type": "object",
                "required": ["workflow_id", "ops"],
                "additionalProperties": true
            }
        }
    })
}

fn workflow_schema() -> Value {
    json!({
        "type": "object",
        "required": ["workflow"],
        "properties": {
            "workflow": {
                "type": "object",
                "required": ["id", "name", "nodes", "edges"],
                "additionalProperties": true
            }
        }
    })
}

fn run_id_schema() -> Value {
    json!({
        "type": "object",
        "required": ["run_id"],
        "properties": {
            "run_id": { "type": "string" }
        }
    })
}

fn resources() -> Value {
    json!([
        resource("lightflow://workflows", "Workflow DAGs", "application/json"),
        resource("lightflow://nodes", "Node Assets", "application/json"),
        resource("lightflow://runtime", "Runtime Streams", "application/json"),
        resource("lightflow://ctx-abi", "Ctx ABI", "application/json"),
        resource("lightflow://runs", "Run Manifests", "application/json"),
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
        "lightflow.list_workflows" => serde_json::to_value(service.list_runtime_workflows()?)?,
        "lightflow.workflow.list" => serde_json::to_value(service.list_runtime_workflows()?)?,
        "lightflow.get_workflow" => serde_json::to_value(
            service.get_runtime_workflow(required_str(&arguments, "workflow_id")?)?,
        )?,
        "lightflow.workflow.open" => ui_open_workflow(service, &arguments)?,
        "lightflow.workflow.read_region" => ui_read_region(service, &arguments)?,
        "lightflow.validate_workflow" => {
            serde_json::to_value(service.validate_runtime_workflow(&workflow_arg(&arguments)?))?
        }
        "lightflow.workflow.validate" => ui_validate_workflow(service, &arguments)?,
        "lightflow.save_workflow" => {
            serde_json::to_value(service.save_runtime_workflow(workflow_arg(&arguments)?)?)?
        }
        "lightflow.workflow.apply_patch" => ui_apply_patch(service, &arguments)?,
        "lightflow.preview_run" => {
            serde_json::to_value(service.preview_runtime_run(runtime_run_request(arguments)?)?)?
        }
        "lightflow.create_run" => {
            serde_json::to_value(service.create_runtime_run(runtime_run_request(arguments)?)?)?
        }
        "lightflow.list_runs" => serde_json::to_value(service.list_runs()?)?,
        "lightflow.get_run" => {
            serde_json::to_value(service.get_run(required_str(&arguments, "run_id")?)?)?
        }
        "lightflow.run_status" => {
            serde_json::to_value(service.run_status(required_str(&arguments, "run_id")?)?)?
        }
        "lightflow.cancel_run" => {
            serde_json::to_value(service.cancel_run(required_str(&arguments, "run_id")?)?)?
        }
        "lightflow.run_events" => {
            json!({ "text": service.run_events(required_str(&arguments, "run_id")?)? })
        }
        "lightflow.run_trace" => {
            json!({ "text": service.run_trace(required_str(&arguments, "run_id")?)? })
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

fn ui_open_workflow(service: &ApiService, arguments: &Value) -> Result<Value, McpError> {
    let workflow_id = required_str(arguments, "workflow_id")?;
    let workflow = service.get_runtime_workflow(workflow_id)?;
    let revision = workflow_revision(&workflow);
    Ok(json!({
        "workflow_id": workflow.id.clone(),
        "revision": revision,
        "workflow": workflow,
        "mode": arguments.get("mode").and_then(Value::as_str).unwrap_or("full")
    }))
}

fn ui_read_region(service: &ApiService, arguments: &Value) -> Result<Value, McpError> {
    let workflow_id = required_str(arguments, "workflow_id")?;
    let workflow = service.get_runtime_workflow(workflow_id)?;
    Ok(workflow_region_response(&workflow))
}

fn ui_validate_workflow(service: &ApiService, arguments: &Value) -> Result<Value, McpError> {
    let workflow = workflow_from_ui_arguments(service, arguments)?;
    serde_json::to_value(service.validate_runtime_workflow(&workflow)).map_err(McpError::from)
}

fn ui_apply_patch(service: &ApiService, arguments: &Value) -> Result<Value, McpError> {
    let patch = ui_patch_arg(arguments)?;
    let workflow = workflow_with_patch(service, &patch)?;
    let validation = service.validate_runtime_workflow(&workflow);
    if !validation.valid {
        return Err(McpError::new(-32602, validation.issues.join("; ")));
    }
    if workflow.id == "workflow.default" && !patch.ops.is_empty() {
        return Err(McpError::new(
            -32602,
            "workflow.default is built in and cannot be overwritten",
        ));
    }
    if patch.ops.is_empty() {
        let revision = workflow_revision(&workflow);
        return Ok(json!({
            "workflow": workflow,
            "revision": revision,
            "base_revision": revision,
            "validation": validation
        }));
    }

    let saved = service.save_runtime_workflow(workflow)?;
    let revision = workflow_revision(&saved.workflow);
    Ok(json!({
        "workflow": saved.workflow,
        "path": saved.path,
        "revision": revision,
        "base_revision": revision,
        "validation": validation
    }))
}

fn workflow_from_ui_arguments(
    service: &ApiService,
    arguments: &Value,
) -> Result<RuntimeWorkflow, McpError> {
    if arguments.get("workflow").is_some() {
        return workflow_arg(arguments);
    }
    if let Some(patch) = arguments.get("local_patch") {
        let patch = serde_json::from_value::<UiWorkflowPatch>(patch.clone())?;
        return workflow_with_patch(service, &patch);
    }
    service
        .get_runtime_workflow(required_str(arguments, "workflow_id")?)
        .map_err(McpError::from)
}

fn workflow_with_patch(
    service: &ApiService,
    patch: &UiWorkflowPatch,
) -> Result<RuntimeWorkflow, McpError> {
    let mut workflow = service.get_runtime_workflow(&patch.workflow_id)?;
    for op in &patch.ops {
        apply_ui_patch_op(&mut workflow, op);
    }
    Ok(workflow)
}

fn apply_ui_patch_op(workflow: &mut RuntimeWorkflow, op: &UiWorkflowPatchOp) {
    match op {
        UiWorkflowPatchOp::AddNode { node } => {
            let runtime_node = runtime_node_from_ui(node, None);
            if workflow
                .nodes
                .iter()
                .all(|existing| existing.id != runtime_node.id)
            {
                workflow.nodes.push(runtime_node);
            }
        }
        UiWorkflowPatchOp::UpdateNode { node } => {
            if let Some(index) = workflow
                .nodes
                .iter()
                .position(|existing| existing.id == node.id)
            {
                let existing = workflow.nodes[index].clone();
                workflow.nodes[index] = runtime_node_from_ui(node, Some(&existing));
            }
        }
        UiWorkflowPatchOp::MoveNode { node_id, position } => {
            if let Some(node) = workflow.nodes.iter_mut().find(|node| node.id == *node_id) {
                node.position = (*position).into();
            }
        }
        UiWorkflowPatchOp::DeleteNode { node_id } => {
            workflow.nodes.retain(|node| node.id != *node_id);
            workflow
                .edges
                .retain(|edge| edge.from.node != *node_id && edge.to.node != *node_id);
        }
        UiWorkflowPatchOp::Connect { from, to } => {
            if workflow
                .edges
                .iter()
                .all(|edge| edge.from != *from || edge.to != *to)
            {
                workflow.edges.push(RuntimeWorkflowEdge {
                    from: from.clone(),
                    to: to.clone(),
                });
            }
        }
        UiWorkflowPatchOp::Disconnect { from, to } => {
            workflow
                .edges
                .retain(|edge| edge.from != *from || edge.to != *to);
        }
    }
}

fn runtime_node_from_ui(
    node: &UiWorkflowNode,
    existing: Option<&RuntimeWorkflowNode>,
) -> RuntimeWorkflowNode {
    let (inputs, outputs) = existing
        .filter(|existing| existing.kind == node.kind)
        .map(|existing| (existing.inputs.clone(), existing.outputs.clone()))
        .unwrap_or_else(|| inferred_ports(&node.kind));
    RuntimeWorkflowNode {
        id: node.id.clone(),
        kind: node.kind.clone(),
        title: Some(node.title.clone()),
        position: node.position.into(),
        component: node.component.clone(),
        inputs,
        outputs,
    }
}

fn inferred_ports(kind: &str) -> (Vec<RuntimePort>, Vec<RuntimePort>) {
    match kind {
        "workflow_input" | "input" => (Vec::new(), vec![runtime_port("workflow", "flow")]),
        "state_transform" | "transform" => (
            vec![runtime_port("tool_result", "json")],
            vec![runtime_port("json", "json")],
        ),
        "run_state" | "state" => (
            vec![runtime_port("json", "json")],
            vec![runtime_port("state", "event")],
        ),
        "preview_sink" | "preview" => (vec![runtime_port("state", "event")], Vec::new()),
        "web_component_slot" | "web_component" => (
            vec![
                runtime_port("component", "component"),
                runtime_port("json", "json"),
            ],
            vec![runtime_port("custom_ui", "preview")],
        ),
        "output" => (vec![runtime_port("tool_result", "json")], Vec::new()),
        _ => (
            vec![runtime_port("workflow", "flow")],
            vec![runtime_port("tool_result", "json")],
        ),
    }
}

fn runtime_port(name: &str, ty: &str) -> RuntimePort {
    RuntimePort {
        name: name.to_owned(),
        ty: ty.to_owned(),
    }
}

fn workflow_region_response(workflow: &RuntimeWorkflow) -> Value {
    json!({
        "workflow_id": workflow.id.clone(),
        "revision": workflow_revision(workflow),
        "nodes": workflow.nodes.iter().map(workflow_region_node).collect::<Vec<_>>(),
        "edges": workflow.edges.iter().enumerate().map(|(index, edge)| {
            json!({
                "id": format!("edge-{}", index + 1),
                "from": edge.from.clone(),
                "to": edge.to.clone()
            })
        }).collect::<Vec<_>>(),
        "total_estimate": workflow.nodes.len(),
        "next_cursor": null
    })
}

fn workflow_region_node(node: &RuntimeWorkflowNode) -> Value {
    json!({
        "id": node.id.clone(),
        "kind": node.kind.clone(),
        "title": node.title.clone().unwrap_or_else(|| node_kind_title(&node.kind).to_owned()),
        "position": {
            "x": node.position.x,
            "y": node.position.y
        },
        "component": node.component.clone()
    })
}

fn node_kind_title(kind: &str) -> &'static str {
    match kind {
        "workflow_input" | "input" => "Workflow Input",
        "mcp_tool" | "tool" => "MCP Tool",
        "state_transform" | "transform" => "State Transform",
        "run_state" | "state" => "Run State",
        "preview_sink" | "preview" => "Preview Sink",
        "web_component_slot" | "web_component" => "Web Component Slot",
        "output" => "Output",
        _ => "MCP Tool",
    }
}

fn workflow_revision(workflow: &RuntimeWorkflow) -> String {
    let text = serde_json::to_string(workflow).unwrap_or_default();
    let checksum = text.bytes().fold(0xcbf29ce484222325_u64, |hash, byte| {
        hash.wrapping_mul(0x100000001b3) ^ u64::from(byte)
    });
    format!("rev-{checksum:016x}")
}

fn ui_patch_arg(arguments: &Value) -> Result<UiWorkflowPatch, McpError> {
    let patch = arguments
        .get("patch")
        .ok_or_else(|| McpError::new(-32602, "missing object argument: patch"))?;
    serde_json::from_value(patch.clone()).map_err(McpError::from)
}

fn read_resource(service: &ApiService, params: &Value) -> Result<Value, McpError> {
    let uri = params
        .get("uri")
        .and_then(Value::as_str)
        .ok_or_else(|| McpError::new(-32602, "resources/read requires params.uri"))?;
    let value = match uri {
        "lightflow://workflows" => serde_json::to_value(service.list_runtime_workflows()?)?,
        "lightflow://nodes" => serde_json::to_value(service.list_nodes()?)?,
        "lightflow://runtime" => serde_json::to_value(stream::stream_info())?,
        "lightflow://ctx-abi" => serde_json::to_value(service.ctx_abi())?,
        "lightflow://runs" => serde_json::to_value(service.list_runs()?)?,
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

fn runtime_run_request(arguments: Value) -> Result<RuntimeRunRequest, McpError> {
    let workflow_id = arguments
        .get("workflow_id")
        .or_else(|| arguments.get("workflow_asset_id"))
        .and_then(Value::as_str)
        .ok_or_else(|| McpError::new(-32602, "missing string argument: workflow_id"))?
        .to_owned();
    let run_id = arguments
        .get("run_id")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    let inputs = arguments.get("inputs").cloned().unwrap_or(Value::Null);
    Ok(RuntimeRunRequest {
        run_id,
        workflow_id,
        inputs,
    })
}

fn workflow_arg(arguments: &Value) -> Result<RuntimeWorkflow, McpError> {
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

#[derive(Debug, Clone, Deserialize)]
struct UiWorkflowPatch {
    workflow_id: String,
    #[serde(default)]
    ops: Vec<UiWorkflowPatchOp>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum UiWorkflowPatchOp {
    AddNode {
        node: UiWorkflowNode,
    },
    UpdateNode {
        node: UiWorkflowNode,
    },
    MoveNode {
        node_id: String,
        position: UiPosition,
    },
    DeleteNode {
        node_id: String,
    },
    Connect {
        from: RuntimeEndpoint,
        to: RuntimeEndpoint,
    },
    Disconnect {
        from: RuntimeEndpoint,
        to: RuntimeEndpoint,
    },
}

#[derive(Debug, Clone, Deserialize)]
struct UiWorkflowNode {
    id: String,
    kind: String,
    title: String,
    position: UiPosition,
    #[serde(default)]
    component: Option<String>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
struct UiPosition {
    x: f64,
    y: f64,
}

impl From<UiPosition> for RuntimePosition {
    fn from(position: UiPosition) -> Self {
        Self {
            x: position.x.round() as i64,
            y: position.y.round() as i64,
        }
    }
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
            "lightflow.run_trace"
        ],
        "resources": [
            "lightflow://workflows",
            "lightflow://nodes",
            "lightflow://runtime",
            "lightflow://ctx-abi",
            "lightflow://runs",
            "lightflow://mcp"
        ],
        "runtimeStreams": {
            "discovery": "/runtime/streams",
            "schema": "/runtime/streams/schema.fbs",
            "snapshotTemplate": "/runtime/streams/{run_id}/snapshot.fb",
            "liveTransport": {
                "name": "webtransport",
                "status": "available",
                "endpoint": "https://127.0.0.1:4433/{run_id}",
                "command": "lightflow stream serve-webtransport --port 4433"
            },
            "frame": {
                "encoding": "flatbuffers",
                "fileIdentifier": "LFRS"
            }
        }
    })
}
