use super::arguments::model_status_filter;
use super::error::McpError;
use super::resource_catalog::mcp_resource;
use crate::api::{
    ApiService, ArtifactListOptions, ModelListOptions, ModelStatusFilter, ProjectWorkspaceOptions,
    RunListOptions,
};
use serde::Serialize;
use serde_json::{Value, json};

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

fn split_resource_uri(uri: &str) -> (&str, Option<&str>) {
    uri.split_once('?')
        .map(|(resource_uri, query)| (resource_uri, Some(query)))
        .unwrap_or((uri, None))
}

fn resource_query_value(query: Option<&str>, name: &str) -> Option<String> {
    resource_query_parts(query)
        .find_map(|(key, value)| (key == name).then_some(value).flatten())
        .map(decode_query_component)
}

fn resource_query_has_key(query: Option<&str>, name: &str) -> bool {
    resource_query_parts(query).any(|(key, _value)| key == name)
}

fn resource_query_parts(query: Option<&str>) -> impl Iterator<Item = (String, Option<&str>)> {
    query
        .into_iter()
        .flat_map(|query| query.split('&'))
        .filter(|part| !part.is_empty())
        .map(|part| {
            let (key, value) = part
                .split_once('=')
                .map(|(key, value)| (key, Some(value)))
                .unwrap_or((part, None));
            (decode_query_component(key), value)
        })
}

fn decode_query_component(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'+' => {
                decoded.push(b' ');
                index += 1;
            }
            b'%' if index + 2 < bytes.len() => {
                if let (Some(high), Some(low)) =
                    (hex_value(bytes[index + 1]), hex_value(bytes[index + 2]))
                {
                    decoded.push(high * 16 + low);
                    index += 3;
                } else {
                    decoded.push(bytes[index]);
                    index += 1;
                }
            }
            byte => {
                decoded.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn resource_id<'a>(uri: &'a str, prefix: &str) -> Option<&'a str> {
    uri.strip_prefix(prefix).filter(|id| !id.contains('/'))
}

fn resource_child_id<'a>(uri: &'a str, prefix: &str, child: &str) -> Option<&'a str> {
    let path = uri.strip_prefix(prefix)?;
    let (id, suffix) = path.rsplit_once('/')?;
    (suffix == child && !id.contains('/')).then_some(id)
}

fn resource_query_bool(query: Option<&str>, name: &str) -> Option<bool> {
    resource_query_value(query, name)
        .map(|value| matches!(value.as_str(), "1" | "true" | "yes"))
        .or_else(|| resource_query_has_key(query, name).then_some(true))
}

fn json_resource_read_response<T: Serialize>(uri: &str, value: &T) -> Result<Value, McpError> {
    Ok(resource_read_response(
        uri,
        "application/json",
        serde_json::to_string_pretty(value)?,
    ))
}

fn resource_query_usize(
    query: Option<&str>,
    name: &str,
    context: &str,
) -> Result<Option<usize>, McpError> {
    let Some(value) = resource_query_value(query, name) else {
        return Ok(None);
    };
    value.parse::<usize>().map(Some).map_err(|_| {
        McpError::new(
            -32602,
            format!("{context} {name} must be a non-negative integer"),
        )
    })
}

fn model_list_options_query(query: Option<&str>) -> Result<ModelListOptions, McpError> {
    let status = match resource_query_value(query, "status") {
        Some(value) => model_status_filter(&value)?,
        None => ModelStatusFilter::All,
    };
    Ok(ModelListOptions {
        workflow_id: resource_query_value(query, "workflow_id"),
        status,
    })
}

fn run_list_options_query(query: Option<&str>) -> Result<RunListOptions, McpError> {
    Ok(RunListOptions {
        limit: resource_query_usize(query, "limit", "lightflow://runs")?,
        workflow_id: resource_query_value(query, "workflow_id"),
        status: resource_query_value(query, "status"),
    })
}

fn artifact_list_options_query(query: Option<&str>) -> Result<ArtifactListOptions, McpError> {
    Ok(ArtifactListOptions {
        limit: resource_query_usize(query, "limit", "lightflow://artifacts")?,
        run_id: resource_query_value(query, "run_id"),
        workflow_id: resource_query_value(query, "workflow_id"),
        kind: resource_query_value(query, "kind"),
    })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resource_query_value_decodes_percent_encoded_values() {
        assert_eq!(
            resource_query_value(
                Some("project=%2Ftmp%2Flightflow%2Fprojects%2Flightflow-std"),
                "project",
            )
            .as_deref(),
            Some("/tmp/lightflow/projects/lightflow-std")
        );
        assert_eq!(
            resource_query_value(Some("workflow_id=lightflow.text%2Bplan"), "workflow_id")
                .as_deref(),
            Some("lightflow.text+plan")
        );
    }

    #[test]
    fn resource_query_value_decodes_percent_encoded_keys() {
        assert_eq!(
            resource_query_value(Some("workflow%5Fid=lightflow.text_plan"), "workflow_id")
                .as_deref(),
            Some("lightflow.text_plan")
        );
    }

    #[test]
    fn resource_query_value_decodes_plus_as_space_and_keeps_malformed_percent() {
        assert_eq!(
            resource_query_value(Some("kind=image+artifact"), "kind").as_deref(),
            Some("image artifact")
        );
        assert_eq!(
            resource_query_value(Some("project=%ZZ%2Flightflow"), "project").as_deref(),
            Some("%ZZ/lightflow")
        );
    }

    #[test]
    fn typed_resource_query_helpers_use_decoded_values() {
        assert_eq!(resource_query_bool(Some("dirty=true"), "dirty"), Some(true));
        assert_eq!(resource_query_bool(Some("dirty"), "dirty"), Some(true));
        assert_eq!(
            resource_query_value(Some("limit"), "limit"),
            None,
            "bare non-boolean parameters are key presence, not values"
        );
        assert_eq!(
            resource_query_usize(Some("limit"), "limit", "lightflow://runs")
                .expect("bare limit is not a value"),
            None
        );
        assert_eq!(
            resource_query_usize(Some("limit=20"), "limit", "lightflow://runs")
                .expect("limit parse"),
            Some(20)
        );
        assert_eq!(
            model_list_options_query(Some("workflow_id=lightflow.text%2Bplan&status=blocked"))
                .expect("model options")
                .workflow_id
                .as_deref(),
            Some("lightflow.text+plan")
        );
    }
}
