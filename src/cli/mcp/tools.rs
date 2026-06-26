use super::arguments::{
    artifact_list_options_arg, model_list_options_arg, patch_arg, recorded_workflow_run,
    required_str, run_list_options_arg, workflow_arg,
};
use super::error::McpError;
use crate::api::{ApiService, ProjectWorkspaceOptions};
use serde_json::{Value, json};

pub(super) fn call_tool(service: &ApiService, params: &Value) -> Result<Value, McpError> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| McpError::new(-32602, "tools/call requires params.name"))?;
    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let value = match name {
        "lightflow.workflow.list" => serde_json::to_value(service.list_workflows()?)?,
        "lightflow.workflow.get" => {
            serde_json::to_value(service.get_workflow(required_str(&arguments, "workflow_id")?)?)?
        }
        "lightflow.workflow.dependencies" => serde_json::to_value(
            service.workflow_dependencies(required_str(&arguments, "workflow_id")?)?,
        )?,
        "lightflow.workflow.plan" => {
            serde_json::to_value(service.plan_workflow(required_str(&arguments, "workflow_id")?)?)?
        }
        "lightflow.workflow.publish_check" => serde_json::to_value(
            service.workflow_publish_check(required_str(&arguments, "workflow_id")?)?,
        )?,
        "lightflow.workflow.publish_list" => serde_json::to_value(
            service.workflow_publish_checks_with_options(&crate::api::WorkflowPublishOptions {
                project: arguments
                    .get("project")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned),
            })?,
        )?,
        "lightflow.workflow.run" => recorded_workflow_run(service, &arguments)?,
        "lightflow.workflow.validate" => {
            serde_json::to_value(service.validate_workflow(&workflow_arg(&arguments)?))?
        }
        "lightflow.workflow.save" => {
            serde_json::to_value(service.save_workflow(workflow_arg(&arguments)?)?)?
        }
        "lightflow.node.list" => serde_json::to_value(service.list_nodes()?)?,
        "lightflow.node.get" => {
            serde_json::to_value(service.get_node(required_str(&arguments, "workflow_id")?)?)?
        }
        "lightflow.executor.list" => serde_json::to_value(service.list_executors())?,
        "lightflow.model.list" => serde_json::to_value(
            service.list_models_with_options(&model_list_options_arg(&arguments)?)?,
        )?,
        "lightflow.run.list" => serde_json::to_value(
            service.list_runs_with_options(&run_list_options_arg(&arguments)?)?,
        )?,
        "lightflow.run.get" => {
            serde_json::to_value(service.get_run(required_str(&arguments, "run_id")?)?)?
        }
        "lightflow.run.events" => {
            serde_json::to_value(service.get_run_events(required_str(&arguments, "run_id")?)?)?
        }
        "lightflow.run.replay" => {
            service.replay_run_with_surface(required_str(&arguments, "run_id")?, "mcp")?
        }
        "lightflow.run.rm" => {
            serde_json::to_value(service.remove_run(required_str(&arguments, "run_id")?)?)?
        }
        "lightflow.artifact.list" => serde_json::to_value(
            service.list_artifacts_with_options(&artifact_list_options_arg(&arguments)?)?,
        )?,
        "lightflow.patch.list" => serde_json::to_value(service.list_patches()?)?,
        "lightflow.patch.get" => {
            serde_json::to_value(service.get_patch(required_str(&arguments, "name")?)?)?
        }
        "lightflow.patch.save" => serde_json::to_value(
            service.save_patch(required_str(&arguments, "name")?, &patch_arg(&arguments)?)?,
        )?,
        "lightflow.patch.validate" => {
            let patch = patch_arg(&arguments)?;
            if let Some(workflow_id) = arguments.get("workflow_id").and_then(Value::as_str) {
                serde_json::to_value(service.validate_patch_for_workflow(workflow_id, patch))?
            } else {
                serde_json::to_value(service.validate_patch(patch))?
            }
        }
        "lightflow.patch.rm" => {
            serde_json::to_value(service.remove_patch(required_str(&arguments, "name")?)?)?
        }
        "lightflow.loop.check" => {
            let workflow_id = arguments.get("workflow_id").and_then(Value::as_str);
            let require_replay = arguments
                .get("require_replay")
                .and_then(Value::as_bool)
                .unwrap_or(false)
                || arguments
                    .get("require_selected_replay")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
            if require_replay && workflow_id.is_none() {
                return Err(McpError::new(
                    -32602,
                    "lightflow.loop.check require_replay requires workflow_id",
                ));
            }
            serde_json::to_value(
                service.local_loop_check_with_options(workflow_id, require_replay)?,
            )?
        }
        "lightflow.loop.changes" => serde_json::to_value(service.local_loop_changes()?)?,
        "lightflow.loop.projects" => serde_json::to_value(
            service.project_workspaces_with_options(ProjectWorkspaceOptions {
                dirty_only: arguments
                    .get("dirty")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                project: arguments
                    .get("project")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned),
            })?,
        )?,
        "lightflow.release.check" => {
            let workflow_id = arguments
                .get("workflow_id")
                .and_then(Value::as_str)
                .unwrap_or("lightflow.text_plan")
                .to_owned();
            let project = arguments
                .get("project")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned);
            serde_json::to_value(service.release_check(&crate::api::ReleaseCheckOptions {
                apply: false,
                workflow_id,
                project,
                profile: crate::api::CheckProfile::Release,
            })?)?
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
