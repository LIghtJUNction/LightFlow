use super::arguments::{bool_arg, expect_arg_len, string_arg};
use crate::api::workflow_package_identity_from_source;
use crate::api::{ApiError, ApiResult};
use crate::workflow::{
    CargoDependency, CargoDependencySource, ModelProvider, ModelRequirement, ModelVariant,
    RuntimeRequirement, WorkflowCondition, WorkflowDependencyRequirement, WorkflowEdge,
    WorkflowEndpoint, WorkflowNode, WorkflowNodeKind, WorkflowPosition, WorkflowSpec,
};
use std::path::Path;

mod ports;

use super::macro_ports::apply_workflow_macro_ports;
use ports::apply_port_builder_method;

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
    parse_workflow_builder_with_package(expression, path, None)
}

pub(super) fn parse_workflow_builder_with_identity(
    expression: &syn::Expr,
    path: &Path,
    id: &str,
    version: &str,
) -> ApiResult<WorkflowSpec> {
    parse_workflow_builder_with_package(expression, path, Some((id, version)))
}

fn parse_workflow_builder_with_package(
    expression: &syn::Expr,
    path: &Path,
    package_identity: Option<(&str, &str)>,
) -> ApiResult<WorkflowSpec> {
    match expression {
        syn::Expr::MethodCall(call) => {
            let mut workflow =
                parse_workflow_builder_with_package(&call.receiver, path, package_identity)?;
            let method = call.method.to_string();
            if apply_port_builder_method(&mut workflow, &method, &call.args, path)? {
                return Ok(workflow);
            }
            match method.as_str() {
                "build" => expect_arg_len(&call.args, 0, &method, path)?,
                "name" => {
                    workflow.name = string_arg(&call.args, 0, &method, path)?;
                    expect_arg_len(&call.args, 1, &method, path)?;
                }
                "category" => {
                    workflow.category = Some(string_arg(&call.args, 0, &method, path)?);
                    expect_arg_len(&call.args, 1, &method, path)?;
                }
                "description" => {
                    workflow.description = Some(string_arg(&call.args, 0, &method, path)?);
                    expect_arg_len(&call.args, 1, &method, path)?;
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
        syn::Expr::Macro(call) if is_workflow_macro(&call.mac.path) => {
            let (id, version) = match package_identity {
                Some((id, version)) => (id.to_owned(), version.to_owned()),
                None => workflow_package_identity_from_source(path)?,
            };
            let mut workflow = WorkflowSpec {
                id,
                version,
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
            };
            apply_workflow_macro_ports(&mut workflow, call, path)?;
            Ok(workflow)
        }
        _ => Err(ApiError::InvalidRequest(format!(
            "unsupported workflow definition expression in {:?}",
            path
        ))),
    }
}

fn is_workflow_macro(path: &syn::Path) -> bool {
    path.segments
        .last()
        .is_some_and(|segment| segment.ident == "workflow")
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
