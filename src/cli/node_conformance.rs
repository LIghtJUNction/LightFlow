use super::CliResult;
use crate::api::{ApiService, executor_registry, workflow_placeholder_issues};
use crate::workflow::{PortSpec, WorkflowSpec};
use serde::Serialize;

mod skills;

#[derive(Debug, Clone, Serialize)]
pub(super) struct NodeConformanceReport {
    workflow_id: String,
    pub(super) valid: bool,
    checks: Vec<NodeConformanceCheck>,
}

#[derive(Debug, Clone, Serialize)]
struct NodeConformanceCheck {
    id: &'static str,
    status: CheckStatus,
    message: String,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum CheckStatus {
    Passed,
    Warning,
    Failed,
}

impl NodeConformanceCheck {
    fn passed(id: &'static str, message: impl Into<String>) -> Self {
        Self {
            id,
            status: CheckStatus::Passed,
            message: message.into(),
        }
    }

    fn warning(id: &'static str, message: impl Into<String>) -> Self {
        Self {
            id,
            status: CheckStatus::Warning,
            message: message.into(),
        }
    }

    fn failed(id: &'static str, message: impl Into<String>) -> Self {
        Self {
            id,
            status: CheckStatus::Failed,
            message: message.into(),
        }
    }
}

pub(super) fn node_conformance(
    service: &ApiService,
    workflow_id: &str,
) -> CliResult<NodeConformanceReport> {
    let workflow = service.get_workflow(workflow_id)?;
    let mut checks = Vec::new();

    push_validation_check(service, &workflow, &mut checks);
    push_help_check(service, workflow_id, &mut checks);
    push_schema_check(&workflow, &mut checks);
    push_placeholder_check(&workflow, &mut checks);
    push_model_check(&workflow, &mut checks);
    push_runtime_check(&workflow, &mut checks);
    skills::push_skill_check(service.repo_root(), &workflow, &mut checks);

    let valid = !checks
        .iter()
        .any(|check| check.status == CheckStatus::Failed);
    Ok(NodeConformanceReport {
        workflow_id: workflow.id,
        valid,
        checks,
    })
}

fn push_validation_check(
    service: &ApiService,
    workflow: &WorkflowSpec,
    checks: &mut Vec<NodeConformanceCheck>,
) {
    let validation = service.validate_workflow(workflow);
    if validation.valid {
        checks.push(NodeConformanceCheck::passed(
            "workflow.validation",
            "workflow validates against discovered dependencies",
        ));
    } else {
        checks.push(NodeConformanceCheck::failed(
            "workflow.validation",
            format!(
                "workflow validation failed: {}",
                validation.issues.join("; ")
            ),
        ));
    }
}

fn push_help_check(
    service: &ApiService,
    workflow_id: &str,
    checks: &mut Vec<NodeConformanceCheck>,
) {
    match super::workflow_help::workflow_help(service, &[workflow_id.to_owned()], "node test help")
    {
        Ok(_) => checks.push(NodeConformanceCheck::passed(
            "workflow.help",
            "lfw help contract can be generated",
        )),
        Err(error) => checks.push(NodeConformanceCheck::failed(
            "workflow.help",
            format!("lfw help contract failed: {error}"),
        )),
    }
}

fn push_schema_check(workflow: &WorkflowSpec, checks: &mut Vec<NodeConformanceCheck>) {
    let mut issues = Vec::new();
    if workflow.inputs.is_empty() {
        issues.push("workflow has no input ports".to_owned());
    }
    if workflow.outputs.is_empty() {
        issues.push("workflow has no output ports".to_owned());
    }
    for port in workflow.inputs.iter().chain(workflow.outputs.iter()) {
        push_port_schema_issues(port, &mut issues);
    }
    if issues.is_empty() {
        checks.push(NodeConformanceCheck::passed(
            "node.schema",
            "inputs and outputs include usable Node Schema metadata",
        ));
    } else {
        checks.push(NodeConformanceCheck::failed(
            "node.schema",
            issues.join("; "),
        ));
    }
}

fn push_port_schema_issues(port: &PortSpec, issues: &mut Vec<String>) {
    if port.name.trim().is_empty() {
        issues.push("port has an empty name".to_owned());
    }
    if port.ty.trim().is_empty() {
        issues.push(format!("port {} has an empty type", port.name));
    }
    if port.description.as_deref().unwrap_or("").trim().is_empty() {
        issues.push(format!("port {} is missing description", port.name));
    }
    if port.ty == "artifact" && port.artifact_kind.is_none() {
        issues.push(format!(
            "artifact port {} is missing artifact_kind",
            port.name
        ));
    }
    if port.required.unwrap_or(false) && port.default.is_some() {
        issues.push(format!(
            "required input {} should not also declare a default",
            port.name
        ));
    }
}

fn push_placeholder_check(workflow: &WorkflowSpec, checks: &mut Vec<NodeConformanceCheck>) {
    let issues = workflow_placeholder_issues(workflow);

    if issues.is_empty() {
        checks.push(NodeConformanceCheck::passed(
            "node.placeholders",
            "workflow metadata has no generated TODO placeholders",
        ));
    } else {
        checks.push(NodeConformanceCheck::warning(
            "node.placeholders",
            format!(
                "replace generated placeholders before publishing: {}",
                issues.join("; ")
            ),
        ));
    }
}

fn push_model_check(workflow: &WorkflowSpec, checks: &mut Vec<NodeConformanceCheck>) {
    let model_ids = workflow
        .models
        .iter()
        .map(|model| model.id.as_str())
        .collect::<Vec<_>>();
    let mut issues = Vec::new();
    for model in &workflow.models {
        if model.id.trim().is_empty() {
            issues.push("model requirement has an empty id".to_owned());
        }
        if model.capability.trim().is_empty() {
            issues.push(format!(
                "model requirement {} has an empty capability",
                model.id
            ));
        }
    }
    for port in workflow.inputs.iter().chain(workflow.outputs.iter()) {
        if let Some(requirement) = &port.model_requirement
            && !model_ids.iter().any(|id| id == requirement)
        {
            issues.push(format!(
                "port {} references missing model requirement {}",
                port.name, requirement
            ));
        }
    }

    if issues.is_empty() {
        let message = if workflow.models.is_empty() {
            "workflow declares no model requirements".to_owned()
        } else {
            "model requirements and port bindings are consistent".to_owned()
        };
        checks.push(NodeConformanceCheck::passed("node.models", message));
    } else {
        checks.push(NodeConformanceCheck::failed(
            "node.models",
            issues.join("; "),
        ));
    }
}

fn push_runtime_check(workflow: &WorkflowSpec, checks: &mut Vec<NodeConformanceCheck>) {
    if workflow.runtimes.is_empty() {
        checks.push(NodeConformanceCheck::passed(
            "node.runtime",
            "workflow uses passthrough execution and declares no runtime capability",
        ));
        return;
    }

    let executors = executor_registry();
    let mut issues = Vec::new();
    for runtime in &workflow.runtimes {
        let matching = executors
            .iter()
            .filter(|executor| {
                executor
                    .capabilities
                    .iter()
                    .any(|capability| capability == &runtime.capability)
            })
            .collect::<Vec<_>>();
        if matching.is_empty() {
            issues.push(format!(
                "runtime capability {} has no registered executor",
                runtime.capability
            ));
            continue;
        }
        if !matching.iter().any(|executor| executor.available) {
            issues.push(format!(
                "runtime capability {} has registered executors but none are currently available",
                runtime.capability
            ));
        }
    }

    if issues.is_empty() {
        checks.push(NodeConformanceCheck::passed(
            "node.runtime",
            "runtime capabilities have available executors",
        ));
    } else {
        checks.push(NodeConformanceCheck::failed(
            "node.runtime",
            issues.join("; "),
        ));
    }
}
