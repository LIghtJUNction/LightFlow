use cortex_core::ApiFormat;
use lightflow::api::{ApiError, ApiService, CreateRunRequest};
use lightflow::cortex::{ChannelId, CortexHome, HookId, JobId, StepId, ThreadId, ToolId};
use lightflow::runs::{RunStore, RuntimeDirs};
use lightflow::server;
use lightflow::stream;
use serde::Serialize;
use std::env;
use std::io::{self, Read, Write};
use std::net::SocketAddr;

#[tokio::main]
async fn main() {
    if let Err(error) = run(env::args().skip(1).collect()).await {
        eprintln!("{error}");
        std::process::exit(error.exit_code());
    }
}

async fn run(args: Vec<String>) -> CliResult<()> {
    let Some(command) = args.first().map(String::as_str) else {
        return Err(CliError::Usage(usage()));
    };
    let args = &args[1..];
    let service = default_service()?;

    match command {
        "assets" => {
            let kind = required_arg(args, 0, "asset kind")?;
            ensure_no_extra_args(args, 1, "assets")?;
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
                    let manifest = service.create_run(parse_create_run_request(args, action)?)?;
                    print_json(&manifest)?;
                }
                "preview" => {
                    let preview = service.preview_run(parse_create_run_request(args, action)?)?;
                    print_json(&preview)?;
                }
                "list" => {
                    ensure_no_extra_args(args, 1, "run list")?;
                    print_json(&service.list_runs()?)?;
                }
                "get" => {
                    let run_id = required_arg(args, 1, "run id")?;
                    ensure_no_extra_args(args, 2, "run get")?;
                    print_json(&service.get_run(run_id)?)?;
                }
                "status" => {
                    let run_id = required_arg(args, 1, "run id")?;
                    ensure_no_extra_args(args, 2, "run status")?;
                    print_json(&service.run_status(run_id)?)?;
                }
                "request" => {
                    let run_id = required_arg(args, 1, "run id")?;
                    ensure_no_extra_args(args, 2, "run request")?;
                    print_json(&service.run_request(run_id)?)?;
                }
                "workflow" => {
                    let run_id = required_arg(args, 1, "run id")?;
                    ensure_no_extra_args(args, 2, "run workflow")?;
                    print_json(&service.run_workflow(run_id)?)?;
                }
                "submit" => {
                    let run_id = required_arg(args, 1, "run id")?;
                    let step_id = required_arg(args, 2, "step id")?;
                    ensure_no_extra_args(args, 4, "run submit")?;
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
                    ensure_no_extra_args(args, 2, "run refresh")?;
                    print_json(&service.refresh_run(run_id)?)?;
                }
                "events" => {
                    let run_id = required_arg(args, 1, "run id")?;
                    ensure_no_extra_args(args, 2, "run events")?;
                    print_text(&service.run_events(run_id)?)?;
                }
                "trace" => {
                    let run_id = required_arg(args, 1, "run id")?;
                    ensure_no_extra_args(args, 2, "run trace")?;
                    print_text(&service.run_trace(run_id)?)?;
                }
                _ => {
                    return Err(CliError::Usage(
                        "run action must be create|preview|list|get|status|request|workflow|submit|refresh|events|trace"
                            .to_owned(),
                    ));
                }
            }
        }
        "ctx" => {
            let action = required_arg(args, 0, "ctx action")?;
            match action {
                "abi" => {
                    ensure_no_extra_args(args, 1, "ctx abi")?;
                    print_json(&service.ctx_abi())?;
                }
                "api" => {
                    let format = required_arg(args, 1, "api format")?
                        .parse::<ApiFormat>()
                        .map_err(|error| CliError::Usage(error.to_string()))?;
                    let step_id = StepId::new(required_arg(args, 2, "step id")?)
                        .map_err(|error| CliError::Usage(error.to_string()))?;
                    ensure_no_extra_args(args, 3, "ctx api")?;
                    print_json(&default_cortex_home().api_exchange(format, step_id))?;
                }
                "tool" => {
                    let tool_id = ToolId::new(required_arg(args, 1, "tool id")?)
                        .map_err(|error| CliError::Usage(error.to_string()))?;
                    let step_id = StepId::new(required_arg(args, 2, "step id")?)
                        .map_err(|error| CliError::Usage(error.to_string()))?;
                    ensure_no_extra_args(args, 3, "ctx tool")?;
                    print_json(&default_cortex_home().tool_exchange(tool_id, step_id))?;
                }
                "thread" => {
                    let thread_id = ThreadId::new(required_arg(args, 1, "thread id")?)
                        .map_err(|error| CliError::Usage(error.to_string()))?;
                    let step_id = StepId::new(required_arg(args, 2, "step id")?)
                        .map_err(|error| CliError::Usage(error.to_string()))?;
                    ensure_no_extra_args(args, 3, "ctx thread")?;
                    print_json(&default_cortex_home().thread_exchange(thread_id, step_id))?;
                }
                "chan" => {
                    let channel_id = ChannelId::new(required_arg(args, 1, "channel id")?)
                        .map_err(|error| CliError::Usage(error.to_string()))?;
                    ensure_no_extra_args(args, 2, "ctx chan")?;
                    print_json(&default_cortex_home().channel_paths(channel_id))?;
                }
                "job" => {
                    let job_id = JobId::new(required_arg(args, 1, "job id")?)
                        .map_err(|error| CliError::Usage(error.to_string()))?;
                    ensure_no_extra_args(args, 2, "ctx job")?;
                    print_json(&default_cortex_home().job_exchange(job_id))?;
                }
                "hook" => {
                    let hook_id = HookId::new(required_arg(args, 1, "hook id")?)
                        .map_err(|error| CliError::Usage(error.to_string()))?;
                    ensure_no_extra_args(args, 2, "ctx hook")?;
                    print_json(&default_cortex_home().hook_exchange(hook_id))?;
                }
                _ => {
                    return Err(CliError::Usage(
                        "ctx action must be abi|api|tool|thread|chan|job|hook".to_owned(),
                    ));
                }
            }
        }
        "serve" => {
            let bind = parse_bind_addr(args)?;
            server::serve(service, &bind).await?;
        }
        "stream" => {
            let action = required_arg(args, 0, "stream action")?;
            match action {
                "info" => {
                    ensure_no_extra_args(args, 1, "stream info")?;
                    print_json(&stream::stream_info())?;
                }
                "schema" => {
                    ensure_no_extra_args(args, 1, "stream schema")?;
                    print_text(stream::SCHEMA)?;
                }
                "snapshot" => {
                    let run_id = required_arg(args, 1, "run id")?;
                    ensure_no_extra_args(args, 2, "stream snapshot")?;
                    print_bytes(&stream::encode_run_snapshot(&service, run_id)?)?;
                }
                "serve-webtransport" => {
                    let bind = parse_bind_addr_with_default(
                        &args[1..],
                        "4433",
                        "stream serve-webtransport",
                    )?;
                    let bind = bind.parse::<SocketAddr>().map_err(|error| {
                        CliError::Usage(format!(
                            "invalid WebTransport bind address {bind}: {error}"
                        ))
                    })?;
                    stream::serve_webtransport(service, bind).await?;
                }
                _ => {
                    return Err(CliError::Usage(
                        "stream action must be info|schema|snapshot|serve-webtransport".to_owned(),
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

fn print_bytes(value: &[u8]) -> CliResult<()> {
    let stdout = io::stdout();
    let mut handle = stdout.lock();
    handle.write_all(value)?;
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

fn parse_create_run_request(args: &[String], action: &str) -> CliResult<CreateRunRequest> {
    let workflow_asset_id = required_arg(args, 1, "workflow asset id")?.to_owned();
    let mut run_id = None;
    let mut inputs = None;
    let mut index = 2;
    while index < args.len() {
        let flag = args[index].as_str();
        match flag {
            "--id" => {
                set_flag_once(
                    &mut run_id,
                    flag,
                    required_flag_value(args, index, flag)?.to_owned(),
                )?;
            }
            "--inputs" => {
                let value = request_json(required_flag_value(args, index, flag)?)?;
                set_flag_once(&mut inputs, flag, value)?;
            }
            _ => {
                return Err(CliError::Usage(format!(
                    "unexpected argument for run {action}: {flag}"
                )));
            }
        }
        index += 2;
    }

    Ok(CreateRunRequest {
        run_id,
        workflow_asset_id,
        inputs: inputs.unwrap_or(serde_json::Value::Null),
    })
}

fn required_arg<'a>(args: &'a [String], index: usize, label: &str) -> CliResult<&'a str> {
    args.get(index)
        .map(String::as_str)
        .ok_or_else(|| CliError::Usage(format!("missing {label}")))
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

fn set_flag_once<T>(slot: &mut Option<T>, flag: &str, value: T) -> CliResult<()> {
    if slot.is_some() {
        return Err(CliError::Usage(format!("duplicate flag {flag}")));
    }
    *slot = Some(value);
    Ok(())
}

fn parse_bind_addr(args: &[String]) -> CliResult<String> {
    parse_bind_addr_with_default(args, "5174", "serve")
}

fn parse_bind_addr_with_default(
    args: &[String],
    default_port: &str,
    command: &str,
) -> CliResult<String> {
    let mut host = "127.0.0.1".to_owned();
    let mut port = default_port.to_owned();
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
        "  lightflow assets workflows|nodes|compositions|models",
        "  lightflow run create <workflow_asset_id> [--id <run_id>] [--inputs <json|-|@file>]",
        "  lightflow run preview <workflow_asset_id> [--id <run_id>] [--inputs <json|-|@file>]",
        "  lightflow run list",
        "  lightflow run get <run_id>",
        "  lightflow run status <run_id>",
        "  lightflow run request <run_id>",
        "  lightflow run workflow <run_id>",
        "  lightflow run submit <run_id> <step_id> [<json_request_body|-|@file>]",
        "  lightflow run refresh <run_id>",
        "  lightflow run events <run_id>",
        "  lightflow run trace <run_id>",
        "  lightflow ctx abi",
        "  lightflow ctx api <format> <step_id>",
        "  lightflow ctx tool <tool_id> <step_id>",
        "  lightflow ctx thread <thread_id> <step_id>",
        "  lightflow ctx chan <channel_id>",
        "  lightflow ctx job <job_id>",
        "  lightflow ctx hook <hook_id>",
        "  lightflow serve [--host <host>] [--port <port>]",
        "  lightflow stream info",
        "  lightflow stream schema",
        "  lightflow stream snapshot <run_id>",
        "  lightflow stream serve-webtransport [--host <host>] [--port <port>]",
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
