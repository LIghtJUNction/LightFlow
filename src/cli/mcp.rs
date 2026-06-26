//! MCP JSON-RPC adapter and CLI entrypoint.

use super::{CliError, CliResult, ensure_no_extra_args, request_json};
use crate::api::ApiService;
use serde_json::{Value, json};
use std::io::{self, Read};

mod arguments;
mod error;
mod resource_catalog;
mod resources;
mod tools;
mod tools_catalog;

use error::{McpError, error};
use resource_catalog::{mcp_methods, resource_templates, resources, string_field_list};
use resources::read_resource;
use tools::call_tool;
use tools_catalog::tools;

const PROTOCOL_VERSION: &str = "2024-11-05";

pub(super) fn execute_mcp_request(service: &ApiService, args: &[String]) -> CliResult<Value> {
    if args
        .first()
        .is_some_and(|arg| matches!(arg.as_str(), "-h" | "--help" | "help"))
    {
        return Err(CliError::Usage(mcp_usage()));
    }
    let request = match args {
        [] => {
            let mut body = String::new();
            io::stdin().read_to_string(&mut body)?;
            if body.trim().is_empty() {
                return Err(CliError::Usage(mcp_usage()));
            }
            serde_json::from_str(&body)?
        }
        [_] => request_json(&args[0])?,
        _ => {
            ensure_no_extra_args(args, 1, "mcp")?;
            unreachable!("ensure_no_extra_args returns on extra arguments")
        }
    };
    Ok(handle_request(service, request))
}

fn mcp_usage() -> String {
    let mut lines = vec![
        "usage:",
        "  lfw mcp [<json|-|@file>]",
        "",
        "examples:",
        "  lfw mcp '{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\"}'",
        "  lfw mcp '{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/list\"}'",
        "  lfw mcp '{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"resources/list\"}'",
        "  lfw mcp '{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"resources/templates/list\"}'",
        "  lfw mcp '{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"resources/read\",\"params\":{\"uri\":\"lightflow://workflows/lightflow.text_plan/plan\"}}'",
        "",
        "methods:",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect::<Vec<_>>();
    append_usage_values(&mut lines, mcp_methods());
    lines.push(String::new());
    lines.push("resources:".to_owned());
    append_usage_values(&mut lines, string_field_list(resources(), "uri"));
    lines.push(String::new());
    lines.push("resource templates:".to_owned());
    append_usage_values(
        &mut lines,
        string_field_list(resource_templates(), "uriTemplate"),
    );
    lines.join("\n")
}

fn append_usage_values(lines: &mut Vec<String>, values: Value) {
    if let Some(values) = values.as_array() {
        for value in values.iter().filter_map(Value::as_str) {
            lines.push(format!("  {value}"));
        }
    }
}

/// Handle one JSON-RPC request for the CLI MCP transport or `/mcp` HTTP endpoint.
#[must_use]
pub fn handle_request(service: &ApiService, request: Value) -> Value {
    let id = request.get("id").cloned().unwrap_or(Value::Null);
    let Some(method) = request.get("method").and_then(Value::as_str) else {
        return error(id, -32600, "invalid JSON-RPC request", None);
    };
    let params = request.get("params").cloned().unwrap_or(Value::Null);

    let result = match method {
        "initialize" => Ok(initialize_result()),
        "tools/list" => Ok(json!({ "tools": tools() })),
        "tools/call" => call_tool(service, &params),
        "resources/list" => Ok(json!({ "resources": resources() })),
        "resources/templates/list" => Ok(json!({ "resourceTemplates": resource_templates() })),
        "resources/read" => read_resource(service, &params),
        "ping" => Ok(json!({})),
        _ => Err(McpError::new(
            -32601,
            format!("unknown MCP method: {method}"),
        )),
    };

    match result {
        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        Err(error_value) => error(id, error_value.code, &error_value.message, error_value.data),
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
