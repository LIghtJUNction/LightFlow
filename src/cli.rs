use crate::api::{ApiError, ApiService};
use crate::server;
use crate::workflow::WorkflowSpec;
use serde_json::json;
use std::env;
use std::path::Path;

mod add;
mod artifacts;
mod batch;
mod development;
mod history;
mod import;
mod info;
mod list;
pub(crate) mod loop_check;
pub mod mcp;
mod models;
mod node;
mod node_conformance;
mod patches;
mod project;
mod publish;
mod release;
mod run;
mod run_execution;
mod runtime;
mod support;
mod sync;
mod upgrade;
mod workflow_help;
mod workflows;

use add::{add_dependency, parse_add_dependency_options};
use artifacts::list_artifacts;
use batch::{execute_batch, parse_batch_options};
use development::manage_development;
use history::{manage_runs, parse_replay_run_id, trace_run};
use import::{import_workflow_repo, parse_import_options};
use info::architecture_info;
use list::{list_workflows, parse_list_options};
use loop_check::manage_loop;
use models::manage_models;
use node::manage_nodes;
use patches::manage_patches;
use project::{
    InitMode, add_workflow, init_plugin_project, init_workflow_project, normalize_workflow_id,
    parse_add_workflow_options, parse_init_options,
};
use publish::{parse_publish_options, publish_crate};
use release::{parse_release_options, release_check};
use run::{lfx_usage, parse_run_options, parse_run_options_for_command};
use run_execution::execute_and_record_run_options;
use runtime::{RuntimeConfig, ensure_lfw_shell_setup};
use support::parse_bind_addr;
pub(crate) use support::{
    CliError, CliResult, ensure_no_extra_args, home_usage, print_json, request_json, required_arg,
    run_status, usage, validate_path_segment,
};
use sync::{parse_sync_options, sync_project};
use upgrade::{
    cargo_workspace_root, parse_cargo_workspace_options, update_index, upgrade_workspace,
};
use workflows::{
    workflow_dependencies_shortcut, workflow_help_shortcut, workflow_plan_shortcut,
    workflow_subcommand,
};

/// Run the LightFlow CLI from process arguments.
pub async fn run_from_env() -> CliResult<()> {
    run(env::args().skip(1).collect()).await
}

/// Run the quick workflow executor from process arguments.
pub async fn run_lfx_from_env() -> CliResult<()> {
    let mut args = env::args();
    let command = args
        .next()
        .and_then(|arg| {
            Path::new(&arg)
                .file_name()
                .and_then(|name| name.to_str())
                .map(str::to_owned)
        })
        .unwrap_or_else(|| "lfx".to_owned());
    run_lfx_with_command(command.as_str(), args.collect()).await
}

/// Run the quick workflow executor with explicit arguments.
pub async fn run_lfx(args: Vec<String>) -> CliResult<()> {
    run_lfx_with_command("lfx", args).await
}

async fn run_lfx_with_command(command: &str, args: Vec<String>) -> CliResult<()> {
    if args.is_empty()
        || args
            .first()
            .is_some_and(|arg| matches!(arg.as_str(), "-h" | "--help" | "help"))
    {
        return Err(CliError::Usage(lfx_usage(command)));
    }
    let runtime = RuntimeConfig::load()?;
    let service =
        ApiService::new(env::current_dir()?).with_workflow_paths(runtime.workflow_paths.clone());
    let options = parse_run_options_for_command(service.repo_root(), &args, command)?;
    print_json(&execute_and_record_run_options(&service, options)?)?;
    Ok(())
}

/// Run the LightFlow CLI with explicit arguments.
pub async fn run(args: Vec<String>) -> CliResult<()> {
    let Some(command) = args.first().map(String::as_str) else {
        return Err(CliError::Usage(usage()));
    };
    let args = &args[1..];
    let runtime = RuntimeConfig::load()?;
    let service =
        ApiService::new(env::current_dir()?).with_workflow_paths(runtime.workflow_paths.clone());

    match command {
        "init" => {
            let options = parse_init_options(args)?;
            let output = match options.mode {
                InitMode::Workflow => {
                    let shell_setup = ensure_lfw_shell_setup(&runtime)?;
                    let mut output = init_workflow_project(&options.root)?;
                    output["config"] = json!({
                        "rc": runtime.rc_path,
                        "lfw_path": runtime.lfw_path,
                        "rc_created": shell_setup.rc_created,
                        "workflow_workspace_manifest": shell_setup.workspace_manifest,
                        "workflow_workspace_created": shell_setup.workspace_created,
                        "shell": shell_setup.shell,
                        "shell_config": shell_setup.shell_config,
                        "source_line": shell_setup.source_line,
                        "source_installed": shell_setup.source_installed,
                    });
                    output
                }
                InitMode::Plugin => init_plugin_project(&options.root)?,
            };
            print_json(&output)?;
        }
        "new" => {
            let options = parse_add_workflow_options(args)?;
            let workflow_id = normalize_workflow_id(&options.workflow_id);
            let root = if options.global {
                ensure_lfw_shell_setup(&runtime)?;
                runtime.home_path.as_path()
            } else {
                Path::new(".")
            };
            print_json(&add_workflow(
                root,
                &workflow_id,
                options.name.as_deref(),
                options.category.as_deref(),
                options.runtime.as_deref(),
                options.global,
            )?)?;
        }
        "add" => {
            let options = parse_add_dependency_options(args)?;
            let root = if options.global {
                ensure_lfw_shell_setup(&runtime)?;
                runtime.home_path.as_path()
            } else {
                Path::new(".")
            };
            print_json(&add_dependency(root, &options, options.global)?)?;
        }
        "import" => {
            let options = parse_import_options(args)?;
            let (root, repo_store_root) = if options.global {
                ensure_lfw_shell_setup(&runtime)?;
                let store = runtime.home_path.join("repos");
                (runtime.home_path.as_path(), store)
            } else {
                let cwd = env::current_dir()?;
                (Path::new("."), cwd.join(".lightflow").join("repos"))
            };
            print_json(&import_workflow_repo(root, &repo_store_root, &options)?)?;
        }
        "home" => {
            if args
                .first()
                .is_some_and(|arg| matches!(arg.as_str(), "-h" | "--help" | "help"))
            {
                return Err(CliError::Usage(home_usage()));
            }
            ensure_no_extra_args(args, 0, "home")?;
            let shell_setup = ensure_lfw_shell_setup(&runtime)?;
            print_json(&json!({
                "home": runtime.home_path,
                "manifest": shell_setup.workspace_manifest,
                "workflows": runtime.default_workflow_path,
                "repos": runtime.home_path.join("repos"),
                "lfw_path": runtime.lfw_path,
            }))?;
        }
        "info" | "arch" | "architecture" => {
            print_json(&architecture_info(&service, &runtime, args)?)?;
        }
        "list" | "ls" => {
            let options = parse_list_options(args)?;
            print_json(&list_workflows(&service, &options)?)?;
        }
        "workflows" => {
            print_json(&workflow_subcommand(&service, args)?)?;
        }
        "deps" | "dependencies" => {
            print_json(&workflow_dependencies_shortcut(&service, args)?)?;
        }
        "plan" => {
            print_json(&workflow_plan_shortcut(&service, args)?)?;
        }
        "help" => {
            print_json(&workflow_help_shortcut(&service, args)?)?;
        }
        "sync" => {
            let options = parse_sync_options(args)?;
            print_json(&sync_project(&service, &options)?)?;
        }
        "update" => {
            let options = parse_cargo_workspace_options(args, command)?;
            let root = cargo_workspace_root(&env::current_dir()?, &runtime.home_path, &options);
            print_json(&update_index(&root)?)?;
        }
        "upgrade" => {
            let options = parse_cargo_workspace_options(args, command)?;
            let root = cargo_workspace_root(&env::current_dir()?, &runtime.home_path, &options);
            print_json(&upgrade_workspace(&root)?)?;
        }
        "models" => {
            print_json(&manage_models(&service, args)?)?;
        }
        "node" | "nodes" => {
            print_json(&manage_nodes(&service, args)?)?;
        }
        "mcp" => {
            print_json(&mcp::execute_mcp_request(&service, args)?)?;
        }
        "batch" => {
            let options = parse_batch_options(args)?;
            print_json(&execute_batch(&service, &options)?)?;
        }
        "trace" => {
            print_json(&trace_run(&service, args)?)?;
        }
        "runs" => {
            print_json(&manage_runs(&service, args)?)?;
        }
        "artifact" | "artifacts" => {
            print_json(&list_artifacts(&service, args)?)?;
        }
        "patch" | "patches" => {
            print_json(&manage_patches(&service, args)?)?;
        }
        "replay" => {
            let run_id = parse_replay_run_id(args)?;
            print_json(&service.replay_run_with_surface(run_id, "cli")?)?;
        }
        "publish" => {
            let options = parse_publish_options(args)?;
            print_json(&publish_crate(Path::new("."), &options)?)?;
        }
        "release" => {
            let options = parse_release_options(args)?;
            print_json(&release_check(&service, &options)?)?;
        }
        "dev" | "development" => {
            print_json(&manage_development(&service, args)?)?;
        }
        "loop" => {
            print_json(&manage_loop(&service, args)?)?;
        }
        "run" => {
            let options = parse_run_options(service.repo_root(), args)?;
            print_json(&execute_and_record_run_options(&service, options)?)?;
        }
        "serve" => {
            let bind = parse_bind_addr(args, command)?;
            server::serve(service, &bind).await?;
        }
        "-h" | "--help" => return Err(CliError::Usage(usage())),
        _ => return Err(CliError::Usage(usage())),
    }

    Ok(())
}
