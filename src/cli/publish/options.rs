use super::super::project::normalize_workflow_id;
use crate::cli::{CliError, CliResult};
use std::path::PathBuf;

#[derive(Debug, Clone, Eq, PartialEq)]
pub(in crate::cli) struct PublishOptions {
    pub(super) target: PublishTarget,
    pub(super) apply: bool,
    pub(super) allow_dirty: bool,
    pub(super) require_publishable: bool,
    pub(super) project: Option<String>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(in crate::cli) enum PublishTarget {
    Root,
    Workflow(String),
    Crate(PathBuf),
    Workflows,
}

pub(in crate::cli) fn parse_publish_options(args: &[String]) -> CliResult<PublishOptions> {
    let mut target = None;
    let mut apply = false;
    let mut allow_dirty = false;
    let mut require_publishable = false;
    let mut project = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "-h" | "--help" | "help" => return Err(CliError::Usage(publish_usage())),
            "--apply" => {
                apply = true;
                index += 1;
            }
            "--workflows" => {
                if target.is_some() {
                    return Err(CliError::Usage(
                        "publish accepts only one target".to_owned(),
                    ));
                }
                target = Some(PublishTarget::Workflows);
                index += 1;
            }
            "--dry-run" => {
                apply = false;
                index += 1;
            }
            "--allow-dirty" => {
                allow_dirty = true;
                index += 1;
            }
            "--require-publishable" => {
                require_publishable = true;
                index += 1;
            }
            "--project" => {
                project = Some(required_publish_flag_value(args, index)?.to_owned());
                index += 2;
            }
            "--crate" => {
                if target.is_some() {
                    return Err(CliError::Usage(
                        "publish accepts only one target".to_owned(),
                    ));
                }
                target = Some(PublishTarget::Crate(PathBuf::from(
                    required_publish_flag_value(args, index)?,
                )));
                index += 2;
            }
            value if value.starts_with("--") => {
                return Err(CliError::Usage(publish_usage()));
            }
            value => {
                if target.is_some() {
                    return Err(CliError::Usage(
                        "publish accepts only one target".to_owned(),
                    ));
                }
                target = Some(PublishTarget::Workflow(normalize_workflow_id(value)));
                index += 1;
            }
        }
    }
    let target = target.unwrap_or(PublishTarget::Root);
    if project.is_some() && !matches!(target, PublishTarget::Workflows) {
        return Err(CliError::Usage(publish_usage()));
    }
    Ok(PublishOptions {
        target,
        apply,
        allow_dirty,
        require_publishable,
        project,
    })
}

fn required_publish_flag_value(args: &[String], index: usize) -> CliResult<&str> {
    let Some(value) = args.get(index + 1).map(String::as_str) else {
        return Err(CliError::Usage(publish_usage()));
    };
    if value.starts_with("--") {
        return Err(CliError::Usage(publish_usage()));
    }
    Ok(value)
}

pub(super) fn publish_usage() -> String {
    [
        "usage:",
        "  lfw publish [workflow_id|--crate <path>|--workflows] [--project <name>] [--apply] [--allow-dirty] [--require-publishable]",
        "",
        "Builds a Cargo publish plan without publishing by default.",
        "--workflows checks workflow crates in dependency order.",
        "--project filters --workflows to one linked project workspace and accepts full names, paths, labels, or lightflow-* short aliases such as std, flux, rig, or custom-tools.",
        "--require-publishable fails when any selected workflow crate still has publish blockers.",
        "--apply runs cargo publish; --allow-dirty forwards Cargo's dirty-worktree allowance.",
    ]
    .join("\n")
}
