use lightflow::api::{ApiError, ApiService};
use lightflow::component::ComponentSpec;
use lightflow::server;
use lightflow::workflow::WorkflowSpec;
use serde::Serialize;
use std::env;
use std::io::{self, Read};

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
    let service = ApiService::new(env::current_dir()?);

    match command {
        "components" => {
            let action = required_arg(args, 0, "component action")?;
            match action {
                "list" => {
                    ensure_no_extra_args(args, 1, "components list")?;
                    print_json(&service.list_components()?)?;
                }
                "get" => {
                    let component_id = required_arg(args, 1, "component id")?;
                    ensure_no_extra_args(args, 2, "components get")?;
                    print_json(&service.get_component(component_id)?)?;
                }
                "save" => {
                    let component: ComponentSpec = serde_json::from_value(request_json(
                        required_arg(args, 1, "component json")?,
                    )?)?;
                    ensure_no_extra_args(args, 2, "components save")?;
                    print_json(&service.save_component(component)?)?;
                }
                _ => {
                    return Err(CliError::Usage(
                        "component action must be list|get|save".to_owned(),
                    ));
                }
            }
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
                        "workflow action must be list|get|validate|save".to_owned(),
                    ));
                }
            }
        }
        "serve" => {
            let bind = parse_bind_addr(args)?;
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

fn parse_bind_addr(args: &[String]) -> CliResult<String> {
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
                    "unexpected argument for serve: {flag}"
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
        "  lightflow components list",
        "  lightflow components get <component_id>",
        "  lightflow components save <json|-|@file>",
        "  lightflow workflows list",
        "  lightflow workflows get <workflow_id>",
        "  lightflow workflows validate <json|-|@file>",
        "  lightflow workflows save <json|-|@file>",
        "  lightflow serve [--host <host>] [--port <port>]",
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
