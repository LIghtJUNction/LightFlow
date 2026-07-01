use super::error::McpError;
use super::resource_catalog::mcp_resource;
use crate::api::{ApiService, ProjectWorkspaceOptions};
use serde::Serialize;
use serde_json::{Value, json};

mod queries;
use queries::{
    artifact_list_options_query, model_list_options_query, resource_child_id, resource_id,
    resource_query_bool, resource_query_value, run_list_options_query, split_resource_uri,
};

const OPENAPI_YAML: &str = include_str!("../../../openapi/lightflow.yaml");

pub(super) fn read_resource(service: &ApiService, params: &Value) -> Result<Value, McpError> {
    let uri = params
        .get("uri")
        .and_then(Value::as_str)
        .ok_or_else(|| McpError::new(-32602, "resources/read requires params.uri"))?;
    let (resource_uri, query) = split_resource_uri(uri);
    if resource_uri == "lightflow://openapi" {
        return Ok(resource_read_response(
            uri,
            "application/yaml",
            OPENAPI_YAML.to_owned(),
        ));
    }
    if let Some(workflow_id) =
        resource_child_id(resource_uri, "lightflow://workflows/", "dependencies")
    {
        let value = service.workflow_dependencies(workflow_id)?;
        return json_resource_read_response(uri, &value);
    }
    if let Some(workflow_id) = resource_child_id(resource_uri, "lightflow://workflows/", "plan") {
        let value = service.plan_workflow(workflow_id)?;
        return json_resource_read_response(uri, &value);
    }
    if let Some(workflow_id) = resource_child_id(resource_uri, "lightflow://workflows/", "publish")
    {
        let value = service.workflow_publish_check(workflow_id)?;
        return json_resource_read_response(uri, &value);
    }
    if let Some(workflow_id) = resource_id(resource_uri, "lightflow://workflows/") {
        let value = service.get_workflow(workflow_id)?;
        return json_resource_read_response(uri, &value);
    }
    if let Some(workflow_id) = resource_id(resource_uri, "lightflow://nodes/") {
        let value = service.get_node(workflow_id)?;
        return json_resource_read_response(uri, &value);
    }
    if let Some(run_id) = resource_child_id(resource_uri, "lightflow://runs/", "events") {
        let value = service.get_run_events(run_id)?;
        return json_resource_read_response(uri, &value);
    }
    if let Some(run_id) = resource_id(resource_uri, "lightflow://runs/") {
        let value = service.get_run(run_id)?;
        return json_resource_read_response(uri, &value);
    }
    if let Some(name) = resource_id(resource_uri, "lightflow://patches/") {
        let value = service.get_patch(name)?;
        return json_resource_read_response(uri, &value);
    }
    let value = match resource_uri {
        "lightflow://workflows" => serde_json::to_value(service.list_workflows()?)?,
        "lightflow://nodes" => serde_json::to_value(service.list_nodes()?)?,
        "lightflow://executors" => serde_json::to_value(service.list_executors())?,
        "lightflow://models" => serde_json::to_value(
            service.list_models_with_options(&model_list_options_query(query)?)?,
        )?,
        "lightflow://runs" => {
            serde_json::to_value(service.list_runs_with_options(&run_list_options_query(query)?)?)?
        }
        "lightflow://artifacts" => serde_json::to_value(
            service.list_artifacts_with_options(&artifact_list_options_query(query)?)?,
        )?,
        "lightflow://patches" => serde_json::to_value(service.list_patches()?)?,
        "lightflow://publish" => serde_json::to_value(
            service.workflow_publish_checks_with_options(&crate::api::WorkflowPublishOptions {
                project: resource_query_value(query, "project"),
            })?,
        )?,
        "lightflow://loop" => {
            let workflow_id = resource_query_value(query, "workflow_id");
            let require_replay = resource_query_bool(query, "require_replay").unwrap_or(false)
                || resource_query_bool(query, "require_selected_replay").unwrap_or(false);
            if require_replay && workflow_id.is_none() {
                return Err(McpError::new(
                    -32602,
                    "lightflow://loop require_replay requires workflow_id",
                ));
            }
            serde_json::to_value(
                service.local_loop_check_with_options(workflow_id.as_deref(), require_replay)?,
            )?
        }
        "lightflow://loop/changes" => serde_json::to_value(service.local_loop_changes()?)?,
        "lightflow://loop/projects" => serde_json::to_value(
            service.project_workspaces_with_options(ProjectWorkspaceOptions {
                dirty_only: resource_query_bool(query, "dirty").unwrap_or(false),
                project: resource_query_value(query, "project"),
            })?,
        )?,
        "lightflow://release" => {
            let mut options = crate::api::ReleaseCheckOptions::default();
            if let Some(workflow_id) = resource_query_value(query, "workflow_id") {
                options.workflow_id = workflow_id;
            }
            options.project = resource_query_value(query, "project");
            serde_json::to_value(service.release_check(&options)?)?
        }
        "lightflow://mcp" => mcp_resource(),
        _ => return Err(McpError::new(-32602, format!("unknown resource: {uri}"))),
    };

    json_resource_read_response(uri, &value)
}

fn json_resource_read_response<T: Serialize>(uri: &str, value: &T) -> Result<Value, McpError> {
    Ok(resource_read_response(
        uri,
        "application/json",
        serde_json::to_string_pretty(value)?,
    ))
}

fn resource_read_response(uri: &str, mime_type: &str, text: String) -> Value {
    json!({
        "contents": [
            {
                "uri": uri,
                "mimeType": mime_type,
                "text": text
            }
        ]
    })
}
