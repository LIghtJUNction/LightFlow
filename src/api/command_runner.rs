use crate::api::executors;
use crate::api::plan::COMMAND_ENGINE;
use crate::api::{ApiError, ApiResult};
use crate::workflow::{WorkflowArtifact, WorkflowSpec};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const PROTOCOL: &str = "lightflow.command.v1";
const TIMEOUT_ENV: &str = "LIGHTFLOW_COMMAND_TIMEOUT_MS";
const DEFAULT_TIMEOUT_MS: u64 = 30 * 60 * 1_000;
const MAX_TIMEOUT_MS: u64 = 24 * 60 * 60 * 1_000;
const STDOUT_LIMIT: usize = 16 * 1024 * 1024;
const STDERR_LIMIT: usize = 256 * 1024;
const WAIT_INTERVAL: Duration = Duration::from_millis(10);

#[derive(Debug)]
pub(super) struct CommandExecution {
    pub(super) outputs: Map<String, Value>,
    pub(super) artifacts: Vec<WorkflowArtifact>,
    pub(super) replay_fingerprint: Option<Value>,
}

#[derive(Serialize)]
struct CommandRequest<'a> {
    protocol: &'static str,
    workflow: WorkflowIdentity<'a>,
    inputs: &'a Map<String, Value>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    models: &'a BTreeMap<String, crate::runner::ModelBinding>,
}

#[derive(Serialize)]
struct WorkflowIdentity<'a> {
    id: &'a str,
    version: &'a str,
}

#[derive(Deserialize)]
struct CommandResponse {
    outputs: Map<String, Value>,
    #[serde(default)]
    artifacts: Vec<WorkflowArtifact>,
    replay_fingerprint: Value,
}

struct ProcessOutput {
    status: ExitStatus,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct RunnerProtocol {
    pub(super) id: &'static str,
    pub(super) engine: &'static str,
    pub(super) response_policy: ResponsePolicy,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(super) enum ResponsePolicy {
    LegacyCommand,
    Runner,
}

pub(super) struct RunnerRequest<'a> {
    pub(super) inputs: &'a Map<String, Value>,
    pub(super) models: &'a BTreeMap<String, crate::runner::ModelBinding>,
}

pub(super) fn execute(
    root: &Path,
    workflow: &WorkflowSpec,
    inputs: &Map<String, Value>,
) -> ApiResult<CommandExecution> {
    let runner = executors::validated_command_runner_path().map_err(ApiError::InvalidRequest)?;
    let timeout = command_timeout(std::env::var(TIMEOUT_ENV).ok().as_deref())
        .map_err(ApiError::InvalidRequest)?;
    execute_with_runner(root, workflow, inputs, &runner, timeout)
}

fn execute_with_runner(
    root: &Path,
    workflow: &WorkflowSpec,
    inputs: &Map<String, Value>,
    runner: &Path,
    timeout: Duration,
) -> ApiResult<CommandExecution> {
    let mut command = Command::new(runner);
    execute_with_command(
        root,
        workflow,
        inputs,
        &mut command,
        &format!("external command runner {}", runner.display()),
        RunnerProtocol {
            id: PROTOCOL,
            engine: COMMAND_ENGINE,
            response_policy: ResponsePolicy::LegacyCommand,
        },
        timeout,
    )
}

pub(super) fn execute_with_command(
    root: &Path,
    workflow: &WorkflowSpec,
    inputs: &Map<String, Value>,
    command: &mut Command,
    label: &str,
    protocol: RunnerProtocol,
    timeout: Duration,
) -> ApiResult<CommandExecution> {
    execute_with_command_and_models(
        root,
        workflow,
        RunnerRequest {
            inputs,
            models: &BTreeMap::new(),
        },
        command,
        label,
        protocol,
        timeout,
    )
}

pub(super) fn execute_with_command_and_models(
    root: &Path,
    workflow: &WorkflowSpec,
    request: RunnerRequest<'_>,
    command: &mut Command,
    label: &str,
    protocol: RunnerProtocol,
    timeout: Duration,
) -> ApiResult<CommandExecution> {
    let request = serde_json::to_vec(&CommandRequest {
        protocol: protocol.id,
        workflow: WorkflowIdentity {
            id: &workflow.id,
            version: &workflow.version,
        },
        inputs: request.inputs,
        models: request.models,
    })
    .map_err(|error| {
        ApiError::InvalidRequest(format!("serialize external command request: {error}"))
    })?;
    let output = run_process(root, command, label, &request, timeout)?;
    if !output.status.success() {
        return Err(ApiError::InvalidRequest(format!(
            "{label} failed with {}: {}",
            display_status(output.status),
            display_stderr(&output.stderr)
        )));
    }

    let response: CommandResponse = serde_json::from_slice(&output.stdout).map_err(|error| {
        ApiError::InvalidRequest(format!("{label} returned invalid JSON: {error}"))
    })?;
    validate_response(
        root,
        workflow,
        response,
        protocol.engine,
        protocol.response_policy,
    )
}

fn run_process(
    root: &Path,
    command: &mut Command,
    label: &str,
    request: &[u8],
    timeout: Duration,
) -> ApiResult<ProcessOutput> {
    command
        .current_dir(root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;

        command.process_group(0);
    }
    let mut child = command
        .spawn()
        .map_err(|error| ApiError::InvalidRequest(format!("failed to start {label}: {error}")))?;

    let stdout = child.stdout.take().ok_or_else(|| {
        ApiError::InvalidRequest("external command runner stdout is unavailable".to_owned())
    })?;
    let stderr = child.stderr.take().ok_or_else(|| {
        ApiError::InvalidRequest("external command runner stderr is unavailable".to_owned())
    })?;
    let stdout_reader = thread::spawn(move || read_capped(stdout, STDOUT_LIMIT));
    let stderr_reader = thread::spawn(move || read_capped(stderr, STDERR_LIMIT));

    let write_result = child
        .stdin
        .take()
        .ok_or_else(|| {
            ApiError::InvalidRequest("external command runner stdin is unavailable".to_owned())
        })
        .and_then(|mut stdin| {
            stdin.write_all(request).map_err(|error| {
                ApiError::InvalidRequest(format!(
                    "failed to write external command request: {error}"
                ))
            })
        });
    let status = wait_for_child(&mut child, timeout, label);
    let stdout = join_reader(stdout_reader, label, "stdout")?;
    let stderr = join_reader(stderr_reader, label, "stderr")?;
    let status = status?;
    if status.success() {
        write_result?;
    }

    Ok(ProcessOutput {
        status,
        stdout,
        stderr,
    })
}

fn wait_for_child(child: &mut Child, timeout: Duration, label: &str) -> ApiResult<ExitStatus> {
    let started = Instant::now();
    loop {
        if let Some(status) = child.try_wait().map_err(ApiError::Io)? {
            return Ok(status);
        }
        if started.elapsed() >= timeout {
            terminate_child(child);
            return Err(ApiError::InvalidRequest(format!(
                "{label} timed out after {} ms",
                timeout.as_millis()
            )));
        }
        thread::sleep(WAIT_INTERVAL);
    }
}

#[cfg(unix)]
fn terminate_child(child: &mut Child) {
    let process_group = -(child.id() as libc::pid_t);
    // SAFETY: the negative PID targets only the process group created for this child.
    unsafe {
        libc::kill(process_group, libc::SIGKILL);
    }
    let _ = child.wait();
}

#[cfg(not(unix))]
fn terminate_child(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

fn read_capped(mut reader: impl Read, limit: usize) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader
        .by_ref()
        .take((limit + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        // Keep draining so the child never blocks on a full pipe and can
        // exit on its own; the oversized output still fails the run.
        let _ = std::io::copy(&mut reader, &mut std::io::sink());
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("process output exceeds {limit} bytes"),
        ));
    }
    Ok(bytes)
}

fn join_reader(
    reader: thread::JoinHandle<std::io::Result<Vec<u8>>>,
    label: &str,
    stream: &str,
) -> ApiResult<Vec<u8>> {
    reader
        .join()
        .map_err(|_| ApiError::InvalidRequest(format!("{label} {stream} reader panicked")))?
        .map_err(|error| {
            ApiError::InvalidRequest(format!("failed to read {label} {stream}: {error}"))
        })
}

fn validate_response(
    root: &Path,
    workflow: &WorkflowSpec,
    response: CommandResponse,
    engine: &str,
    response_policy: ResponsePolicy,
) -> ApiResult<CommandExecution> {
    let declared = workflow
        .outputs
        .iter()
        .map(|port| port.name.as_str())
        .collect::<BTreeSet<_>>();
    let returned = response
        .outputs
        .keys()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let missing = declared.difference(&returned).copied().collect::<Vec<_>>();
    let unknown = returned.difference(&declared).copied().collect::<Vec<_>>();
    if !missing.is_empty() || !unknown.is_empty() {
        return Err(ApiError::InvalidRequest(format!(
            "external command outputs do not match workflow {}: missing [{}], unknown [{}]",
            workflow.id,
            missing.join(", "),
            unknown.join(", ")
        )));
    }
    for port in &workflow.outputs {
        let value = &response.outputs[&port.name];
        if !output_value_matches_type(value, &port.ty) {
            return Err(ApiError::InvalidRequest(format!(
                "external command output {} must match declared type {}, got {}",
                port.name,
                port.ty,
                json_type_name(value)
            )));
        }
    }
    if !response.replay_fingerprint.is_object() {
        return Err(ApiError::InvalidRequest(
            "external command replay_fingerprint must be a JSON object".to_owned(),
        ));
    }
    if response_policy == ResponsePolicy::Runner
        && !response
            .replay_fingerprint
            .get("implementation")
            .and_then(Value::as_str)
            .is_some_and(|identity| !identity.trim().is_empty())
    {
        return Err(ApiError::InvalidRequest(
            "package command replay_fingerprint requires a non-empty implementation identity"
                .to_owned(),
        ));
    }

    validate_artifacts(root, &response.artifacts, response_policy)?;
    Ok(CommandExecution {
        outputs: response.outputs,
        artifacts: response.artifacts,
        replay_fingerprint: Some(json!({
            "engine": engine,
            "runner": response.replay_fingerprint,
        })),
    })
}

fn output_value_matches_type(value: &Value, ty: &str) -> bool {
    // Null is a declared output without a value; required outputs are
    // enforced by workflow-level port validation.
    if value.is_null() {
        return true;
    }
    match ty {
        "text" | "path" => value.is_string(),
        "integer" => value.as_i64().is_some() || value.as_u64().is_some(),
        "number" => value.is_number(),
        "boolean" => value.is_boolean(),
        "artifact" => value.is_object(),
        "artifact[]" => value.is_array(),
        "json" => true,
        _ => true,
    }
}

fn json_type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn validate_artifacts(
    root: &Path,
    artifacts: &[WorkflowArtifact],
    response_policy: ResponsePolicy,
) -> ApiResult<()> {
    let mut ids = BTreeSet::new();
    for artifact in artifacts {
        if artifact.id.trim().is_empty()
            || artifact.kind.trim().is_empty()
            || artifact.path.trim().is_empty()
            || artifact.mime_type.trim().is_empty()
        {
            return Err(ApiError::InvalidRequest(
                "external command artifacts require non-empty id, kind, path, and mime_type"
                    .to_owned(),
            ));
        }
        if !ids.insert(artifact.id.as_str()) {
            return Err(ApiError::InvalidRequest(format!(
                "external command returned duplicate artifact id {}",
                artifact.id
            )));
        }
        let path = artifact_path(root, &artifact.path, response_policy)?;
        if !path.is_file() {
            return Err(ApiError::InvalidRequest(format!(
                "external command artifact does not name an existing file: {}",
                path.display()
            )));
        }
        if response_policy == ResponsePolicy::Runner {
            let canonical_root = root.canonicalize().map_err(ApiError::from)?;
            let canonical_path = path.canonicalize().map_err(ApiError::from)?;
            if !canonical_path.starts_with(&canonical_root) {
                return Err(ApiError::InvalidRequest(format!(
                    "package command artifact escapes project root: {}",
                    artifact.path
                )));
            }
        }
    }
    Ok(())
}

fn artifact_path(root: &Path, value: &str, response_policy: ResponsePolicy) -> ApiResult<PathBuf> {
    let path = Path::new(value);
    if response_policy == ResponsePolicy::Runner {
        if path.is_absolute()
            || path.components().any(|component| {
                matches!(
                    component,
                    std::path::Component::ParentDir
                        | std::path::Component::RootDir
                        | std::path::Component::Prefix(_)
                )
            })
        {
            return Err(ApiError::InvalidRequest(format!(
                "package command artifact path must be relative and cannot contain `..`: {value}"
            )));
        }
        return Ok(root.join(path));
    }
    Ok(if path.is_absolute() {
        path.to_owned()
    } else {
        root.join(path)
    })
}

pub(super) fn command_timeout(value: Option<&str>) -> Result<Duration, String> {
    let Some(value) = value else {
        return Ok(Duration::from_millis(DEFAULT_TIMEOUT_MS));
    };
    let milliseconds = value.parse::<u64>().map_err(|_| {
        format!("{TIMEOUT_ENV} must be an integer from 1 through {MAX_TIMEOUT_MS} milliseconds")
    })?;
    if !(1..=MAX_TIMEOUT_MS).contains(&milliseconds) {
        return Err(format!(
            "{TIMEOUT_ENV} must be from 1 through {MAX_TIMEOUT_MS} milliseconds"
        ));
    }
    Ok(Duration::from_millis(milliseconds))
}

fn display_status(status: ExitStatus) -> String {
    status
        .code()
        .map(|code| format!("exit code {code}"))
        .unwrap_or_else(|| "termination by signal".to_owned())
}

fn display_stderr(stderr: &[u8]) -> String {
    let value = String::from_utf8_lossy(stderr);
    let value = value.trim();
    if value.is_empty() {
        "no stderr output".to_owned()
    } else {
        value.to_owned()
    }
}

#[cfg(test)]
mod tests;
