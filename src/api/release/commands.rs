use std::path::Path;
use std::process::Command;

use super::utils::output_tail;
use super::{ReleaseCheck, ReleaseCheckKind, ReleaseCheckStatus};
use crate::api::{ApiError, ApiResult};

pub(super) fn release_commands(
    workflow_id: &str,
    project: Option<&str>,
) -> Vec<(&'static str, Vec<String>)> {
    let mut project_workspaces_command =
        command_args(["cargo", "run", "--bin", "lfw", "--", "loop", "projects"]);
    let mut dirty_project_workspaces_command = command_args([
        "cargo", "run", "--bin", "lfw", "--", "loop", "projects", "--dirty",
    ]);
    let mut workflow_publish_command = command_args([
        "cargo",
        "run",
        "--bin",
        "lfw",
        "--",
        "publish",
        "--workflows",
        "--require-publishable",
    ]);
    if let Some(project) = project {
        project_workspaces_command.extend(["--project".to_owned(), project.to_owned()]);
        dirty_project_workspaces_command.extend(["--project".to_owned(), project.to_owned()]);
        workflow_publish_command.extend(["--project".to_owned(), project.to_owned()]);
    }
    vec![
        (
            "release.command.fmt",
            command_args(["cargo", "fmt", "--check"]),
        ),
        (
            "release.command.local_workflow_loop",
            command_args(["cargo", "run", "--bin", "lfw", "--", "loop", "check"]),
        ),
        (
            "release.command.selected_workflow_loop",
            selected_workflow_loop_command(workflow_id),
        ),
        (
            "release.command.workflow_change_skills",
            command_args(["cargo", "run", "--bin", "lfw", "--", "loop", "changes"]),
        ),
        (
            "release.command.project_workspaces",
            project_workspaces_command,
        ),
        (
            "release.command.dirty_project_workspaces",
            dirty_project_workspaces_command,
        ),
        (
            "release.command.workflow_publish_ready",
            workflow_publish_command,
        ),
        (
            "release.command.clippy",
            command_args(["cargo", "clippy", "--all-targets", "--", "-D", "warnings"]),
        ),
        ("release.command.test", command_args(["cargo", "test"])),
        (
            "release.command.workflow_skills",
            command_args([
                "cargo",
                "test",
                "--test",
                "standard_nodes",
                "repository_workflow_crates_have_agent_skills",
            ]),
        ),
        (
            "release.command.rig",
            command_args(["cargo", "test", "--features", "rig", "--test", "llm_rig"]),
        ),
        (
            "release.command.flux_native",
            command_args(["cargo", "check", "--features", "flux-native"]),
        ),
    ]
}

pub(super) fn command_args<const N: usize>(args: [&str; N]) -> Vec<String> {
    args.into_iter().map(ToOwned::to_owned).collect()
}

fn selected_workflow_loop_command(workflow_id: &str) -> Vec<String> {
    let mut command = command_args(["cargo", "run", "--bin", "lfw", "--", "loop", "check"]);
    command.push(workflow_id.to_owned());
    command.push("--require-replay".to_owned());
    command
}

pub(super) fn command_check(
    root: &Path,
    id: &'static str,
    command: Vec<String>,
    apply: bool,
) -> ApiResult<ReleaseCheck> {
    if !apply {
        return Ok(ReleaseCheck {
            id,
            kind: ReleaseCheckKind::Command,
            status: ReleaseCheckStatus::Planned,
            message: "command is planned; pass --apply to execute it".to_owned(),
            details: Vec::new(),
            count: Some(1),
            command: Some(command),
            path: None,
            exit_code: None,
            stdout_tail: None,
            stderr_tail: None,
        });
    }

    let Some((program, args)) = command.split_first() else {
        return Err(ApiError::InvalidRequest(
            "release command is empty".to_owned(),
        ));
    };
    let output = Command::new(program)
        .args(args)
        .current_dir(root)
        .output()?;
    let exit_code = output.status.code();
    let stdout_tail = output_tail(&output.stdout);
    let stderr_tail = output_tail(&output.stderr);
    if output.status.success() {
        Ok(ReleaseCheck {
            id,
            kind: ReleaseCheckKind::Command,
            status: ReleaseCheckStatus::Passed,
            message: "command passed".to_owned(),
            details: Vec::new(),
            count: Some(1),
            command: Some(command),
            path: None,
            exit_code,
            stdout_tail,
            stderr_tail,
        })
    } else {
        Ok(ReleaseCheck {
            id,
            kind: ReleaseCheckKind::Command,
            status: ReleaseCheckStatus::Failed,
            message: "command failed".to_owned(),
            details: Vec::new(),
            count: Some(1),
            command: Some(command),
            path: None,
            exit_code,
            stdout_tail,
            stderr_tail,
        })
    }
}

pub(super) fn command_skipped_check(id: &'static str, command: Vec<String>) -> ReleaseCheck {
    ReleaseCheck {
        id,
        kind: ReleaseCheckKind::Command,
        status: ReleaseCheckStatus::Skipped,
        message: "command skipped because an earlier release gate failed".to_owned(),
        details: Vec::new(),
        count: Some(1),
        command: Some(command),
        path: None,
        exit_code: None,
        stdout_tail: None,
        stderr_tail: None,
    }
}
