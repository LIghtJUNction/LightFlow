use super::{ApiService, LocalLoopCheck, LocalLoopStatus};
use crate::api::history::RunSummary;

pub(super) fn loop_check_messages(
    checks: &[LocalLoopCheck],
    status: LocalLoopStatus,
) -> Vec<String> {
    checks
        .iter()
        .filter(|check| check.status == status)
        .map(|check| format!("{}: {}", check.id, check.message))
        .collect()
}

pub(super) fn next_commands(
    command_workflow_id: &str,
    replay_selector: &str,
    selected_workflow_id: Option<&str>,
    selected_has_dependency_graph: bool,
) -> Vec<Vec<String>> {
    let publish_command = if let Some(workflow_id) = selected_workflow_id {
        vec!["lfw", "publish", workflow_id]
    } else {
        vec!["lfw", "publish", "--workflows"]
    };
    let mut commands = vec![
        vec!["lfw", "list"],
        vec!["lfw", "node", "test", command_workflow_id],
        vec!["lfw", "plan", command_workflow_id],
        vec![
            "lfw",
            "models",
            "requirements",
            command_workflow_id,
            "--blocked",
        ],
        vec![
            "lfw",
            "sync",
            command_workflow_id,
            "--auto-model",
            "--apply",
        ],
        vec!["lfw", "run", command_workflow_id, "--inputs", "@input.json"],
        vec!["lfw", "trace", replay_selector],
        vec!["lfw", "replay", replay_selector],
        vec!["lfw", "loop", "changes"],
        vec!["lfw", "loop", "projects"],
        vec!["lfw", "loop", "projects", "--dirty"],
    ];
    commands.push(publish_command);
    if selected_has_dependency_graph {
        commands.push(vec!["lfw", "publish", "--workflows"]);
    }
    commands
        .into_iter()
        .map(|command| command.into_iter().map(ToOwned::to_owned).collect())
        .collect()
}

pub(super) fn latest_completed_run_id(service: &ApiService, workflow_id: &str) -> Option<String> {
    service
        .list_runs()
        .ok()?
        .runs
        .into_iter()
        .find(|run| run_includes_workflow(run, workflow_id) && run.status == "completed")
        .map(|run| run.run_id)
}

pub(super) fn run_includes_workflow(run: &RunSummary, workflow_id: &str) -> bool {
    run.workflow_ids
        .iter()
        .any(|recorded_workflow_id| recorded_workflow_id == workflow_id)
}
