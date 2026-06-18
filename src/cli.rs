use crate::api::{ApiError, ApiService};
use crate::server;
use crate::workflow::{WorkflowArtifact, WorkflowExecution, WorkflowSpec};
use serde::Serialize;
use serde_json::json;
use std::env;
use std::io::{self, Read};
use std::path::Path;
use std::process::Command;

mod add;
mod batch;
mod history;
mod install;
mod list;
pub mod mcp;
mod models;
mod patches;
mod project;
mod publish;
mod run;
mod runtime;
mod sync;
mod upgrade;

use add::{add_dependency, parse_add_dependency_options};
use batch::{execute_batch, parse_batch_options};
use history::{
    manage_runs, now_ms, parse_replay_run_id, read_manifest, record_failed_run, record_run,
    trace_run,
};
use install::{install_workflow_repo, parse_install_options};
use list::{list_workflows, parse_list_options};
use models::manage_models;
use patches::manage_patches;
use project::{
    InitMode, add_workflow, init_plugin_project, init_workflow_project, normalize_workflow_id,
    parse_add_workflow_options, parse_init_options,
};
use publish::{parse_publish_options, publish_crate};
use run::{RunOptions, lfx_usage, parse_run_options};
use runtime::{RuntimeConfig, ensure_lfw_shell_setup};
use sync::{parse_sync_options, sync_project};
use upgrade::{
    cargo_workspace_root, parse_cargo_workspace_options, update_index, upgrade_workspace,
};

/// Run the LightFlow CLI from process arguments.
pub async fn run_from_env() -> CliResult<()> {
    run(env::args().skip(1).collect()).await
}

/// Run the quick workflow executor from process arguments.
pub async fn run_lfx_from_env() -> CliResult<()> {
    run_lfx(env::args().skip(1).collect()).await
}

/// Run the quick workflow executor with explicit arguments.
pub async fn run_lfx(args: Vec<String>) -> CliResult<()> {
    if args
        .first()
        .is_some_and(|arg| matches!(arg.as_str(), "-h" | "--help" | "help"))
    {
        return Err(CliError::Usage(lfx_usage()));
    }
    let runtime = RuntimeConfig::load()?;
    let service =
        ApiService::new(env::current_dir()?).with_workflow_paths(runtime.workflow_paths.clone());
    let options = parse_run_options(service.repo_root(), &args)?;
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
        "install" => {
            let options = parse_install_options(args)?;
            let (root, repo_store_root) = if options.global {
                ensure_lfw_shell_setup(&runtime)?;
                let store = runtime.home_path.join("repos");
                (runtime.home_path.as_path(), store)
            } else {
                let cwd = env::current_dir()?;
                (Path::new("."), cwd.join(".lightflow").join("repos"))
            };
            print_json(&install_workflow_repo(root, &repo_store_root, &options)?)?;
        }
        "home" => {
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
        "list" | "ls" => {
            let options = parse_list_options(args)?;
            print_json(&list_workflows(&service, &options)?)?;
        }
        "workflows" => {
            let action = required_arg(args, 0, "workflow action")?;
            match action {
                "list" => {
                    ensure_no_extra_args(args, 1, "workflows list")?;
                    print_json(&service.list_workflows()?)?;
                }
                "get" => {
                    let workflow_id = required_arg(args, 1, "workflow id")?;
                    ensure_no_extra_args(args, 2, "workflows get")?;
                    print_json(&service.get_workflow(workflow_id)?)?;
                }
                "deps" | "dependencies" => {
                    let workflow_id = required_arg(args, 1, "workflow id")?;
                    ensure_no_extra_args(args, 2, "workflows deps")?;
                    print_json(&service.workflow_dependencies(workflow_id)?)?;
                }
                "validate" => {
                    let workflow: WorkflowSpec = serde_json::from_value(request_json(
                        required_arg(args, 1, "workflow json")?,
                    )?)?;
                    ensure_no_extra_args(args, 2, "workflows validate")?;
                    print_json(&service.validate_workflow(&workflow))?;
                }
                "save" => {
                    let workflow: WorkflowSpec = serde_json::from_value(request_json(
                        required_arg(args, 1, "workflow json")?,
                    )?)?;
                    ensure_no_extra_args(args, 2, "workflows save")?;
                    print_json(&service.save_workflow(workflow)?)?;
                }
                _ => {
                    return Err(CliError::Usage(
                        "workflow action must be list|get|deps|validate|save".to_owned(),
                    ));
                }
            }
        }
        "deps" | "dependencies" => {
            let workflow_id = required_arg(args, 0, "workflow id")?;
            ensure_no_extra_args(args, 1, "deps")?;
            print_json(&service.workflow_dependencies(workflow_id)?)?;
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
            print_json(&manage_models(args)?)?;
        }
        "mcp" => {
            print_json(&mcp::execute_mcp_request(&service, args)?)?;
        }
        "batch" => {
            let options = parse_batch_options(args)?;
            print_json(&execute_batch(&service, &options)?)?;
        }
        "trace" => {
            print_json(&trace_run(service.repo_root(), args)?)?;
        }
        "runs" => {
            print_json(&manage_runs(service.repo_root(), args)?)?;
        }
        "patch" | "patches" => {
            print_json(&manage_patches(service.repo_root(), args)?)?;
        }
        "replay" => {
            let run_id = parse_replay_run_id(args)?;
            let manifest = read_manifest(service.repo_root(), run_id)?;
            print_json(&execute_and_record_run_options(
                &service,
                RunOptions {
                    stages: manifest.stages,
                },
            )?)?;
        }
        "publish" => {
            let options = parse_publish_options(args)?;
            print_json(&publish_crate(Path::new("."), &options)?)?;
        }
        "run" => {
            let options = parse_run_options(service.repo_root(), args)?;
            print_json(&execute_and_record_run_options(&service, options)?)?;
        }
        "serve" => {
            let bind = parse_bind_addr(args, command)?;
            server::serve(service, &bind).await?;
        }
        "-h" | "--help" | "help" => return Err(CliError::Usage(usage())),
        _ => return Err(CliError::Usage(usage())),
    }

    Ok(())
}

fn print_json(value: &impl Serialize) -> CliResult<()> {
    let stdout = io::stdout();
    let mut handle = stdout.lock();
    serde_json::to_writer_pretty(&mut handle, value)?;
    println!();
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
enum RunOutput {
    Single(WorkflowExecution),
    Pipeline(PipelineExecution),
}

#[derive(Debug, Clone, PartialEq, Serialize)]
struct PipelineExecution {
    pipeline: bool,
    stages: Vec<WorkflowExecution>,
    outputs: serde_json::Map<String, serde_json::Value>,
    artifacts: Vec<WorkflowArtifact>,
}

fn execute_and_record_run_options(
    service: &ApiService,
    options: RunOptions,
) -> CliResult<serde_json::Value> {
    let started_at_ms = now_ms();
    let output = match execute_run_options(service, options.clone()) {
        Ok(output) => output,
        Err(error) => {
            let completed_at_ms = now_ms();
            let error_json = json!({
                "message": error.to_string(),
            });
            let history = record_failed_run(
                service.repo_root(),
                &options,
                &error_json,
                started_at_ms,
                completed_at_ms,
            )?;
            return Err(CliError::Usage(format!(
                "{}\nrun_id: {}\ntrace_path: {}",
                error,
                history.run_id,
                history.run_dir.join("execution.json").display()
            )));
        }
    };
    let completed_at_ms = now_ms();
    let history = record_run(
        service.repo_root(),
        &options,
        &output,
        started_at_ms,
        completed_at_ms,
    )?;
    let mut value = serde_json::to_value(&output)?;
    let Some(object) = value.as_object_mut() else {
        return Err(CliError::Usage(
            "workflow execution output must be a JSON object".to_owned(),
        ));
    };
    object.insert("run_id".to_owned(), history.run_id.into());
    object.insert(
        "run_dir".to_owned(),
        history.run_dir.display().to_string().into(),
    );
    object.insert(
        "trace_path".to_owned(),
        history
            .run_dir
            .join("execution.json")
            .display()
            .to_string()
            .into(),
    );
    Ok(value)
}

fn execute_run_options(service: &ApiService, options: RunOptions) -> CliResult<RunOutput> {
    let mut previous_outputs = serde_json::Map::new();
    let mut executions = Vec::new();
    let mut artifacts = Vec::new();
    let stage_count = options.stages.len();

    for (index, mut stage) in options.stages.into_iter().enumerate() {
        if index > 0 {
            let explicit_inputs = std::mem::take(&mut stage.execution.inputs);
            stage.execution.inputs = previous_outputs.clone();
            stage.execution.inputs.extend(explicit_inputs);
        }
        let execution = service.execute_workflow(&stage.workflow_id, stage.execution)?;
        previous_outputs = execution.outputs.clone();
        artifacts.extend(execution.artifacts.clone());
        executions.push(execution);
    }

    if stage_count == 1 {
        let execution = executions
            .pop()
            .ok_or_else(|| CliError::Usage("missing workflow id".to_owned()))?;
        return Ok(RunOutput::Single(execution));
    }

    Ok(RunOutput::Pipeline(PipelineExecution {
        pipeline: true,
        stages: executions,
        outputs: previous_outputs,
        artifacts,
    }))
}

fn request_body(argument: &str) -> CliResult<String> {
    if argument == "-" {
        let mut body = String::new();
        io::stdin().read_to_string(&mut body)?;
        return Ok(body);
    }
    if let Some(path) = argument.strip_prefix('@') {
        return std::fs::read_to_string(path).map_err(CliError::from);
    }
    Ok(argument.to_owned())
}

fn request_json(argument: &str) -> CliResult<serde_json::Value> {
    let body = request_body(argument)?;
    serde_json::from_str(&body).map_err(CliError::from)
}

fn required_arg<'a>(args: &'a [String], index: usize, label: &str) -> CliResult<&'a str> {
    args.get(index)
        .map(String::as_str)
        .ok_or_else(|| CliError::Usage(format!("missing {label}")))
}

fn parse_bind_addr(args: &[String], command: &str) -> CliResult<String> {
    let mut host = "127.0.0.1".to_owned();
    let mut port = "5174".to_owned();
    let mut index = 0;
    while index < args.len() {
        let flag = args[index].as_str();
        match flag {
            "--host" => host = required_flag_value(args, index, flag)?.to_owned(),
            "--port" => port = required_flag_value(args, index, flag)?.to_owned(),
            _ => {
                return Err(CliError::Usage(format!(
                    "unexpected argument for {command}: {flag}"
                )));
            }
        }
        index += 2;
    }
    Ok(format!("{host}:{port}"))
}

fn required_flag_value<'a>(args: &'a [String], index: usize, flag: &str) -> CliResult<&'a str> {
    let value = args
        .get(index + 1)
        .map(String::as_str)
        .ok_or_else(|| CliError::Usage(format!("missing value for {flag}")))?;
    if value.starts_with("--") {
        return Err(CliError::Usage(format!("missing value for {flag}")));
    }
    Ok(value)
}

fn ensure_no_extra_args(args: &[String], max_len: usize, command: &str) -> CliResult<()> {
    if let Some(extra) = args.get(max_len) {
        return Err(CliError::Usage(format!(
            "unexpected argument for {command}: {extra}"
        )));
    }
    Ok(())
}

fn usage() -> String {
    [
        "usage:",
        "  lfw init [--workflow|--plugin] [path]",
        "  lfw home",
        "  lfw add <crate_name> [--version <version>] [--path <path>|--git <url>] [--package <package>] [--editable] [--global|-g]",
        "  lfw install <path-or-git-url> [--git] [--name <name>] [--global|-g]",
        "  lfw new <workflow_id> --category <name> [--name <name>] [--global|-g]",
        "  lfw list [--brief|--detail] [--category <name>]",
        "  lfw list --categories",
        "  lfw ls [--brief|--detail] [--category <name>]",
        "  lfw workflows list",
        "  lfw workflows get <workflow_id>",
        "  lfw workflows deps <workflow_id>",
        "  lfw workflows validate <json|-|@file>",
        "  lfw workflows save <json|-|@file>",
        "  lfw deps <workflow_id>",
        "  lfw update [--global|-g]",
        "  lfw upgrade [--global|-g]",
        "  lfw sync [workflow_id] [--model <requirement=variant>] [--hf-model <requirement=format:repo[:file]>] [--hf-url <requirement=url>] [--auto-model|--select-model] [--locked] [--apply]",
        "  lfw models list|download|rm|prune",
        "  lfw mcp [<json|-|@file>]",
        "  lfw batch run <jobs.jsonl> [--workflow <workflow_id>] [--run-id <id>] [--max-gpu-jobs <n|auto>] [--max-cpu-jobs <n|auto>] [--batch-size <n|auto>] [--retries <n>] [--reserve-mem <size>] [--reserve-vram <size>] [--max-load <n>]",
        "  lfw batch resume <run_id> [--max-gpu-jobs <n|auto>]",
        "  lfw trace [last|run_id]",
        "  lfw runs list|get|rm ...",
        "  lfw patch list|get|save|validate|rm ...",
        "  lfw replay <run_id>",
        "  lfw publish [workflow_id|--crate <path>|--workflows] [--apply] [--allow-dirty]",
        "  lfw run <workflow_id> [--input|-i <name=json>] [--inputs <json|-|@file>] [--text <text>] [--image <path>] [--output <path>] [--disable <node>] [--enable <node>] [--patch <json|-|@file|name>] ['|' <workflow_id> ...]",
        "  lfw serve [--host <host>] [--port <port>]",
    ]
    .join("\n")
}

fn run_status(command: &mut Command) -> CliResult<()> {
    let status = command.status()?;
    if status.success() {
        Ok(())
    } else {
        Err(CliError::Usage(format!(
            "command failed with status {status}"
        )))
    }
}

/// CLI result type.
pub type CliResult<T> = Result<T, CliError>;

/// CLI error with stable exit-code mapping.
#[derive(Debug)]
pub enum CliError {
    Usage(String),
    Api(ApiError),
    Io(io::Error),
    Json(serde_json::Error),
}

impl CliError {
    /// Process exit code for this error.
    #[must_use]
    pub const fn exit_code(&self) -> i32 {
        match self {
            Self::Usage(_) | Self::Api(_) => 2,
            Self::Io(_) | Self::Json(_) => 1,
        }
    }
}

impl std::fmt::Display for CliError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Usage(message) => f.write_str(message),
            Self::Api(error) => std::fmt::Display::fmt(error, f),
            Self::Io(error) => std::fmt::Display::fmt(error, f),
            Self::Json(error) => std::fmt::Display::fmt(error, f),
        }
    }
}

impl std::error::Error for CliError {}

impl From<ApiError> for CliError {
    fn from(error: ApiError) -> Self {
        Self::Api(error)
    }
}

impl From<io::Error> for CliError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for CliError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

#[cfg(test)]
mod tests {
    use super::parse_bind_addr;

    #[test]
    fn serve_bind_addr_supports_custom_host_and_port() {
        assert_eq!(parse_bind_addr(&[], "serve").unwrap(), "127.0.0.1:5174");
        assert_eq!(
            parse_bind_addr(
                &[
                    "--host".to_owned(),
                    "0.0.0.0".to_owned(),
                    "--port".to_owned(),
                    "8080".to_owned(),
                ],
                "serve"
            )
            .unwrap(),
            "0.0.0.0:8080"
        );
    }
}
