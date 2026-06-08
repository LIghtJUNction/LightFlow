use cortex_core::ApiFormat;
use lightflow::api::{ApiError, ApiService, CreateRunRequest};
use lightflow::cortex::{CortexHome, StepId, ThreadId, ToolId};
use lightflow::runs::{RunStore, RuntimeDirs};
use serde::Serialize;
use std::env;
use std::io::{self, Read, Write};

fn main() {
    if let Err(error) = run(env::args().skip(1).collect()) {
        eprintln!("{error}");
        std::process::exit(error.exit_code());
    }
}

fn run(args: Vec<String>) -> CliResult<()> {
    let Some(command) = args.first().map(String::as_str) else {
        return Err(CliError::Usage(usage()));
    };
    let args = &args[1..];
    let service = default_service()?;

    match command {
        "assets" => {
            let kind = required_arg(args, 0, "asset kind")?;
            match kind {
                "workflows" => print_json(&service.list_workflows()?)?,
                "nodes" => print_json(&service.list_nodes()?)?,
                "compositions" => print_json(&service.list_compositions()?)?,
                "models" => print_json(&service.list_models()?)?,
                _ => {
                    return Err(CliError::Usage(
                        "asset kind must be workflows|nodes|compositions|models".to_owned(),
                    ));
                }
            }
        }
        "run" => {
            let action = required_arg(args, 0, "run action")?;
            match action {
                "create" => {
                    let workflow_asset_id = required_arg(args, 1, "workflow asset id")?;
                    let run_id = optional_flag_value(args, "--id");
                    let inputs = optional_flag_value(args, "--inputs")
                        .map(|argument| request_json(&argument))
                        .transpose()?
                        .unwrap_or(serde_json::Value::Null);
                    let manifest = service.create_run(CreateRunRequest {
                        run_id,
                        workflow_asset_id: workflow_asset_id.to_owned(),
                        inputs,
                    })?;
                    print_json(&manifest)?;
                }
                "get" => {
                    let run_id = required_arg(args, 1, "run id")?;
                    print_json(&service.get_run(run_id)?)?;
                }
                "submit" => {
                    let run_id = required_arg(args, 1, "run id")?;
                    let step_id = required_arg(args, 2, "step id")?;
                    let body = args
                        .get(3)
                        .map(String::as_str)
                        .map(request_body)
                        .transpose()?;
                    print_json(&service.submit_step(
                        run_id,
                        step_id,
                        body.as_deref().map(str::as_bytes),
                    )?)?;
                }
                "refresh" => {
                    let run_id = required_arg(args, 1, "run id")?;
                    print_json(&service.refresh_run(run_id)?)?;
                }
                "events" => {
                    let run_id = required_arg(args, 1, "run id")?;
                    print_text(&service.run_events(run_id)?)?;
                }
                "trace" => {
                    let run_id = required_arg(args, 1, "run id")?;
                    print_text(&service.run_trace(run_id)?)?;
                }
                _ => {
                    return Err(CliError::Usage(
                        "run action must be create|get|submit|refresh|events|trace".to_owned(),
                    ));
                }
            }
        }
        "ctx" => {
            let action = required_arg(args, 0, "ctx action")?;
            match action {
                "api" => {
                    let format = required_arg(args, 1, "api format")?
                        .parse::<ApiFormat>()
                        .map_err(|error| CliError::Usage(error.to_string()))?;
                    let step_id = StepId::new(required_arg(args, 2, "step id")?)
                        .map_err(|error| CliError::Usage(error.to_string()))?;
                    print_json(&default_cortex_home().api_exchange(format, step_id))?;
                }
                "tool" => {
                    let tool_id = ToolId::new(required_arg(args, 1, "tool id")?)
                        .map_err(|error| CliError::Usage(error.to_string()))?;
                    let step_id = StepId::new(required_arg(args, 2, "step id")?)
                        .map_err(|error| CliError::Usage(error.to_string()))?;
                    print_json(&default_cortex_home().tool_exchange(tool_id, step_id))?;
                }
                "thread" => {
                    let thread_id = ThreadId::new(required_arg(args, 1, "thread id")?)
                        .map_err(|error| CliError::Usage(error.to_string()))?;
                    let step_id = StepId::new(required_arg(args, 2, "step id")?)
                        .map_err(|error| CliError::Usage(error.to_string()))?;
                    print_json(&default_cortex_home().thread_exchange(thread_id, step_id))?;
                }
                _ => {
                    return Err(CliError::Usage(
                        "ctx action must be api|tool|thread".to_owned(),
                    ));
                }
            }
        }
        "-h" | "--help" | "help" => return Err(CliError::Usage(usage())),
        _ => return Err(CliError::Usage(usage())),
    }

    Ok(())
}

fn default_service() -> CliResult<ApiService> {
    let repo_root = env::current_dir()?;
    let run_store = RunStore::new(RuntimeDirs::from_env());
    match env_cortex_home()? {
        Some(cortex_home) => Ok(ApiService::with_cortex_home(
            repo_root,
            run_store,
            cortex_home,
        )),
        None => Ok(ApiService::new(repo_root, run_store)),
    }
}

fn default_cortex_home() -> CortexHome {
    env_cortex_home()
        .ok()
        .flatten()
        .unwrap_or_else(CortexHome::default_for_current_user)
}

fn env_cortex_home() -> CliResult<Option<CortexHome>> {
    let Some(mount) = env::var_os("LIGHTFLOW_CTX_MOUNT") else {
        return Ok(None);
    };
    let uid = match env::var("LIGHTFLOW_CTX_UID") {
        Ok(value) => value
            .parse::<u32>()
            .map_err(|error| CliError::Usage(format!("invalid LIGHTFLOW_CTX_UID: {error}")))?,
        Err(_) => CortexHome::default_for_current_user().uid(),
    };
    Ok(Some(CortexHome::new(mount, uid)))
}

fn print_json(value: &impl Serialize) -> CliResult<()> {
    let stdout = io::stdout();
    let mut handle = stdout.lock();
    serde_json::to_writer_pretty(&mut handle, value)?;
    println!();
    Ok(())
}

fn print_text(value: &str) -> CliResult<()> {
    let stdout = io::stdout();
    let mut handle = stdout.lock();
    handle.write_all(value.as_bytes())?;
    Ok(())
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

fn optional_flag_value(args: &[String], flag: &str) -> Option<String> {
    args.windows(2)
        .find(|window| window.first().map(String::as_str) == Some(flag))
        .and_then(|window| window.get(1))
        .cloned()
}

fn usage() -> String {
    [
        "usage:",
        "  lightflow assets workflows|nodes|compositions|models",
        "  lightflow run create <workflow_asset_id> [--id <run_id>] [--inputs <json|-|@file>]",
        "  lightflow run get <run_id>",
        "  lightflow run submit <run_id> <step_id> [<json_request_body|-|@file>]",
        "  lightflow run refresh <run_id>",
        "  lightflow run events <run_id>",
        "  lightflow run trace <run_id>",
        "  lightflow ctx api <format> <step_id>",
        "  lightflow ctx tool <tool_id> <step_id>",
        "  lightflow ctx thread <thread_id> <step_id>",
        "",
        "environment:",
        "  LIGHTFLOW_CTX_MOUNT overrides /ctx for tests or sandboxes",
        "  LIGHTFLOW_CTX_UID overrides the CortexFS uid path segment",
    ]
    .join("\n")
}

type CliResult<T> = Result<T, CliError>;

#[derive(Debug)]
enum CliError {
    Usage(String),
    Api(ApiError),
    Io(io::Error),
    Json(serde_json::Error),
}

impl CliError {
    const fn exit_code(&self) -> i32 {
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
