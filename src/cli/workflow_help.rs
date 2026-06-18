use super::{CliResult, ensure_no_extra_args, required_arg};
use crate::api::ApiService;
use crate::workflow::{ModelRequirement, PortSpec, RuntimeRequirement, WorkflowSpec};
use serde::Serialize;
use serde_json::json;

#[derive(Debug, Clone, Serialize)]
struct WorkflowHelpPort {
    name: String,
    #[serde(rename = "type")]
    ty: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    cli_flag: Option<String>,
    value_hint: String,
}

#[derive(Debug, Clone, Serialize)]
struct WorkflowHelpUsage {
    command: Vec<String>,
    input_flags: Vec<String>,
    inputs_json_shape: serde_json::Value,
}

pub(super) fn workflow_help(
    service: &ApiService,
    args: &[String],
    command: &str,
) -> CliResult<serde_json::Value> {
    let workflow_id = required_arg(args, 0, "workflow id")?;
    ensure_no_extra_args(args, 1, command)?;
    let workflow = service.get_workflow(workflow_id)?;
    let dependencies = service.workflow_dependencies(workflow_id)?;
    Ok(workflow_help_json(&workflow, dependencies))
}

fn workflow_help_json(
    workflow: &WorkflowSpec,
    dependencies: crate::workflow::WorkflowDependencyReport,
) -> serde_json::Value {
    let input_ports = workflow
        .inputs
        .iter()
        .map(|port| help_port(port, true))
        .collect::<Vec<_>>();
    let output_ports = workflow
        .outputs
        .iter()
        .map(|port| help_port(port, false))
        .collect::<Vec<_>>();
    let input_flags = input_ports
        .iter()
        .map(|port| format!("-i {}={}", port.name, port.value_hint))
        .collect::<Vec<_>>();

    json!({
        "workflow": {
            "id": workflow.id,
            "version": workflow.version,
            "name": workflow.name,
            "category": workflow.category,
            "description": workflow.description,
            "kind": if workflow.nodes.is_empty() { "leaf" } else { "composite" },
        },
        "ports": {
            "inputs": input_ports,
            "outputs": output_ports,
            "constraints": {
                "source": "workflow metadata",
                "note": "LightFlow currently records port names and types; required/default constraints are not represented in the workflow DSL yet."
            }
        },
        "dependencies": {
            "complete": dependencies.complete,
            "workflow_order": dependencies.workflow_order,
            "missing_workflows": dependencies.missing_workflows,
            "version_mismatches": dependencies.version_mismatches,
            "declared": workflow.dependencies,
        },
        "models": workflow.models.iter().map(model_help).collect::<Vec<_>>(),
        "runtimes": workflow.runtimes.iter().map(runtime_help).collect::<Vec<_>>(),
        "graph": {
            "nodes": workflow.nodes,
            "edges": workflow.edges,
        },
        "usage": WorkflowHelpUsage {
            command: run_command(workflow, &input_flags),
            input_flags,
            inputs_json_shape: inputs_json_shape(workflow),
        },
    })
}

fn help_port(port: &PortSpec, cli_input: bool) -> WorkflowHelpPort {
    WorkflowHelpPort {
        name: port.name.clone(),
        ty: port.ty.clone(),
        cli_flag: cli_input.then(|| format!("-i {}={}", port.name, value_hint(&port.ty))),
        value_hint: value_hint(&port.ty).to_owned(),
    }
}

fn value_hint(ty: &str) -> &'static str {
    match ty {
        "text" | "path" => "\"...\"",
        "integer" => "0",
        "number" => "0.0",
        "boolean" => "false",
        "artifact" | "artifact[]" | "json" => "{}",
        _ => "null",
    }
}

fn run_command(workflow: &WorkflowSpec, input_flags: &[String]) -> Vec<String> {
    let mut command = vec!["lfw".to_owned(), "run".to_owned(), workflow.id.clone()];
    command.extend(input_flags.iter().cloned());
    command
}

fn inputs_json_shape(workflow: &WorkflowSpec) -> serde_json::Value {
    let mut inputs = serde_json::Map::new();
    for port in &workflow.inputs {
        inputs.insert(port.name.clone(), example_value(&port.ty));
    }
    serde_json::Value::Object(inputs)
}

fn example_value(ty: &str) -> serde_json::Value {
    match ty {
        "text" | "path" => serde_json::Value::String("...".to_owned()),
        "integer" => 0.into(),
        "number" => serde_json::json!(0.0),
        "boolean" => false.into(),
        "artifact[]" => serde_json::Value::Array(Vec::new()),
        "artifact" | "json" => serde_json::json!({}),
        _ => serde_json::Value::Null,
    }
}

fn model_help(model: &ModelRequirement) -> serde_json::Value {
    json!({
        "id": model.id,
        "capability": model.capability,
        "variants": model.variants,
    })
}

fn runtime_help(runtime: &RuntimeRequirement) -> serde_json::Value {
    json!({
        "id": runtime.id,
        "capability": runtime.capability,
        "engine": runtime.engine,
    })
}
