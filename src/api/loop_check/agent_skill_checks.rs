use super::check_messages::summarize_messages;
use super::project_workspace_inspection::discover_present_project_workspaces;
use super::workflow_crates::{discover_local_workflow_crates, workflow_id_from_crate};
use super::{ApiResult, LocalLoopCheck};
use crate::api::agent_skill_issues;
use std::fs;
use std::path::{Path, PathBuf};

pub(super) fn push_agent_skill_check(
    root: &Path,
    checks: &mut Vec<LocalLoopCheck>,
) -> ApiResult<()> {
    let crates = discover_agent_skill_workflow_crates(root)?;
    if crates.is_empty() {
        checks.push(LocalLoopCheck::warning(
            "loop.workflow.agent_skills",
            "no local workflow crates found under .lightflow/workflows/*/*, workflows/*/*, or projects/*/workflows/*/*",
        ));
        return Ok(());
    }

    let missing = crates
        .iter()
        .filter_map(
            |crate_ref| match workflow_crate_agent_skill_issue(&crate_ref.path) {
                Ok(Some(issue)) => Some(format!("{} ({issue})", crate_ref.display_path.display())),
                Ok(None) => None,
                Err(error) => Some(format!("{} ({error})", crate_ref.display_path.display())),
            },
        )
        .collect::<Vec<_>>();
    if missing.is_empty() {
        checks.push(
            LocalLoopCheck::passed(
                "loop.workflow.agent_skills",
                "local and linked workflow crates have usable agent skills",
            )
            .count(crates.len()),
        );
    } else {
        checks.push(
            LocalLoopCheck::failed(
                "loop.workflow.agent_skills",
                format!(
                    "workflow crates missing usable agent skills: {}",
                    summarize_messages(&missing, 5)
                ),
            )
            .count(missing.len())
            .details(missing),
        );
    }
    Ok(())
}

#[derive(Debug)]
struct WorkflowCrateRef {
    path: PathBuf,
    display_path: PathBuf,
}

fn discover_agent_skill_workflow_crates(root: &Path) -> ApiResult<Vec<WorkflowCrateRef>> {
    let mut crates = discover_local_workflow_crates(root)?
        .into_iter()
        .map(|path| WorkflowCrateRef {
            display_path: path
                .strip_prefix(root)
                .map(Path::to_path_buf)
                .unwrap_or_else(|_| path.clone()),
            path,
        })
        .collect::<Vec<_>>();

    for workspace in discover_present_project_workspaces(root)? {
        for path in discover_local_workflow_crates(&workspace.root)? {
            let relative = path
                .strip_prefix(&workspace.root)
                .map(Path::to_path_buf)
                .unwrap_or_else(|_| path.clone());
            crates.push(WorkflowCrateRef {
                path,
                display_path: workspace.display_prefix.join(relative),
            });
        }
    }

    crates.sort_by(|left, right| left.display_path.cmp(&right.display_path));
    Ok(crates)
}

fn workflow_crate_agent_skill_issue(crate_dir: &Path) -> ApiResult<Option<String>> {
    let workflow_id = workflow_id_from_crate(crate_dir)?;
    let skills = crate_dir.join(".agent").join("skills");
    let Ok(entries) = fs::read_dir(&skills) else {
        return Ok(Some(format!("missing {}", skills.display())));
    };
    for entry in entries.flatten() {
        let skill_path = entry.path().join("SKILL.md");
        if !skill_path.is_file() {
            continue;
        }
        match fs::read_to_string(&skill_path) {
            Ok(source) => {
                let issues = agent_skill_issues(&source, &workflow_id);
                if issues.is_empty() {
                    return Ok(None);
                }
                return Ok(Some(format!(
                    "agent skill is missing: {}",
                    issues.join(", ")
                )));
            }
            Err(error) => {
                return Ok(Some(format!(
                    "failed to read {}: {error}",
                    skill_path.display()
                )));
            }
        }
    }
    Ok(Some(format!(
        "no SKILL.md found under {}",
        skills.display()
    )))
}
