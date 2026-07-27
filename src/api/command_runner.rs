use crate::api::executors;
use crate::api::plan::COMMAND_ENGINE;
use crate::api::{ApiError, ApiResult};
use crate::runner::{CommandRequest, CommandResponse};
use crate::workflow::{WorkflowArtifact, WorkflowSpec};
use serde_json::{Map, Value, json};
use std::collections::BTreeSet;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

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

struct ProcessOutput {
    status: ExitStatus,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
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
    let request = serde_json::to_vec(&CommandRequest::new(
        &workflow.id,
        &workflow.version,
        inputs.clone(),
    ))
    .map_err(|error| {
        ApiError::InvalidRequest(format!("serialize external command request: {error}"))
    })?;
    let output = run_process(root, runner, &request, timeout)?;
    if !output.status.success() {
        return Err(ApiError::InvalidRequest(format!(
            "external command runner {} failed with {}: {}",
            runner.display(),
            display_status(output.status),
            display_stderr(&output.stderr)
        )));
    }

    let response: CommandResponse = serde_json::from_slice(&output.stdout).map_err(|error| {
        ApiError::InvalidRequest(format!(
            "external command runner {} returned invalid JSON: {error}",
            runner.display()
        ))
    })?;
    validate_response(root, workflow, response)
}

fn run_process(
    root: &Path,
    runner: &Path,
    request: &[u8],
    timeout: Duration,
) -> ApiResult<ProcessOutput> {
    let mut command = Command::new(runner);
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
    let mut child = command.spawn().map_err(|error| {
        ApiError::InvalidRequest(format!(
            "failed to start external command runner {}: {error}",
            runner.display()
        ))
    })?;

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
    let status = wait_for_child(&mut child, timeout);
    let stdout = join_reader(stdout_reader, "stdout")?;
    let stderr = join_reader(stderr_reader, "stderr")?;
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

fn wait_for_child(child: &mut Child, timeout: Duration) -> ApiResult<ExitStatus> {
    let started = Instant::now();
    loop {
        if let Some(status) = child.try_wait().map_err(ApiError::Io)? {
            return Ok(status);
        }
        if started.elapsed() >= timeout {
            terminate_child(child);
            return Err(ApiError::InvalidRequest(format!(
                "external command runner timed out after {} ms",
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
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("process output exceeds {limit} bytes"),
        ));
    }
    Ok(bytes)
}

fn join_reader(
    reader: thread::JoinHandle<std::io::Result<Vec<u8>>>,
    stream: &str,
) -> ApiResult<Vec<u8>> {
    reader
        .join()
        .map_err(|_| {
            ApiError::InvalidRequest(format!("external command runner {stream} reader panicked"))
        })?
        .map_err(|error| {
            ApiError::InvalidRequest(format!(
                "failed to read external command runner {stream}: {error}"
            ))
        })
}

fn validate_response(
    root: &Path,
    workflow: &WorkflowSpec,
    response: CommandResponse,
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
    validate_artifacts(root, &response.artifacts)?;
    Ok(CommandExecution {
        outputs: response.outputs,
        artifacts: response.artifacts,
        replay_fingerprint: Some(json!({
            "engine": COMMAND_ENGINE,
            "runner": response.replay_fingerprint,
        })),
    })
}

fn output_value_matches_type(value: &Value, ty: &str) -> bool {
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

fn validate_artifacts(root: &Path, artifacts: &[WorkflowArtifact]) -> ApiResult<()> {
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
        let path = artifact_path(root, &artifact.path);
        if !path.is_file() {
            return Err(ApiError::InvalidRequest(format!(
                "external command artifact does not name an existing file: {}",
                path.display()
            )));
        }
    }
    Ok(())
}

fn artifact_path(root: &Path, value: &str) -> PathBuf {
    let path = Path::new(value);
    if path.is_absolute() {
        path.to_owned()
    } else {
        root.join(path)
    }
}

fn command_timeout(value: Option<&str>) -> Result<Duration, String> {
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
