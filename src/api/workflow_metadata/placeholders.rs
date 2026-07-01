use crate::workflow::{PortSpec, WorkflowSpec};

pub(crate) fn workflow_placeholder_issues(workflow: &WorkflowSpec) -> Vec<String> {
    let mut issues = Vec::new();
    if unresolved_placeholder(workflow.description.as_deref()) {
        issues.push("workflow.description contains unresolved TODO".to_owned());
    }
    collect_port_placeholder_issues("input", &workflow.inputs, &mut issues);
    collect_port_placeholder_issues("output", &workflow.outputs, &mut issues);
    issues
}

fn collect_port_placeholder_issues(kind: &str, ports: &[PortSpec], issues: &mut Vec<String>) {
    for port in ports {
        if unresolved_placeholder(port.description.as_deref()) {
            issues.push(format!(
                "workflow.{kind}.{}.description contains unresolved TODO",
                port.name
            ));
        }
    }
}

fn unresolved_placeholder(value: Option<&str>) -> bool {
    value.is_some_and(|value| value.to_ascii_lowercase().contains("todo"))
}
