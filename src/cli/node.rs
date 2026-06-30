use super::{CliError, CliResult, ensure_no_extra_args};
use crate::api::{ApiService, agent_skill_issues, executor_registry, workflow_placeholder_issues};
use crate::workflow::{PortSpec, WorkflowSpec};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize)]
struct NodeConformanceReport {
    workflow_id: String,
    valid: bool,
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

pub(super) fn manage_nodes(service: &ApiService, args: &[String]) -> CliResult<serde_json::Value> {
    let action = match args.first().map(String::as_str) {
        Some("-h" | "--help" | "help") | None => {
            return Err(CliError::Usage(node_usage()));
        }
        Some(action) => action,
    };
    match action {
        "test" => {
            if args
                .get(1)
                .is_some_and(|arg| matches!(arg.as_str(), "-h" | "--help" | "help"))
            {
                return Err(CliError::Usage(node_usage()));
            }
            let workflow_id = required_node_workflow_id(args, 1)?;
            ensure_no_extra_args(args, 2, "node test")?;
            let report = node_conformance(service, workflow_id)?;
            let valid = report.valid;
            let value = serde_json::to_value(report)?;
            if valid {
                Ok(value)
            } else {
                Err(CliError::Usage(value.to_string()))
            }
        }
        _ => Err(CliError::Usage(format!(
            "node action must be test\n{}",
            node_usage()
        ))),
    }
}

fn required_node_workflow_id(args: &[String], index: usize) -> CliResult<&str> {
    let Some(value) = args.get(index).map(String::as_str) else {
        return Err(CliError::Usage(node_usage()));
    };
    if value.starts_with('-') || value == "|" {
        return Err(CliError::Usage(node_usage()));
    }
    Ok(value)
}

fn node_usage() -> String {
    [
        "usage:",
        "  lfw node test <workflow_id>",
        "",
        "Runs workflow node conformance checks for developer handoff.",
        "Checks validation, generated help, port schema metadata, placeholder text, model readiness, runtime executor metadata, and colocated agent skill coverage.",
    ]
    .join("\n")
}

fn node_conformance(service: &ApiService, workflow_id: &str) -> CliResult<NodeConformanceReport> {
    let workflow = service.get_workflow(workflow_id)?;
    let mut checks = Vec::new();

    push_validation_check(service, &workflow, &mut checks);
    push_help_check(service, workflow_id, &mut checks);
    push_schema_check(&workflow, &mut checks);
    push_placeholder_check(&workflow, &mut checks);
    push_model_check(&workflow, &mut checks);
    push_runtime_check(&workflow, &mut checks);
    push_skill_check(service.repo_root(), &workflow, &mut checks);

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

fn push_skill_check(root: &Path, workflow: &WorkflowSpec, checks: &mut Vec<NodeConformanceCheck>) {
    let Some(skill_dir) = workflow_skill_dir(root, workflow) else {
        checks.push(NodeConformanceCheck::warning(
            "node.skill",
            "workflow crate could not be located under the current project root; skipped skill check",
        ));
        return;
    };
    let Ok(entries) = fs::read_dir(&skill_dir) else {
        checks.push(NodeConformanceCheck::failed(
            "node.skill",
            format!("missing agent skill directory {}", skill_dir.display()),
        ));
        return;
    };

    let mut checked_any = false;
    for entry in entries.flatten() {
        let skill_path = entry.path().join("SKILL.md");
        if !skill_path.exists() {
            continue;
        }
        checked_any = true;
        match fs::read_to_string(&skill_path) {
            Ok(source) => {
                let issues = agent_skill_issues(&source, &workflow.id);
                if issues.is_empty() {
                    checks.push(NodeConformanceCheck::passed(
                        "node.skill",
                        format!("agent skill found at {}", skill_path.display()),
                    ));
                    return;
                }
                checks.push(NodeConformanceCheck::failed(
                    "node.skill",
                    format!(
                        "agent skill {} is missing: {}",
                        skill_path.display(),
                        issues.join(", ")
                    ),
                ));
            }
            Err(error) => checks.push(NodeConformanceCheck::failed(
                "node.skill",
                format!("failed to read {}: {error}", skill_path.display()),
            )),
        }
    }

    if !checked_any {
        checks.push(NodeConformanceCheck::failed(
            "node.skill",
            format!("no SKILL.md found under {}", skill_dir.display()),
        ));
    }
}

fn workflow_skill_dir(root: &Path, workflow: &WorkflowSpec) -> Option<PathBuf> {
    let category = workflow.category.as_deref()?;
    [
        root.join(".lightflow").join("workflows"),
        root.join("workflows"),
    ]
    .into_iter()
    .map(|collection| {
        collection
            .join(category)
            .join(workflow_crate_dir_name_for_category(
                &workflow.id,
                Some(category),
            ))
    })
    .find(|crate_dir| crate_dir.exists())
    .map(|crate_dir| crate_dir.join(".agent").join("skills"))
}

fn workflow_crate_dir_name_for_category(workflow_id: &str, category: Option<&str>) -> String {
    let without_namespace = workflow_id
        .strip_prefix("lightflow.")
        .unwrap_or(workflow_id);
    let short = category
        .and_then(|category| without_namespace.strip_prefix(&format!("{category}.")))
        .unwrap_or(without_namespace);
    short.replace('.', "_")
}
