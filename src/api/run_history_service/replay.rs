use crate::api::replay_fingerprints::replay_report;
use crate::api::{ApiError, ApiResult, ApiService, history, nodes};
use crate::workflow::{WorkflowArtifact, WorkflowExecution};

impl ApiService {
    /// Replay a recorded run by executing the stored manifest stages and
    /// writing a new immutable run record.
    pub fn replay_run(&self, selector: &str) -> ApiResult<serde_json::Value> {
        self.replay_run_with_surface(selector, "http")
    }

    /// Replay a recorded run with an explicit adapter surface label.
    pub fn replay_run_with_surface(
        &self,
        selector: &str,
        surface: &str,
    ) -> ApiResult<serde_json::Value> {
        let replay_stages = history::replay_stages(&self.repo_root, selector)?;
        let stages = replay_stages.stages;
        let original = history::get_run(&self.repo_root, selector)?;
        let started_at_ms = history::now_ms();
        let output =
            match self.execute_recorded_stages(&stages, replay_stages.stage_inputs_resolved) {
                Ok(output) => output,
                Err(error) => {
                    let completed_at_ms = history::now_ms();
                    let error_json = serde_json::json!({
                        "code": error.code(),
                        "message": error.message(),
                        "replayed_from": selector,
                    });
                    history::record_failed_run_with_surface(
                        &self.repo_root,
                        &stages,
                        &error_json,
                        None::<&serde_json::Value>,
                        started_at_ms,
                        completed_at_ms,
                        surface,
                    )?;
                    return Err(error);
                }
            };
        let completed_at_ms = history::now_ms();
        let mut value = self.execution_with_model_locks(&output)?;
        let replay = replay_report(selector, &original.execution, &value);
        {
            let Some(object) = value.as_object_mut() else {
                return Err(ApiError::InvalidRequest(
                    "replay output must be a JSON object".to_owned(),
                ));
            };
            object.insert("replayed_from".to_owned(), selector.to_owned().into());
            object.insert("replay".to_owned(), replay);
        }
        let record = history::record_completed_run_with_surface(
            &self.repo_root,
            &stages,
            &value,
            started_at_ms,
            completed_at_ms,
            surface,
        )?;
        let Some(object) = value.as_object_mut() else {
            return Err(ApiError::InvalidRequest(
                "replay output must be a JSON object".to_owned(),
            ));
        };
        object.insert("run_id".to_owned(), record.run_id.into());
        object.insert(
            "run_dir".to_owned(),
            record.run_dir.display().to_string().into(),
        );
        object.insert(
            "trace_path".to_owned(),
            record
                .run_dir
                .join("execution.json")
                .display()
                .to_string()
                .into(),
        );
        Ok(value)
    }

    /// Serialize an execution response and attach the current model-lock
    /// fingerprints that make replay model drift explicit.
    pub fn execution_with_model_locks(
        &self,
        execution: &impl serde::Serialize,
    ) -> ApiResult<serde_json::Value> {
        let mut value = serde_json::to_value(execution).map_err(|error| {
            ApiError::InvalidRequest(format!("invalid execution JSON: {error}"))
        })?;
        self.attach_model_locks(&mut value)?;
        Ok(value)
    }

    fn execute_recorded_stages(
        &self,
        stages: &[history::RunStageRecord],
        stage_inputs_resolved: bool,
    ) -> ApiResult<ApiRunOutput> {
        let mut previous_outputs = serde_json::Map::new();
        let mut executions = Vec::new();
        let mut artifacts = Vec::new();
        let stage_count = stages.len();

        for (index, stage) in stages.iter().cloned().enumerate() {
            let mut execution_options = stage.execution;
            if index > 0 && !stage_inputs_resolved {
                let explicit_inputs = std::mem::take(&mut execution_options.inputs);
                execution_options.inputs = previous_outputs.clone();
                execution_options.inputs.extend(explicit_inputs);
            }
            let execution = self.execute_workflow(&stage.workflow_id, execution_options)?;
            previous_outputs = execution.outputs.clone();
            artifacts.extend(execution.artifacts.clone());
            executions.push(execution);
        }

        if stage_count == 1 {
            let execution = executions.pop().ok_or_else(|| {
                ApiError::InvalidRequest("run has no replayable stages".to_owned())
            })?;
            return Ok(ApiRunOutput::Single(execution));
        }

        Ok(ApiRunOutput::Pipeline(ApiPipelineExecution {
            pipeline: true,
            stages: executions,
            outputs: previous_outputs,
            artifacts,
        }))
    }

    fn attach_model_locks(&self, value: &mut serde_json::Value) -> ApiResult<()> {
        let workflows = self.workflow_specs()?;
        let model_locks = nodes::model_lock_fingerprints(&self.repo_root, &workflows, value);
        let Some(object) = value.as_object_mut() else {
            return Err(ApiError::InvalidRequest(
                "workflow execution output must be a JSON object".to_owned(),
            ));
        };
        object.insert(
            "model_locks".to_owned(),
            serde_json::to_value(model_locks).map_err(|error| {
                ApiError::InvalidRequest(format!("invalid model lock JSON: {error}"))
            })?,
        );
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(untagged)]
enum ApiRunOutput {
    Single(WorkflowExecution),
    Pipeline(ApiPipelineExecution),
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
struct ApiPipelineExecution {
    pipeline: bool,
    stages: Vec<WorkflowExecution>,
    outputs: serde_json::Map<String, serde_json::Value>,
    artifacts: Vec<WorkflowArtifact>,
}
