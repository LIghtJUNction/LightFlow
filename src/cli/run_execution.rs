use super::{CliError, CliResult};
use crate::api::{ApiService, RunStageRecord};
use crate::cli::run::RunOptions;
use crate::workflow::{WorkflowArtifact, WorkflowExecution};
use serde::Serialize;
use serde_json::json;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
enum RunOutput {
    Single(Box<WorkflowExecution>),
    Pipeline(PipelineExecution),
}

#[derive(Debug, Clone, PartialEq, Serialize)]
struct PipelineExecution {
    pipeline: bool,
    stages: Vec<WorkflowExecution>,
    outputs: serde_json::Map<String, serde_json::Value>,
    artifacts: Vec<WorkflowArtifact>,
}

#[derive(Debug)]
struct ExecutedRunOptions {
    output: RunOutput,
    stages: Vec<RunStageRecord>,
}

#[derive(Debug)]
struct RunOptionsExecutionError {
    error: CliError,
    stages: Vec<RunStageRecord>,
    partial_output: Option<RunOutput>,
}

pub(super) fn execute_and_record_run_options(
    service: &ApiService,
    options: RunOptions,
) -> CliResult<serde_json::Value> {
    let started_at_ms = ApiService::now_ms();
    let executed = match execute_run_options(service, options) {
        Ok(executed) => executed,
        Err(error) => {
            let completed_at_ms = ApiService::now_ms();
            let error_json = json!({
                "message": error.error.to_string(),
            });
            let history = service.record_failed_run_with_surface(
                &error.stages,
                &error_json,
                error.partial_output.as_ref(),
                started_at_ms,
                completed_at_ms,
                "cli",
            )?;
            return Err(CliError::Usage(format!(
                "{}\nrun_id: {}\ntrace_path: {}",
                error.error,
                history.run_id,
                history.run_dir.join("execution.json").display()
            )));
        }
    };
    let completed_at_ms = ApiService::now_ms();
    let mut value = service.execution_with_model_locks(&executed.output)?;
    let history = service.record_completed_run_with_surface(
        &executed.stages,
        &value,
        started_at_ms,
        completed_at_ms,
        "cli",
    )?;
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

fn execute_run_options(
    service: &ApiService,
    options: RunOptions,
) -> Result<ExecutedRunOptions, Box<RunOptionsExecutionError>> {
    let mut previous_outputs = serde_json::Map::new();
    let mut executions = Vec::new();
    let mut artifacts = Vec::new();
    let mut effective_stages = Vec::new();
    let stage_count = options.stages.len();

    for (index, mut stage) in options.stages.into_iter().enumerate() {
        if index > 0 {
            let explicit_inputs = std::mem::take(&mut stage.execution.inputs);
            stage.execution.inputs = previous_outputs.clone();
            stage.execution.inputs.extend(explicit_inputs);
        }
        effective_stages.push(RunStageRecord {
            workflow_id: stage.workflow_id.clone(),
            execution: stage.execution.clone(),
        });
        let execution = match service.execute_workflow(&stage.workflow_id, stage.execution) {
            Ok(execution) => execution,
            Err(error) => {
                return Err(Box::new(RunOptionsExecutionError {
                    error: error.into(),
                    stages: effective_stages,
                    partial_output: partial_run_output(
                        stage_count,
                        executions.clone(),
                        previous_outputs.clone(),
                        artifacts.clone(),
                    ),
                }));
            }
        };
        previous_outputs = execution.outputs.clone();
        artifacts.extend(execution.artifacts.clone());
        executions.push(execution);
    }

    if stage_count == 1 {
        let Some(execution) = executions.pop() else {
            return Err(Box::new(RunOptionsExecutionError {
                error: CliError::Usage("missing workflow id".to_owned()),
                stages: effective_stages,
                partial_output: None,
            }));
        };
        return Ok(ExecutedRunOptions {
            output: RunOutput::Single(Box::new(execution)),
            stages: effective_stages,
        });
    }

    Ok(ExecutedRunOptions {
        output: RunOutput::Pipeline(PipelineExecution {
            pipeline: true,
            stages: executions,
            outputs: previous_outputs,
            artifacts,
        }),
        stages: effective_stages,
    })
}

fn partial_run_output(
    stage_count: usize,
    executions: Vec<WorkflowExecution>,
    outputs: serde_json::Map<String, serde_json::Value>,
    artifacts: Vec<WorkflowArtifact>,
) -> Option<RunOutput> {
    if executions.is_empty() {
        return None;
    }
    if stage_count == 1 {
        return executions
            .into_iter()
            .next()
            .map(Box::new)
            .map(RunOutput::Single);
    }
    Some(RunOutput::Pipeline(PipelineExecution {
        pipeline: true,
        stages: executions,
        outputs,
        artifacts,
    }))
}
