use crate::api::command_runner::{
    self, CommandExecution, ResponsePolicy, RunnerProtocol, RunnerRequest,
};
use crate::api::plan::RUNNER_ENGINE;
use crate::api::source::WorkflowOrigin;
use crate::api::{ApiError, ApiResult};
use crate::runner::PROTOCOL;
use crate::workflow::WorkflowSpec;
use serde_json::{Map, Value};
use std::path::Path;
use std::process::Command;

const TIMEOUT_ENV: &str = "LIGHTFLOW_COMMAND_TIMEOUT_MS";

pub(super) fn execute(
    root: &Path,
    workflow: &WorkflowSpec,
    inputs: &Map<String, Value>,
    origin: &WorkflowOrigin,
) -> ApiResult<CommandExecution> {
    let runner = origin.runner_bin.as_deref().ok_or_else(|| {
        ApiError::InvalidRequest(format!(
            "workflow package {} has no [package.metadata.lightflow] runner",
            origin.package_name
        ))
    })?;
    if !origin.manifest_path.is_file() {
        return Err(ApiError::InvalidRequest(format!(
            "workflow package {} manifest no longer exists: {}",
            origin.package_name,
            origin.manifest_path.display()
        )));
    }

    let timeout = command_runner::command_timeout(std::env::var(TIMEOUT_ENV).ok().as_deref())
        .map_err(ApiError::InvalidRequest)?;
    let models = super::model_manager::resolve_runner_models(root, workflow, inputs)?;
    let mut command = Command::new("cargo");
    command
        .arg("run")
        .arg("--quiet")
        .arg("--manifest-path")
        .arg(&origin.manifest_path);
    if !origin.runner_features.is_empty() {
        command
            .arg("--features")
            .arg(origin.runner_features.join(","));
    }
    command.arg("--bin").arg(runner);
    let label = format!("package runner {} ({runner})", origin.package_name);
    command_runner::execute_with_command_and_models(
        root,
        workflow,
        RunnerRequest {
            inputs,
            models: &models,
        },
        &mut command,
        &label,
        RunnerProtocol {
            id: PROTOCOL,
            engine: RUNNER_ENGINE,
            response_policy: ResponsePolicy::Runner,
        },
        timeout,
    )
}
