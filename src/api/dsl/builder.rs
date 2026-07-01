use super::arguments::{
    bool_arg, expect_arg_len, find_port_mut, json_array_string_arg, json_string_arg, number_arg,
    string_arg,
};
use crate::api::{ApiError, ApiResult};
use crate::workflow::{
    CargoDependency, CargoDependencySource, ModelProvider, ModelRequirement, ModelVariant,
    PortSpec, RuntimeRequirement, WorkflowCondition, WorkflowDependencyRequirement, WorkflowEdge,
    WorkflowEndpoint, WorkflowNode, WorkflowNodeKind, WorkflowPosition, WorkflowSpec,
};
use std::path::Path;

pub(super) fn define_expression(function: &syn::ItemFn) -> Option<&syn::Expr> {
    function
        .block
        .stmts
        .iter()
        .rev()
        .find_map(|statement| match statement {
            syn::Stmt::Expr(syn::Expr::Return(return_expr), _) => return_expr.expr.as_deref(),
            syn::Stmt::Expr(expression, _) => Some(expression),
            _ => None,
        })
}

pub(super) fn parse_workflow_builder(
    expression: &syn::Expr,
    path: &Path,
) -> ApiResult<WorkflowSpec> {
    match expression {
        syn::Expr::MethodCall(call) => {
            let mut workflow = parse_workflow_builder(&call.receiver, path)?;
            let method = call.method.to_string();
            match method.as_str() {
                "build" => expect_arg_len(&call.args, 0, &method, path)?,
                "version" => {
                    workflow.version = string_arg(&call.args, 0, &method, path)?;
                    expect_arg_len(&call.args, 1, &method, path)?;
                }
                "name" => {
                    workflow.name = string_arg(&call.args, 0, &method, path)?;
                    expect_arg_len(&call.args, 1, &method, path)?;
                }
                "description" => {
                    workflow.description = Some(string_arg(&call.args, 0, &method, path)?);
                    expect_arg_len(&call.args, 1, &method, path)?;
                }
                "input" => {
                    workflow.inputs.push(PortSpec::new(
                        string_arg(&call.args, 0, &method, path)?,
                        string_arg(&call.args, 1, &method, path)?,
                    ));
                    expect_arg_len(&call.args, 2, &method, path)?;
                }
                "output" => {
                    workflow.outputs.push(PortSpec::new(
                        string_arg(&call.args, 0, &method, path)?,
                        string_arg(&call.args, 1, &method, path)?,
                    ));
                    expect_arg_len(&call.args, 2, &method, path)?;
                }
                "input_description" => {
                    let name = string_arg(&call.args, 0, &method, path)?;
                    let description = string_arg(&call.args, 1, &method, path)?;
                    if let Some(port) = find_port_mut(&mut workflow.inputs, &name) {
                        port.description = Some(description);
                    }
                    expect_arg_len(&call.args, 2, &method, path)?;
                }
                "output_description" => {
                    let name = string_arg(&call.args, 0, &method, path)?;
                    let description = string_arg(&call.args, 1, &method, path)?;
                    if let Some(port) = find_port_mut(&mut workflow.outputs, &name) {
                        port.description = Some(description);
                    }
                    expect_arg_len(&call.args, 2, &method, path)?;
                }
                "input_required" => {
                    let name = string_arg(&call.args, 0, &method, path)?;
                    let required = bool_arg(&call.args, 1, &method, path)?;
                    if let Some(port) = find_port_mut(&mut workflow.inputs, &name) {
                        port.required = Some(required);
                    }
                    expect_arg_len(&call.args, 2, &method, path)?;
                }
                "input_default_json" => {
                    let name = string_arg(&call.args, 0, &method, path)?;
                    let value = json_string_arg(&call.args, 1, &method, path)?;
                    if let Some(port) = find_port_mut(&mut workflow.inputs, &name) {
                        port.default = Some(value);
                    }
                    expect_arg_len(&call.args, 2, &method, path)?;
                }
                "input_range" => {
                    let name = string_arg(&call.args, 0, &method, path)?;
                    let min = number_arg(&call.args, 1, &method, path)?;
                    let max = number_arg(&call.args, 2, &method, path)?;
                    let step = number_arg(&call.args, 3, &method, path)?;
                    if let Some(port) = find_port_mut(&mut workflow.inputs, &name) {
                        port.min = Some(min);
                        port.max = Some(max);
                        port.step = Some(step);
                    }
                    expect_arg_len(&call.args, 4, &method, path)?;
                }
                "input_enum_json" => {
                    let name = string_arg(&call.args, 0, &method, path)?;
                    let values = json_array_string_arg(&call.args, 1, &method, path)?;
                    if let Some(port) = find_port_mut(&mut workflow.inputs, &name) {
                        port.enum_values = values;
                    }
                    expect_arg_len(&call.args, 2, &method, path)?;
                }
                "input_widget" => {
                    let name = string_arg(&call.args, 0, &method, path)?;
                    let widget = string_arg(&call.args, 1, &method, path)?;
                    if let Some(port) = find_port_mut(&mut workflow.inputs, &name) {
                        port.widget = Some(widget);
                    }
                    expect_arg_len(&call.args, 2, &method, path)?;
                }
                "input_artifact_kind" => {
                    let name = string_arg(&call.args, 0, &method, path)?;
                    let kind = string_arg(&call.args, 1, &method, path)?;
                    if let Some(port) = find_port_mut(&mut workflow.inputs, &name) {
                        port.artifact_kind = Some(kind);
                    }
                    expect_arg_len(&call.args, 2, &method, path)?;
                }
                "output_artifact_kind" => {
                    let name = string_arg(&call.args, 0, &method, path)?;
                    let kind = string_arg(&call.args, 1, &method, path)?;
                    if let Some(port) = find_port_mut(&mut workflow.outputs, &name) {
                        port.artifact_kind = Some(kind);
                    }
                    expect_arg_len(&call.args, 2, &method, path)?;
                }
                "input_model_requirement" => {
                    let name = string_arg(&call.args, 0, &method, path)?;
                    let requirement_id = string_arg(&call.args, 1, &method, path)?;
                    if let Some(port) = find_port_mut(&mut workflow.inputs, &name) {
                        port.model_requirement = Some(requirement_id);
                    }
                    expect_arg_len(&call.args, 2, &method, path)?;
                }
                "output_model_requirement" => {
                    let name = string_arg(&call.args, 0, &method, path)?;
                    let requirement_id = string_arg(&call.args, 1, &method, path)?;
                    if let Some(port) = find_port_mut(&mut workflow.outputs, &name) {
                        port.model_requirement = Some(requirement_id);
                    }
                    expect_arg_len(&call.args, 2, &method, path)?;
                }
                "depends_on" => {
                    workflow.dependencies.push(WorkflowDependencyRequirement {
                        workflow_id: string_arg(&call.args, 0, &method, path)?,
                        version: Some(string_arg(&call.args, 1, &method, path)?),
                        install: None,
                    });
                    expect_arg_len(&call.args, 2, &method, path)?;
                }
                "depends_on_crate" => {
                    let workflow_id = string_arg(&call.args, 0, &method, path)?;
                    let version = string_arg(&call.args, 1, &method, path)?;
                    let crate_name = string_arg(&call.args, 2, &method, path)?;
                    workflow.dependencies.push(WorkflowDependencyRequirement {
                        workflow_id,
                        version: Some(version.clone()),
                        install: Some(CargoDependency {
                            crate_name,
                            version: Some(version),
                            source: None,
                            package: None,
                        }),
                    });
                    expect_arg_len(&call.args, 3, &method, path)?;
                }
                "depends_on_path" => {
                    let workflow_id = string_arg(&call.args, 0, &method, path)?;
                    let version = string_arg(&call.args, 1, &method, path)?;
                    let crate_name = string_arg(&call.args, 2, &method, path)?;
                    let dependency_path = string_arg(&call.args, 3, &method, path)?;
                    workflow.dependencies.push(WorkflowDependencyRequirement {
                        workflow_id,
                        version: Some(version.clone()),
                        install: Some(CargoDependency {
                            crate_name,
                            version: Some(version),
                            source: Some(CargoDependencySource::Path(dependency_path)),
                            package: None,
                        }),
                    });
                    expect_arg_len(&call.args, 4, &method, path)?;
                }
                "depends_on_git" => {
                    let workflow_id = string_arg(&call.args, 0, &method, path)?;
                    let version = string_arg(&call.args, 1, &method, path)?;
                    let crate_name = string_arg(&call.args, 2, &method, path)?;
                    let git = string_arg(&call.args, 3, &method, path)?;
                    let package = string_arg(&call.args, 4, &method, path)?;
                    workflow.dependencies.push(WorkflowDependencyRequirement {
                        workflow_id,
                        version: Some(version.clone()),
                        install: Some(CargoDependency {
                            crate_name,
                            version: Some(version),
                            source: Some(CargoDependencySource::Git(git)),
                            package: Some(package).filter(|package| !package.is_empty()),
                        }),
                    });
                    expect_arg_len(&call.args, 5, &method, path)?;
                }
                "model" => {
                    workflow.models.push(ModelRequirement {
                        id: string_arg(&call.args, 0, &method, path)?,
                        capability: string_arg(&call.args, 1, &method, path)?,
                        variants: Vec::new(),
                    });
                    expect_arg_len(&call.args, 2, &method, path)?;
                }
                "hf_model" => {
                    push_hf_model_variant(
                        &mut workflow,
                        string_arg(&call.args, 0, &method, path)?,
                        string_arg(&call.args, 1, &method, path)?,
                        string_arg(&call.args, 2, &method, path)?,
                        string_arg(&call.args, 3, &method, path)?,
                        string_arg(&call.args, 4, &method, path)?,
                        string_arg(&call.args, 5, &method, path)?,
                    );
                    expect_arg_len(&call.args, 6, &method, path)?;
                }
                "runtime" => {
                    workflow.runtimes.push(RuntimeRequirement {
                        id: string_arg(&call.args, 0, &method, path)?,
                        capability: string_arg(&call.args, 1, &method, path)?,
                        engine: None,
                    });
                    expect_arg_len(&call.args, 2, &method, path)?;
                }
                "builtin_runtime" => {
                    workflow.runtimes.push(RuntimeRequirement {
                        id: string_arg(&call.args, 0, &method, path)?,
                        capability: string_arg(&call.args, 1, &method, path)?,
                        engine: Some(string_arg(&call.args, 2, &method, path)?),
                    });
                    expect_arg_len(&call.args, 3, &method, path)?;
                }
                "node" => {
                    workflow.nodes.push(WorkflowNode {
                        id: string_arg(&call.args, 0, &method, path)?,
                        kind: WorkflowNodeKind::Workflow,
                        workflow_id: string_arg(&call.args, 1, &method, path)?,
                        condition: None,
                        then_workflow_id: None,
                        else_workflow_id: None,
                        title: None,
                        disabled: false,
                        position: WorkflowPosition::default(),
                        config: serde_json::Value::Null,
                    });
                    expect_arg_len(&call.args, 2, &method, path)?;
                }
                "disabled_node" => {
                    workflow.nodes.push(WorkflowNode {
                        id: string_arg(&call.args, 0, &method, path)?,
                        kind: WorkflowNodeKind::Workflow,
                        workflow_id: string_arg(&call.args, 1, &method, path)?,
                        condition: None,
                        then_workflow_id: None,
                        else_workflow_id: None,
                        title: None,
                        disabled: true,
                        position: WorkflowPosition::default(),
                        config: serde_json::Value::Null,
                    });
                    expect_arg_len(&call.args, 2, &method, path)?;
                }
                "if_node" => {
                    workflow.nodes.push(WorkflowNode {
                        id: string_arg(&call.args, 0, &method, path)?,
                        kind: WorkflowNodeKind::If,
                        workflow_id: String::new(),
                        condition: Some(WorkflowCondition::InputEquals {
                            input: string_arg(&call.args, 1, &method, path)?,
                            value: serde_json::Value::Bool(bool_arg(&call.args, 2, &method, path)?),
                        }),
                        then_workflow_id: Some(string_arg(&call.args, 3, &method, path)?),
                        else_workflow_id: Some(string_arg(&call.args, 4, &method, path)?),
                        title: None,
                        disabled: false,
                        position: WorkflowPosition::default(),
                        config: serde_json::Value::Null,
                    });
                    expect_arg_len(&call.args, 5, &method, path)?;
                }
                "edge" => {
                    workflow.edges.push(WorkflowEdge {
                        from: WorkflowEndpoint {
                            node: string_arg(&call.args, 0, &method, path)?,
                            port: string_arg(&call.args, 1, &method, path)?,
                        },
                        to: WorkflowEndpoint {
                            node: string_arg(&call.args, 2, &method, path)?,
                            port: string_arg(&call.args, 3, &method, path)?,
                        },
                    });
                    expect_arg_len(&call.args, 4, &method, path)?;
                }
                _ => {
                    return Err(ApiError::InvalidRequest(format!(
                        "unsupported workflow builder method .{method}(...) in {:?}",
                        path
                    )));
                }
            }
            Ok(workflow)
        }
        syn::Expr::Call(call) if is_workflow_constructor(call) => {
            expect_arg_len(&call.args, 1, "workflow", path)?;
            Ok(WorkflowSpec {
                id: string_arg(&call.args, 0, "workflow", path)?,
                version: "0.1.0".to_owned(),
                name: String::new(),
                category: None,
                description: None,
                inputs: Vec::new(),
                outputs: Vec::new(),
                config_schema: serde_json::Value::Null,
                dependencies: Vec::new(),
                models: Vec::new(),
                runtimes: Vec::new(),
                nodes: Vec::new(),
                edges: Vec::new(),
            })
        }
        _ => Err(ApiError::InvalidRequest(format!(
            "unsupported workflow definition expression in {:?}",
            path
        ))),
    }
}

fn push_hf_model_variant(
    workflow: &mut WorkflowSpec,
    requirement_id: String,
    variant_id: String,
    capability: String,
    format: String,
    repo: String,
    file: String,
) {
    let variant = ModelVariant {
        id: variant_id,
        provider: ModelProvider::HuggingFace,
        format,
        repo,
        file: Some(file).filter(|file| !file.is_empty()),
    };
    if let Some(requirement) = workflow
        .models
        .iter_mut()
        .find(|requirement| requirement.id == requirement_id)
    {
        requirement.variants.push(variant);
    } else {
        workflow.models.push(ModelRequirement {
            id: requirement_id,
            capability,
            variants: vec![variant],
        });
    }
}

fn is_workflow_constructor(call: &syn::ExprCall) -> bool {
    match call.func.as_ref() {
        syn::Expr::Path(path) => path
            .path
            .segments
            .last()
            .is_some_and(|segment| segment.ident == "workflow"),
        _ => false,
    }
}
