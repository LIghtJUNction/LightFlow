use super::NodeConformanceCheck;
use crate::api::{agent_skill_issues, workflow_package_identity};
use crate::workflow::WorkflowSpec;
use std::fs;
use std::path::{Path, PathBuf};

pub(super) fn push_skill_check(
    root: &Path,
    workflow: &WorkflowSpec,
    checks: &mut Vec<NodeConformanceCheck>,
) {
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
            Ok(source) => push_skill_source_check(workflow, checks, &skill_path, &source),
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

fn push_skill_source_check(
    workflow: &WorkflowSpec,
    checks: &mut Vec<NodeConformanceCheck>,
    skill_path: &Path,
    source: &str,
) {
    let issues = agent_skill_issues(source, &workflow.id);
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

fn workflow_skill_dir(root: &Path, workflow: &WorkflowSpec) -> Option<PathBuf> {
    [
        root.join(".lightflow").join("workflows"),
        root.join("workflows"),
    ]
    .into_iter()
    .filter_map(|collection| fs::read_dir(collection).ok())
    .flat_map(|entries| entries.filter_map(Result::ok).map(|entry| entry.path()))
    .find(|crate_dir| {
        workflow_package_identity(&crate_dir.join("Cargo.toml"))
            .is_ok_and(|(workflow_id, _)| workflow_id == workflow.id)
    })
    .map(|crate_dir| crate_dir.join(".agent").join("skills"))
}
