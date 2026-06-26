use super::tools_catalog::tools;
use serde_json::{Value, json};

pub(super) fn resources() -> Value {
    json!([
        resource("lightflow://workflows", "Workflows", "application/json"),
        resource("lightflow://nodes", "Nodes", "application/json"),
        resource("lightflow://executors", "Executors", "application/json"),
        resource("lightflow://models", "Models", "application/json"),
        resource("lightflow://runs", "Runs", "application/json"),
        resource("lightflow://artifacts", "Artifacts", "application/json"),
        resource("lightflow://patches", "Patches", "application/json"),
        resource(
            "lightflow://publish",
            "Publish Readiness",
            "application/json"
        ),
        resource(
            "lightflow://loop",
            "Local Workflow Loop",
            "application/json",
        ),
        resource(
            "lightflow://loop/changes",
            "Local Workflow Loop Changes",
            "application/json",
        ),
        resource(
            "lightflow://loop/projects",
            "Local Workflow Loop Projects",
            "application/json",
        ),
        resource(
            "lightflow://release",
            "Release Readiness",
            "application/json",
        ),
        resource("lightflow://openapi", "OpenAPI", "application/yaml"),
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

pub(super) fn resource_templates() -> Value {
    json!([
        resource_template(
            "lightflow://workflows/{workflow_id}",
            "Workflow Definition",
            "application/json",
            "One discovered workflow definition by workflow id."
        ),
        resource_template(
            "lightflow://workflows/{workflow_id}/dependencies",
            "Workflow Dependencies",
            "application/json",
            "Recursive dependency report for one discovered workflow id."
        ),
        resource_template(
            "lightflow://workflows/{workflow_id}/plan",
            "Workflow Execution Plan",
            "application/json",
            "Executor, graph, and model plan for one discovered workflow id without running it."
        ),
        resource_template(
            "lightflow://workflows/{workflow_id}/publish",
            "Workflow Publish Readiness",
            "application/json",
            "Cargo publish dry-run readiness for one discovered workflow id."
        ),
        resource_template(
            "lightflow://nodes/{workflow_id}",
            "Workflow Node Card",
            "application/json",
            "Editor-facing node card for one discovered workflow id."
        ),
        resource_template(
            "lightflow://models?workflow_id={workflow_id}",
            "Workflow Model Requirements",
            "application/json",
            "Model requirement catalog narrowed to one workflow id."
        ),
        resource_template(
            "lightflow://models?workflow_id={workflow_id}&status={status}",
            "Workflow Model Requirements By Status",
            "application/json",
            "Model requirement catalog narrowed to one workflow id and lock status: all, available, or blocked."
        ),
        resource_template(
            "lightflow://runs?workflow_id={workflow_id}&status={status}&limit={limit}",
            "Workflow Run History",
            "application/json",
            "Run history narrowed to one workflow id, optional status, and maximum result count."
        ),
        resource_template(
            "lightflow://runs/{run_id}",
            "Run Trace",
            "application/json",
            "Full recorded run manifest, execution, and event data by run id, or last."
        ),
        resource_template(
            "lightflow://runs/{run_id}/events",
            "Run Events",
            "application/json",
            "Recorded event timeline for one run id, or last."
        ),
        resource_template(
            "lightflow://artifacts?run_id={run_id}&workflow_id={workflow_id}&kind={kind}&limit={limit}",
            "Workflow Artifact Catalog",
            "application/json",
            "Artifact catalog narrowed by run id, workflow id, artifact kind, and maximum result count."
        ),
        resource_template(
            "lightflow://patches/{name}",
            "Workflow Patch",
            "application/json",
            "Reusable project workflow patch by registry name."
        ),
        resource_template(
            "lightflow://publish?project={project}",
            "Project Publish Readiness",
            "application/json",
            "Dependency-ordered publish readiness for one linked project workspace by full name, label, path, or lightflow-* short alias."
        ),
        resource_template(
            "lightflow://loop?workflow_id={workflow_id}",
            "Workflow Loop Readiness",
            "application/json",
            "Local workflow-loop readiness report narrowed to one selected workflow id."
        ),
        resource_template(
            "lightflow://loop?workflow_id={workflow_id}&require_replay={require_replay}",
            "Workflow Loop Replay Readiness",
            "application/json",
            "Local workflow-loop readiness report for one selected workflow id, optionally failing when no completed run can be replayed."
        ),
        resource_template(
            "lightflow://loop/projects?project={project}",
            "Project Workspace Catalog",
            "application/json",
            "Sibling project workspace catalog narrowed to one linked project workspace by full name, label, path, or lightflow-* short alias."
        ),
        resource_template(
            "lightflow://loop/projects?project={project}&dirty={dirty}",
            "Dirty Project Workspace Catalog",
            "application/json",
            "Sibling project workspace catalog narrowed to one linked project workspace, optionally returning only dirty or gitlink-stale workspaces."
        ),
        resource_template(
            "lightflow://release?workflow_id={workflow_id}",
            "Workflow Release Readiness",
            "application/json",
            "Release readiness dry-run report for one selected workflow id."
        ),
        resource_template(
            "lightflow://release?workflow_id={workflow_id}&project={project}",
            "Project Release Readiness",
            "application/json",
            "Release readiness dry-run report for one selected workflow id and linked project workspace."
        )
    ])
}

fn resource_template(uri_template: &str, name: &str, mime_type: &str, description: &str) -> Value {
    json!({
        "uriTemplate": uri_template,
        "name": name,
        "description": description,
        "mimeType": mime_type
    })
}

pub(super) fn mcp_resource() -> Value {
    json!({
        "endpoint": "http://127.0.0.1:5174/mcp",
        "transport": "http",
        "jsonrpc": "2.0",
        "methods": mcp_methods(),
        "tools": string_field_list(tools(), "name"),
        "resources": string_field_list(resources(), "uri"),
        "resourceTemplates": string_field_list(resource_templates(), "uriTemplate")
    })
}

pub(super) fn mcp_methods() -> Value {
    json!([
        "initialize",
        "ping",
        "tools/list",
        "tools/call",
        "resources/list",
        "resources/templates/list",
        "resources/read"
    ])
}

pub(super) fn string_field_list(values: Value, field: &str) -> Value {
    let items = values
        .as_array()
        .map(|values| {
            values
                .iter()
                .filter_map(|value| value.get(field).and_then(Value::as_str))
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    json!(items)
}
