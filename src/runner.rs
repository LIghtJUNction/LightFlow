//! Helpers and the stable JSON-over-stdio contract for executable workflow crates.

use crate::api::ApiService;
use crate::workflow::{Runnable, WorkflowArtifact, WorkflowExecutionOptions, WorkflowSpec};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::env;
use std::error::Error as StdError;
use std::fmt;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

/// Result type used by executable workflow entrypoints.
pub type RunnerResult<T> = Result<T, Box<dyn StdError>>;

/// Protocol identifier exchanged by LightFlow and workflow runners.
pub const PROTOCOL: &str = "lightflow.runner.v1";

/// Identity of the workflow selected for one runner invocation.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct WorkflowIdentity {
    pub id: String,
    pub version: String,
}

/// Request sent to a workflow runner.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Request {
    pub protocol: String,
    pub workflow: WorkflowIdentity,
    pub inputs: Map<String, Value>,
    #[serde(default)]
    pub models: BTreeMap<String, ModelBinding>,
}

/// One model requirement resolved from the project's lockfile by the host.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct ModelBinding {
    pub requirement_id: String,
    pub variant_id: String,
    pub path: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot_revision: Option<String>,
}

impl Request {
    /// Reject a request produced for another protocol revision.
    pub fn validate_protocol(&self) -> Result<(), Error> {
        if self.protocol == PROTOCOL {
            Ok(())
        } else {
            Err(Error::Protocol(format!(
                "expected {PROTOCOL}, got {}",
                self.protocol
            )))
        }
    }

    /// Require the request to target the runner's own workflow identity.
    pub fn validate_workflow(
        &self,
        expected_id: &str,
        expected_version: &str,
    ) -> Result<(), Error> {
        if self.workflow.id != expected_id {
            return Err(Error::Protocol(format!(
                "expected workflow id {expected_id}, got {}",
                self.workflow.id
            )));
        }
        if self.workflow.version != expected_version {
            return Err(Error::Protocol(format!(
                "expected workflow version {expected_version}, got {}",
                self.workflow.version
            )));
        }
        Ok(())
    }

    /// Validate both the protocol revision and workflow identity.
    pub fn validate_for(&self, expected_id: &str, expected_version: &str) -> Result<(), Error> {
        self.validate_protocol()?;
        self.validate_workflow(expected_id, expected_version)
    }
}

/// Successful response returned by a workflow runner.
///
/// `replay_fingerprint` must contain deterministic implementation identity
/// such as an algorithm and crate version. Artifact paths are validated by the
/// LightFlow host after the runner exits.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Response {
    pub outputs: Map<String, Value>,
    #[serde(default)]
    pub artifacts: Vec<WorkflowArtifact>,
    pub replay_fingerprint: Map<String, Value>,
}

/// Error produced while exchanging the runner protocol.
#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Json(serde_json::Error),
    Protocol(String),
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "runner I/O failed: {error}"),
            Self::Json(error) => write!(formatter, "runner JSON failed: {error}"),
            Self::Protocol(message) => write!(formatter, "invalid runner request: {message}"),
        }
    }
}

impl StdError for Error {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Json(error) => Some(error),
            Self::Protocol(_) => None,
        }
    }
}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for Error {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

/// Read and validate one execution request from stdin.
pub fn read_request_from_stdin() -> Result<Request, Error> {
    read_request(io::stdin().lock())
}

/// Read and validate one execution request from a reader.
pub fn read_request(mut reader: impl Read) -> Result<Request, Error> {
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes)?;
    let request: Request = serde_json::from_slice(&bytes)?;
    request.validate_protocol()?;
    Ok(request)
}

/// Write one execution response to stdout.
pub fn write_response_to_stdout(response: &Response) -> Result<(), Error> {
    write_response(io::stdout().lock(), response)
}

/// Write one execution response to a writer.
pub fn write_response(mut writer: impl Write, response: &Response) -> Result<(), Error> {
    serde_json::to_writer(&mut writer, response)?;
    writer.write_all(b"\n")?;
    Ok(())
}

/// Run one workflow spec from process arguments and print JSON execution output.
///
/// This is intended for `src/bin/*.rs` entrypoints inside workflow crates:
///
/// ```no_run
/// use lightflow::preload::*;
///
/// fn define() -> WorkflowSpec {
///     workflow! {
///         input "value": "text"
///         output "value": "text"
///     }
///         .name("Example")
///         .build()
/// }
///
/// fn main() -> lightflow::runner::RunnerResult<()> {
///     lightflow::runner::run_workflow_from_env(define())
/// }
/// ```
pub fn run_workflow_from_env(workflow: WorkflowSpec) -> RunnerResult<()> {
    let args = env::args().skip(1).collect::<Vec<_>>();
    if args
        .first()
        .is_some_and(|arg| matches!(arg.as_str(), "-h" | "--help" | "help"))
    {
        eprintln!("{}", usage(&workflow.id));
        return Ok(());
    }
    let options = parse_execution_options(&args)?;
    let cwd = env::current_dir()?;
    let service = ApiService::new(cwd).with_workflow_paths(default_workflow_paths());
    let execution = service.execute_workflow_spec(&workflow, options)?;
    print_json(&execution)?;
    Ok(())
}

/// Run a typed workflow from process arguments and print JSON output.
///
/// This is intended for `src/bin/*.rs` entrypoints whose library crate
/// implements [`Workflow`]:
///
/// ```ignore
/// use lightflow::preload::*;
///
/// fn main() -> lightflow::runner::RunnerResult<()> {
///     lightflow::runner::run_typed_workflow_from_env(my_workflow_crate::MyWorkflow)
/// }
/// ```
///
/// The input is a JSON value accepted by the workflow's `Input` type:
///
/// ```bash
/// my-workflow --input '{"user_message":"帮我查最新消息"}'
/// my-workflow --input @input.json
/// echo '{"user_message":"hello"}' | my-workflow --input -
/// ```
pub fn run_typed_workflow_from_env<I, O, W>(workflow: W) -> RunnerResult<()>
where
    I: DeserializeOwned + Send + 'static,
    O: Serialize + Send + 'static,
    W: Runnable<I, O>,
{
    let args = env::args().skip(1).collect::<Vec<_>>();
    if args
        .first()
        .is_some_and(|arg| matches!(arg.as_str(), "-h" | "--help" | "help"))
    {
        eprintln!("{}", typed_usage());
        return Ok(());
    }
    let input = parse_typed_input(&args)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let output = runtime.block_on(workflow.run(input))?;
    print_json(&output)?;
    Ok(())
}

fn parse_execution_options(args: &[String]) -> RunnerResult<WorkflowExecutionOptions> {
    let mut execution = WorkflowExecutionOptions::default();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--input" | "-i" => {
                let value = required_value(args, index, "--input")?;
                insert_input_assignment(&mut execution, value, "--input")?;
                index += 2;
            }
            "--inputs" | "--json" => {
                let value = required_value(args, index, "--inputs")?;
                let inputs = request_json(value)?;
                let Some(inputs) = inputs.as_object() else {
                    return Err("--inputs must be a JSON object".into());
                };
                execution.inputs.extend(inputs.clone());
                index += 2;
            }
            "--text" => {
                insert_named_input(
                    &mut execution,
                    "text",
                    required_value(args, index, "--text")?.to_owned().into(),
                )?;
                index += 2;
            }
            "--prompt" => {
                insert_named_input(
                    &mut execution,
                    "prompt",
                    required_value(args, index, "--prompt")?.to_owned().into(),
                )?;
                index += 2;
            }
            "--image" | "--image-path" => {
                insert_named_input(
                    &mut execution,
                    "image_path",
                    required_value(args, index, args[index].as_str())?
                        .to_owned()
                        .into(),
                )?;
                index += 2;
            }
            "--output" | "--output-path" | "-o" => {
                insert_named_input(
                    &mut execution,
                    "output_path",
                    required_value(args, index, args[index].as_str())?
                        .to_owned()
                        .into(),
                )?;
                index += 2;
            }
            "--disable" => {
                execution
                    .disabled_nodes
                    .push(required_value(args, index, "--disable")?.to_owned());
                index += 2;
            }
            "--enable" => {
                execution
                    .enabled_nodes
                    .push(required_value(args, index, "--enable")?.to_owned());
                index += 2;
            }
            value => return Err(format!("unexpected argument: {value}").into()),
        }
    }
    Ok(execution)
}

fn request_json(value: &str) -> RunnerResult<serde_json::Value> {
    if value == "-" {
        let mut buffer = String::new();
        io::stdin().read_to_string(&mut buffer)?;
        return Ok(serde_json::from_str(&buffer)?);
    }
    if let Some(path) = value.strip_prefix('@') {
        return Ok(serde_json::from_str(&fs::read_to_string(path)?)?);
    }
    Ok(serde_json::from_str(value)?)
}

fn parse_typed_input<T>(args: &[String]) -> RunnerResult<T>
where
    T: DeserializeOwned,
{
    let value = match args {
        [] => serde_json::Value::Object(Default::default()),
        [flag, value] if matches!(flag.as_str(), "--input" | "--input-json" | "--json") => {
            request_json(value)?
        }
        [value] => request_json(value)?,
        _ => return Err(typed_usage().into()),
    };
    Ok(serde_json::from_value(value)?)
}

fn insert_input_assignment(
    execution: &mut WorkflowExecutionOptions,
    value: &str,
    flag: &str,
) -> RunnerResult<()> {
    let Some((name, raw_value)) = value.split_once('=') else {
        return Err(format!("{flag} must use <name=json-or-text>").into());
    };
    insert_named_input(execution, name, parse_input_value(raw_value))
}

fn insert_named_input(
    execution: &mut WorkflowExecutionOptions,
    name: &str,
    value: serde_json::Value,
) -> RunnerResult<()> {
    if name.is_empty() {
        return Err("input name cannot be empty".into());
    }
    execution.inputs.insert(name.to_owned(), value);
    Ok(())
}

fn parse_input_value(value: &str) -> serde_json::Value {
    serde_json::from_str(value).unwrap_or_else(|_| serde_json::Value::String(value.to_owned()))
}

fn required_value<'a>(args: &'a [String], index: usize, flag: &str) -> RunnerResult<&'a str> {
    let Some(value) = args.get(index + 1) else {
        return Err(format!("missing value for {flag}").into());
    };
    if value.starts_with("--") {
        return Err(format!("missing value for {flag}").into());
    }
    Ok(value)
}

fn default_workflow_paths() -> Vec<PathBuf> {
    let data_home = xdg_data_home();
    let default = home_lightflow().unwrap_or_else(|| data_home.join("lightflow"));
    let lfw_path = env::var("LFW_PATH")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| default.display().to_string());
    let mut paths = env::split_paths(&lfw_path).collect::<Vec<_>>();
    if !paths.iter().any(|path| path == &default) {
        paths.push(default);
    }
    paths
}

fn home_lightflow() -> Option<PathBuf> {
    env::var_os("HOME").map(|home| Path::new(&home).join(".lightflow"))
}

fn xdg_data_home() -> PathBuf {
    env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| Path::new(&home).join(".local/share")))
        .unwrap_or_else(|| PathBuf::from("."))
}

fn print_json(value: &impl Serialize) -> RunnerResult<()> {
    let stdout = io::stdout();
    let mut handle = stdout.lock();
    serde_json::to_writer_pretty(&mut handle, value)?;
    println!();
    Ok(())
}

fn usage(workflow_id: &str) -> String {
    format!(
        "usage:\n  {workflow_id} [--input|-i <name=json>] [--inputs <json|-|@file>] [--prompt <text>] [--text <text>] [--image <path>] [--output <path>] [--disable <node>] [--enable <node>]"
    )
}

fn typed_usage() -> String {
    "usage:\n  workflow-bin [--input <json|-|@file>]\n\nIf input is omitted, it defaults to {}."
        .to_owned()
}

#[cfg(test)]
mod tests;
