use super::{ApiError, WorkflowSpec};
use serde::Serialize;
use std::io::{self, Read};
use std::process::Command;

mod usage;

use usage::serve_usage;
pub(crate) use usage::{home_usage, usage, workflow_shortcuts_usage, workflows_usage};

pub(crate) fn print_json(value: &impl Serialize) -> CliResult<()> {
    let stdout = io::stdout();
    let mut handle = stdout.lock();
    serde_json::to_writer_pretty(&mut handle, value)?;
    println!();
    Ok(())
}

pub(crate) fn request_body(argument: &str) -> CliResult<String> {
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

pub(crate) fn request_json(argument: &str) -> CliResult<serde_json::Value> {
    let body = request_body(argument)?;
    serde_json::from_str(&body).map_err(CliError::from)
}

pub(crate) fn workflow_json_argument(argument: &str, command: &str) -> CliResult<WorkflowSpec> {
    let value = request_json(argument).map_err(|error| match error {
        CliError::Usage(message) => CliError::Usage(message),
        other => CliError::Usage(format!(
            "invalid workflow JSON for {command}: {other}\n{}",
            workflows_usage()
        )),
    })?;
    serde_json::from_value(value).map_err(|error| {
        CliError::Usage(format!(
            "invalid workflow JSON for {command}: {error}\n{}",
            workflows_usage()
        ))
    })
}

pub(crate) fn required_arg<'a>(
    args: &'a [String],
    index: usize,
    label: &str,
) -> CliResult<&'a str> {
    args.get(index)
        .map(String::as_str)
        .ok_or_else(|| CliError::Usage(format!("missing {label}")))
}

pub(crate) fn required_workflow_id_arg(
    args: &[String],
    index: usize,
    usage: fn() -> String,
) -> CliResult<&str> {
    let Some(value) = args.get(index).map(String::as_str) else {
        return Err(CliError::Usage(usage()));
    };
    if value.starts_with('-') || value == "|" {
        return Err(CliError::Usage(usage()));
    }
    Ok(value)
}

pub(crate) fn required_workflow_json_arg(args: &[String], index: usize) -> CliResult<&str> {
    let Some(value) = args.get(index).map(String::as_str) else {
        return Err(CliError::Usage(workflows_usage()));
    };
    if value.starts_with('-') || value == "|" {
        return Err(CliError::Usage(workflows_usage()));
    }
    Ok(value)
}

pub(crate) fn validate_path_segment(value: &str, label: &str) -> CliResult<()> {
    if value.is_empty()
        || value == "."
        || value == ".."
        || value.contains('/')
        || value.contains('\\')
    {
        return Err(CliError::Usage(format!(
            "invalid {label} path segment: {value}"
        )));
    }
    Ok(())
}

pub(crate) fn parse_bind_addr(args: &[String], command: &str) -> CliResult<String> {
    let mut host = "127.0.0.1".to_owned();
    let mut port = "5174".to_owned();
    let mut index = 0;
    while index < args.len() {
        let flag = args[index].as_str();
        match flag {
            "-h" | "--help" | "help" => return Err(CliError::Usage(serve_usage())),
            "--host" => host = required_serve_flag_value(args, index)?.to_owned(),
            "--port" => port = required_serve_flag_value(args, index)?.to_owned(),
            flag if flag.starts_with('-') => {
                return Err(CliError::Usage(serve_usage()));
            }
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

fn required_serve_flag_value(args: &[String], index: usize) -> CliResult<&str> {
    let Some(value) = args.get(index + 1).map(String::as_str) else {
        return Err(CliError::Usage(serve_usage()));
    };
    if value.starts_with("--") {
        return Err(CliError::Usage(serve_usage()));
    }
    Ok(value)
}

pub(crate) fn ensure_no_extra_args(
    args: &[String],
    max_len: usize,
    command: &str,
) -> CliResult<()> {
    if let Some(extra) = args.get(max_len) {
        return Err(CliError::Usage(format!(
            "unexpected argument for {command}: {extra}"
        )));
    }
    Ok(())
}

pub(crate) fn run_status(command: &mut Command) -> CliResult<()> {
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
