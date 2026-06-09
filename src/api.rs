//! OpenAPI-facing backend boundary.
//!
//! This module contains framework-independent service operations. HTTP, Unix
//! socket, or CLI frontends should call this layer instead of parsing assets or
//! run files themselves.

use crate::asset::{
    AssetError, AssetRecord, WorkflowDef, WorkflowRequestTemplate, WorkflowStepTarget,
    read_workflow_def,
};
use crate::cortex::{CortexExchange, CortexHome, CtxAbi, StepId, ThreadId, ToolId};
use crate::runs::{
    RunEvent, RunId, RunIdError, RunManifest, RunStepRecord, RunStepStatus, RunStore, RunTrace,
};
use crate::{compositions, models, nodes, workflows};
use cortex_core::ApiFormat;
use petgraph::algo::toposort;
use petgraph::graph::{DiGraph, NodeIndex};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{Display, Formatter};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const DEFAULT_WORKFLOW_ID: &str = "workflow.default";

static AUTO_RUN_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Backend service state independent of any web framework.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ApiService {
    repo_root: PathBuf,
    runs: RunStore,
    cortex_home: CortexHome,
}

impl ApiService {
    /// Create a service rooted at a LightFlow repository and run store.
    #[must_use]
    pub fn new(repo_root: impl Into<PathBuf>, runs: RunStore) -> Self {
        Self {
            repo_root: repo_root.into(),
            runs,
            cortex_home: CortexHome::default_for_current_user(),
        }
    }

    /// Create a service with an explicit CortexFS home. Useful for tests.
    #[must_use]
    pub fn with_cortex_home(
        repo_root: impl Into<PathBuf>,
        runs: RunStore,
        cortex_home: CortexHome,
    ) -> Self {
        Self {
            repo_root: repo_root.into(),
            runs,
            cortex_home,
        }
    }

    /// Repository root used for asset discovery.
    #[must_use]
    pub fn repo_root(&self) -> &Path {
        &self.repo_root
    }

    /// Describe the `/ctx` userspace ABI used by this service.
    #[must_use]
    pub fn ctx_abi(&self) -> CtxAbi {
        self.cortex_home.abi()
    }

    /// List workflow assets.
    pub fn list_workflows(&self) -> ApiResult<AssetList> {
        workflows::discover(&self.repo_root)
            .map(AssetList::new)
            .map_err(ApiError::from)
    }

    /// List node assets.
    pub fn list_nodes(&self) -> ApiResult<AssetList> {
        nodes::discover(&self.repo_root)
            .map(AssetList::new)
            .map_err(ApiError::from)
    }

    /// List composition assets.
    pub fn list_compositions(&self) -> ApiResult<AssetList> {
        compositions::discover(&self.repo_root)
            .map(AssetList::new)
            .map_err(ApiError::from)
    }

    /// List model assets.
    pub fn list_models(&self) -> ApiResult<AssetList> {
        models::discover(&self.repo_root)
            .map(AssetList::new)
            .map_err(ApiError::from)
    }

    /// List runtime workflow DAGs available to MCP/UI clients.
    pub fn list_runtime_workflows(&self) -> ApiResult<RuntimeWorkflowList> {
        let mut workflows = BTreeMap::new();
        workflows.insert(DEFAULT_WORKFLOW_ID.to_owned(), default_runtime_workflow());
        for workflow in self.saved_runtime_workflows()? {
            workflows.insert(workflow.id.clone(), workflow);
        }
        Ok(RuntimeWorkflowList {
            workflows: workflows
                .into_values()
                .map(RuntimeWorkflowSummary::from)
                .collect(),
        })
    }

    /// Read one runtime workflow DAG.
    pub fn get_runtime_workflow(&self, workflow_id: &str) -> ApiResult<RuntimeWorkflow> {
        if workflow_id == DEFAULT_WORKFLOW_ID {
            return Ok(default_runtime_workflow());
        }
        self.saved_runtime_workflows()?
            .into_iter()
            .find(|workflow| workflow.id == workflow_id)
            .ok_or_else(|| ApiError::NotFound(format!("workflow {workflow_id}")))
    }

    /// Validate a runtime workflow DAG without saving or executing it.
    pub fn validate_runtime_workflow(
        &self,
        workflow: &RuntimeWorkflow,
    ) -> RuntimeWorkflowValidation {
        let (issues, topological_order) = runtime_workflow_check(workflow);
        RuntimeWorkflowValidation {
            valid: issues.is_empty(),
            issues,
            topological_order,
        }
    }

    /// Save a runtime workflow DAG under `lightflow/workflows/*.json`.
    pub fn save_runtime_workflow(
        &self,
        workflow: RuntimeWorkflow,
    ) -> ApiResult<RuntimeWorkflowSave> {
        let validation = self.validate_runtime_workflow(&workflow);
        if !validation.valid {
            return Err(ApiError::InvalidRequest(validation.issues.join("; ")));
        }
        if workflow.id == DEFAULT_WORKFLOW_ID {
            return Err(ApiError::InvalidRequest(
                "workflow.default is built in and cannot be overwritten".to_owned(),
            ));
        }
        let path = self.runtime_workflow_path(&workflow.id)?;
        write_json_atomic(&path, &workflow)?;
        Ok(RuntimeWorkflowSave { workflow, path })
    }

    /// Create an initial run manifest under XDG state.
    pub fn create_run(&self, request: CreateRunRequest) -> ApiResult<RunManifest> {
        let run_id = self.planned_run_id(&request)?;
        if self.runs.run_dir_exists(&run_id).map_err(ApiError::from)? {
            return Err(ApiError::Conflict(format!(
                "run {} already exists",
                run_id.as_str()
            )));
        }
        let workflow = self.workflow_asset(&request.workflow_asset_id)?;
        let definition = read_workflow_def(&workflow.source_path).map_err(ApiError::from)?;
        self.validate_workflow_references(&definition)?;
        let mut manifest = RunManifest::new(run_id, request.workflow_asset_id.clone());
        for step in &definition.steps {
            manifest.push_step(self.plan_step(step)?);
        }
        self.runs
            .put_request(&manifest.run_id, &request)
            .map_err(ApiError::from)?;
        self.runs
            .put_resolved_workflow(&manifest.run_id, &definition)
            .map_err(ApiError::from)?;
        self.runs
            .append_event(
                &manifest.run_id,
                RunEvent {
                    event: "run.created",
                    run_id: manifest.run_id.as_str(),
                    step_id: None,
                    detail: Some(manifest.workflow_asset_id.as_str()),
                },
            )
            .map_err(ApiError::from)?;
        self.runs.put_manifest(&manifest).map_err(ApiError::from)?;
        Ok(manifest)
    }

    /// List stored run manifests.
    pub fn list_runs(&self) -> ApiResult<RunList> {
        self.runs
            .list_manifests()
            .map(|runs| RunList { runs })
            .map_err(ApiError::from)
    }

    /// Preview a run without writing any XDG state.
    pub fn preview_run(&self, request: CreateRunRequest) -> ApiResult<RunPreview> {
        let run_id = self.planned_run_id(&request)?;
        let workflow = self.workflow_asset(&request.workflow_asset_id)?;
        let definition = read_workflow_def(&workflow.source_path).map_err(ApiError::from)?;
        let mut issues = self.workflow_issues(&definition);
        let mut steps = Vec::with_capacity(definition.steps.len());
        for step in &definition.steps {
            steps.push(self.preview_step(step, &request, &definition, &mut issues)?);
        }
        Ok(RunPreview {
            run_id,
            workflow,
            definition,
            ready: issues.is_empty(),
            issues,
            steps,
        })
    }

    /// Preview a runtime DAG run without writing state.
    pub fn preview_runtime_run(&self, request: RuntimeRunRequest) -> ApiResult<RuntimeRunPreview> {
        let run_id = self.planned_runtime_run_id(&request)?;
        let workflow = self.get_runtime_workflow(&request.workflow_id)?;
        let validation = self.validate_runtime_workflow(&workflow);
        Ok(RuntimeRunPreview {
            run_id,
            workflow,
            ready: validation.valid,
            issues: validation.issues,
        })
    }

    /// Create a runtime DAG run record without embedding an agent loop.
    pub fn create_runtime_run(&self, request: RuntimeRunRequest) -> ApiResult<RunManifest> {
        let run_id = self.planned_runtime_run_id(&request)?;
        if self.runs.run_dir_exists(&run_id).map_err(ApiError::from)? {
            return Err(ApiError::Conflict(format!(
                "run {} already exists",
                run_id.as_str()
            )));
        }
        let workflow = self.get_runtime_workflow(&request.workflow_id)?;
        let validation = self.validate_runtime_workflow(&workflow);
        if !validation.valid {
            return Err(ApiError::InvalidRequest(validation.issues.join("; ")));
        }
        let manifest = RunManifest::new(run_id, request.workflow_id.clone());
        self.runs
            .put_request(&manifest.run_id, &request)
            .map_err(ApiError::from)?;
        self.runs
            .put_resolved_workflow(&manifest.run_id, &workflow)
            .map_err(ApiError::from)?;
        self.runs
            .append_event(
                &manifest.run_id,
                RunEvent {
                    event: "run.created",
                    run_id: manifest.run_id.as_str(),
                    step_id: None,
                    detail: Some(manifest.workflow_asset_id.as_str()),
                },
            )
            .map_err(ApiError::from)?;
        self.runs.put_manifest(&manifest).map_err(ApiError::from)?;
        Ok(manifest)
    }

    /// Read an existing run manifest.
    pub fn get_run(&self, run_id: &str) -> ApiResult<RunManifest> {
        let run_id = RunId::new(run_id.to_owned()).map_err(ApiError::from)?;
        self.runs.get_manifest(&run_id).map_err(ApiError::from)
    }

    /// Read a derived status summary for an existing run.
    pub fn run_status(&self, run_id: &str) -> ApiResult<RunStatusSummary> {
        let manifest = self.get_run(run_id)?;
        Ok(RunStatusSummary::from_manifest(&manifest))
    }

    /// Mark all cancellable steps in a run as cancelled.
    pub fn cancel_run(&self, run_id: &str) -> ApiResult<RunManifest> {
        let run_id = RunId::new(run_id.to_owned()).map_err(ApiError::from)?;
        let mut manifest = self.runs.get_manifest(&run_id).map_err(ApiError::from)?;
        let mut changed = false;
        if !manifest.cancelled {
            manifest.cancelled = true;
            changed = true;
        }
        for step in &mut manifest.steps {
            if matches!(
                step.status,
                RunStepStatus::Planned | RunStepStatus::Submitted
            ) {
                step.status = RunStepStatus::Cancelled;
                changed = true;
            }
        }
        if changed {
            self.runs
                .append_event(
                    &manifest.run_id,
                    RunEvent {
                        event: "run.cancelled",
                        run_id: manifest.run_id.as_str(),
                        step_id: None,
                        detail: None,
                    },
                )
                .map_err(ApiError::from)?;
            self.runs.put_manifest(&manifest).map_err(ApiError::from)?;
        }
        Ok(manifest)
    }

    /// Read the original request used to create a run.
    pub fn run_request(&self, run_id: &str) -> ApiResult<CreateRunRequest> {
        let run_id = RunId::new(run_id.to_owned()).map_err(ApiError::from)?;
        self.runs.get_manifest(&run_id).map_err(ApiError::from)?;
        self.runs.get_request(&run_id).map_err(ApiError::from)
    }

    /// Read the workflow definition resolved when a run was created.
    pub fn run_workflow(&self, run_id: &str) -> ApiResult<WorkflowDef> {
        let run_id = RunId::new(run_id.to_owned()).map_err(ApiError::from)?;
        self.runs.get_manifest(&run_id).map_err(ApiError::from)?;
        self.runs
            .get_resolved_workflow(&run_id)
            .map_err(ApiError::from)
    }

    /// Submit one planned run step through CortexFS.
    ///
    /// If `body` is `None`, LightFlow renders a request from the stored run
    /// inputs and the workflow step's request template.
    pub fn submit_step(
        &self,
        run_id: &str,
        step_id: &str,
        body: Option<&[u8]>,
    ) -> ApiResult<RunManifest> {
        let run_id = RunId::new(run_id.to_owned()).map_err(ApiError::from)?;
        let mut manifest = self.runs.get_manifest(&run_id).map_err(ApiError::from)?;
        let step_index = manifest
            .steps
            .iter()
            .position(|step| step.step_id == step_id)
            .ok_or_else(|| ApiError::NotFound(format!("run step {step_id}")))?;
        let step = &manifest.steps[step_index];
        if step.status != RunStepStatus::Planned {
            return Err(ApiError::Conflict(format!(
                "run step {step_id} is already {}",
                step.status.as_str()
            )));
        }
        let body: Cow<'_, [u8]> = match body {
            Some(body) => Cow::Borrowed(body),
            None => Cow::Owned(self.render_step_request(&run_id, step_id)?),
        };
        let step = &mut manifest.steps[step_index];
        let submitted = step.cortex.submit_request(&body).map_err(ApiError::from)?;
        let submitted_path = submitted.request_path.clone();
        step.mark_submitted(submitted);
        self.runs
            .append_event(
                &manifest.run_id,
                RunEvent {
                    event: "step.submitted",
                    run_id: manifest.run_id.as_str(),
                    step_id: Some(step_id),
                    detail: submitted_path.to_str(),
                },
            )
            .map_err(ApiError::from)?;
        self.runs
            .append_trace(
                &manifest.run_id,
                RunTrace {
                    event: "cortex.request.committed",
                    step_id,
                    path: Some(&submitted_path),
                },
            )
            .map_err(ApiError::from)?;
        self.runs.put_manifest(&manifest).map_err(ApiError::from)?;
        Ok(manifest)
    }

    fn render_step_request(&self, run_id: &RunId, step_id: &str) -> ApiResult<Vec<u8>> {
        let request = self
            .runs
            .get_request::<CreateRunRequest>(run_id)
            .map_err(ApiError::from)?;
        let workflow = self
            .runs
            .get_resolved_workflow::<WorkflowDef>(run_id)
            .map_err(ApiError::from)?;
        let step = workflow
            .steps
            .iter()
            .find(|step| step.step_id == step_id)
            .ok_or_else(|| ApiError::NotFound(format!("workflow step {step_id}")))?;
        let template = step.request.as_ref().ok_or_else(|| {
            ApiError::InvalidRequest(format!(
                "no request body provided and step {step_id} does not declare a request template"
            ))
        })?;

        let value = match template {
            WorkflowRequestTemplate::OpenAiChatPrompt {
                model_alias,
                input_field,
            } => {
                if !workflow
                    .required_models
                    .iter()
                    .any(|required| required == model_alias)
                {
                    return Err(ApiError::InvalidRequest(format!(
                        "request template model alias {model_alias} is not listed in required_models"
                    )));
                }
                match &step.target {
                    WorkflowStepTarget::Api { format } if format == "openai.chat" => {}
                    _ => {
                        return Err(ApiError::InvalidRequest(format!(
                            "request template for step {step_id} requires an openai.chat API target"
                        )));
                    }
                }
                let prompt = string_input(&request.inputs, input_field)?;
                serde_json::json!({
                    "messages": [
                        {
                            "role": "user",
                            "content": prompt
                        }
                    ]
                })
            }
        };

        serde_json::to_vec(&value).map_err(|error| {
            ApiError::InvalidRequest(format!("failed to render request body: {error}"))
        })
    }

    /// Refresh all run steps from CortexFS outbox files.
    pub fn refresh_run(&self, run_id: &str) -> ApiResult<RunManifest> {
        let run_id = RunId::new(run_id.to_owned()).map_err(ApiError::from)?;
        let mut manifest = self.runs.get_manifest(&run_id).map_err(ApiError::from)?;
        for step in &mut manifest.steps {
            if let Some(outcome) = step.cortex.read_outcome().map_err(ApiError::from)? {
                let (status_event, trace_event, trace_path, status_changed) = match &outcome {
                    crate::cortex::CortexOutcome::Response { .. } => (
                        "step.succeeded",
                        "cortex.response.observed",
                        step.cortex.response.clone(),
                        step.status != RunStepStatus::Succeeded,
                    ),
                    crate::cortex::CortexOutcome::Error { .. } => (
                        "step.failed",
                        "cortex.error.observed",
                        step.cortex.error.clone(),
                        step.status != RunStepStatus::Failed,
                    ),
                };
                step.apply_outcome(outcome);
                if status_changed {
                    self.runs
                        .append_event(
                            &manifest.run_id,
                            RunEvent {
                                event: status_event,
                                run_id: manifest.run_id.as_str(),
                                step_id: Some(step.step_id.as_str()),
                                detail: step.fingerprint.as_deref(),
                            },
                        )
                        .map_err(ApiError::from)?;
                    self.runs
                        .append_trace(
                            &manifest.run_id,
                            RunTrace {
                                event: trace_event,
                                step_id: step.step_id.as_str(),
                                path: Some(&trace_path),
                            },
                        )
                        .map_err(ApiError::from)?;
                }
            }
        }
        self.runs.put_manifest(&manifest).map_err(ApiError::from)?;
        Ok(manifest)
    }

    /// Read run events as JSONL.
    pub fn run_events(&self, run_id: &str) -> ApiResult<String> {
        let run_id = RunId::new(run_id.to_owned()).map_err(ApiError::from)?;
        self.runs.get_manifest(&run_id).map_err(ApiError::from)?;
        self.runs.events(&run_id).map_err(ApiError::from)
    }

    /// Read run trace as JSONL.
    pub fn run_trace(&self, run_id: &str) -> ApiResult<String> {
        let run_id = RunId::new(run_id.to_owned()).map_err(ApiError::from)?;
        self.runs.get_manifest(&run_id).map_err(ApiError::from)?;
        self.runs.trace(&run_id).map_err(ApiError::from)
    }

    fn workflow_asset(&self, workflow_asset_id: &str) -> ApiResult<AssetRecord> {
        workflows::discover(&self.repo_root)?
            .into_iter()
            .find(|record| record.meta.id == workflow_asset_id)
            .ok_or_else(|| ApiError::NotFound(format!("workflow asset {workflow_asset_id}")))
    }

    fn saved_runtime_workflows(&self) -> ApiResult<Vec<RuntimeWorkflow>> {
        let mut workflows = Vec::new();
        match fs::read_dir(self.repo_root.join("lightflow").join("workflows")) {
            Ok(entries) => {
                for entry in entries {
                    let path = entry.map_err(ApiError::from)?.path();
                    if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
                        continue;
                    }
                    let workflow: RuntimeWorkflow = read_json(&path)?;
                    workflows.push(workflow);
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(ApiError::from(error)),
        }
        workflows.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(workflows)
    }

    fn runtime_workflow_path(&self, workflow_id: &str) -> ApiResult<PathBuf> {
        validate_id_segment(workflow_id, "workflow id")?;
        Ok(self
            .repo_root
            .join("lightflow")
            .join("workflows")
            .join(format!("{workflow_id}.json")))
    }

    fn planned_runtime_run_id(&self, request: &RuntimeRunRequest) -> ApiResult<RunId> {
        let run_id = request
            .run_id
            .clone()
            .unwrap_or_else(|| generated_run_id(&request.workflow_id));
        RunId::new(run_id).map_err(ApiError::from)
    }

    fn planned_run_id(&self, request: &CreateRunRequest) -> ApiResult<RunId> {
        let run_id = request
            .run_id
            .clone()
            .unwrap_or_else(|| generated_run_id(&request.workflow_asset_id));
        RunId::new(run_id).map_err(ApiError::from)
    }

    fn workflow_issues(&self, definition: &WorkflowDef) -> Vec<String> {
        let mut issues = Vec::new();
        let model_ids = match models::discover(&self.repo_root) {
            Ok(records) => records
                .into_iter()
                .map(|record| record.meta.id)
                .collect::<BTreeSet<_>>(),
            Err(error) => {
                issues.push(error.to_string());
                BTreeSet::new()
            }
        };
        for required_model in &definition.required_models {
            if !model_ids.contains(required_model) {
                issues.push(format!(
                    "workflow {} requires missing model asset {}",
                    definition.id, required_model
                ));
            }
        }

        let mut callable_ids = match nodes::discover(&self.repo_root) {
            Ok(records) => records
                .into_iter()
                .map(|record| record.meta.id)
                .collect::<BTreeSet<_>>(),
            Err(error) => {
                issues.push(error.to_string());
                BTreeSet::new()
            }
        };
        match compositions::discover(&self.repo_root) {
            Ok(records) => {
                callable_ids.extend(records.into_iter().map(|record| record.meta.id));
            }
            Err(error) => issues.push(error.to_string()),
        }
        for step in &definition.steps {
            if !callable_ids.contains(&step.node_or_composition_id) {
                issues.push(format!(
                    "workflow {} step {} references missing node or composition asset {}",
                    definition.id, step.step_id, step.node_or_composition_id
                ));
            }
        }

        issues
    }

    fn validate_workflow_references(&self, definition: &WorkflowDef) -> ApiResult<()> {
        let issues = self.workflow_issues(definition);
        if issues.is_empty() {
            Ok(())
        } else {
            Err(ApiError::InvalidRequest(issues.join("; ")))
        }
    }

    fn plan_step(&self, step: &crate::asset::WorkflowStepDef) -> ApiResult<RunStepRecord> {
        let step_id = StepId::new(step.step_id.clone()).map_err(|error| {
            ApiError::InvalidRequest(format!("invalid workflow step id: {error}"))
        })?;
        let cortex = match &step.target {
            WorkflowStepTarget::Api { format } => {
                let format = format
                    .parse::<ApiFormat>()
                    .map_err(|error| ApiError::InvalidRequest(error.to_string()))?;
                self.cortex_home.api_exchange(format, step_id)
            }
            WorkflowStepTarget::Tool { tool_id } => {
                let tool_id = ToolId::new(tool_id.clone()).map_err(|error| {
                    ApiError::InvalidRequest(format!("invalid workflow tool id: {error}"))
                })?;
                self.cortex_home.tool_exchange(tool_id, step_id)
            }
            WorkflowStepTarget::Thread { thread_id } => {
                let thread_id = ThreadId::new(thread_id.clone()).map_err(|error| {
                    ApiError::InvalidRequest(format!("invalid workflow thread id: {error}"))
                })?;
                self.cortex_home.thread_exchange(thread_id, step_id)
            }
        };
        Ok(RunStepRecord::planned(
            step.node_or_composition_id.clone(),
            cortex,
        ))
    }

    fn preview_step(
        &self,
        step: &crate::asset::WorkflowStepDef,
        request: &CreateRunRequest,
        definition: &WorkflowDef,
        issues: &mut Vec<String>,
    ) -> ApiResult<RunPreviewStep> {
        let step_id = StepId::new(step.step_id.clone()).map_err(|error| {
            ApiError::InvalidRequest(format!("invalid workflow step id: {error}"))
        })?;
        let cortex = match &step.target {
            WorkflowStepTarget::Api { format } => {
                let format = format
                    .parse::<ApiFormat>()
                    .map_err(|error| ApiError::InvalidRequest(error.to_string()))?;
                self.cortex_home.api_exchange(format, step_id)
            }
            WorkflowStepTarget::Tool { tool_id } => {
                let tool_id = ToolId::new(tool_id.clone()).map_err(|error| {
                    ApiError::InvalidRequest(format!("invalid workflow tool id: {error}"))
                })?;
                self.cortex_home.tool_exchange(tool_id, step_id)
            }
            WorkflowStepTarget::Thread { thread_id } => {
                let thread_id = ThreadId::new(thread_id.clone()).map_err(|error| {
                    ApiError::InvalidRequest(format!("invalid workflow thread id: {error}"))
                })?;
                self.cortex_home.thread_exchange(thread_id, step_id)
            }
        };

        let rendered_request = match &step.request {
            Some(WorkflowRequestTemplate::OpenAiChatPrompt {
                model_alias,
                input_field,
            }) => {
                if !definition
                    .required_models
                    .iter()
                    .any(|required| required == model_alias)
                {
                    issues.push(format!(
                        "request template model alias {model_alias} is not listed in required_models"
                    ));
                }
                match &step.target {
                    WorkflowStepTarget::Api { format } if format == "openai.chat" => {}
                    _ => issues.push(format!(
                        "request template for step {} requires an openai.chat API target",
                        step.step_id
                    )),
                }
                match request
                    .inputs
                    .as_object()
                    .and_then(|object| object.get(input_field))
                {
                    Some(value) if value.is_string() => Some(serde_json::json!({
                        "messages": [
                            {
                                "role": "user",
                                "content": value.as_str().unwrap_or_default()
                            }
                        ]
                    })),
                    Some(_) => {
                        issues.push(format!(
                            "run input field {input_field} must be a string for step {}",
                            step.step_id
                        ));
                        None
                    }
                    None => {
                        issues.push(format!(
                            "run input field {input_field} is required for step {}",
                            step.step_id
                        ));
                        None
                    }
                }
            }
            None => None,
        };

        Ok(RunPreviewStep {
            step_id: step.step_id.clone(),
            node_or_composition_id: step.node_or_composition_id.clone(),
            target: step.target.clone(),
            request_template: step.request.clone(),
            cortex,
            rendered_request,
        })
    }
}

/// List response for asset endpoints.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct AssetList {
    pub assets: Vec<AssetRecord>,
}

impl AssetList {
    /// Wrap discovered assets in the API response shape.
    #[must_use]
    pub const fn new(assets: Vec<AssetRecord>) -> Self {
        Self { assets }
    }
}

/// List response for stored run records.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct RunList {
    pub runs: Vec<RunManifest>,
}

/// List response for runtime workflow DAGs.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct RuntimeWorkflowList {
    pub workflows: Vec<RuntimeWorkflowSummary>,
}

/// Compact workflow row for resource browsers.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct RuntimeWorkflowSummary {
    pub id: String,
    pub name: String,
    pub nodes: usize,
    pub edges: usize,
}

impl From<RuntimeWorkflow> for RuntimeWorkflowSummary {
    fn from(workflow: RuntimeWorkflow) -> Self {
        Self {
            id: workflow.id,
            name: workflow.name,
            nodes: workflow.nodes.len(),
            edges: workflow.edges.len(),
        }
    }
}

/// LightFlow-native workflow graph exchanged over MCP.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct RuntimeWorkflow {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub nodes: Vec<RuntimeWorkflowNode>,
    #[serde(default)]
    pub edges: Vec<RuntimeWorkflowEdge>,
}

/// One node in a LightFlow workflow DAG.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct RuntimeWorkflowNode {
    pub id: String,
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub position: RuntimePosition,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub component: Option<String>,
    #[serde(default)]
    pub inputs: Vec<RuntimePort>,
    #[serde(default)]
    pub outputs: Vec<RuntimePort>,
}

/// Canvas position stored as workflow data, not UI product state.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
pub struct RuntimePosition {
    pub x: i64,
    pub y: i64,
}

/// Named typed node port.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct RuntimePort {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: String,
}

/// Directed edge between two node ports.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct RuntimeWorkflowEdge {
    pub from: RuntimeEndpoint,
    pub to: RuntimeEndpoint,
}

/// One side of a workflow edge.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct RuntimeEndpoint {
    pub node: String,
    pub port: String,
}

/// Validation result for a runtime workflow DAG.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct RuntimeWorkflowValidation {
    pub valid: bool,
    pub issues: Vec<String>,
    pub topological_order: Vec<String>,
}

/// Save response for a runtime workflow DAG.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct RuntimeWorkflowSave {
    pub workflow: RuntimeWorkflow,
    pub path: PathBuf,
}

/// Request body for `POST /runs`.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct CreateRunRequest {
    pub run_id: Option<String>,
    pub workflow_asset_id: String,
    #[serde(default)]
    pub inputs: serde_json::Value,
}

/// Runtime DAG run request used by MCP/UI clients.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct RuntimeRunRequest {
    pub run_id: Option<String>,
    pub workflow_id: String,
    #[serde(default)]
    pub inputs: serde_json::Value,
}

/// Runtime DAG preview response.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct RuntimeRunPreview {
    pub run_id: RunId,
    pub workflow: RuntimeWorkflow,
    pub ready: bool,
    pub issues: Vec<String>,
}

/// Preview response for a run request before any state is written.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct RunPreview {
    pub run_id: RunId,
    pub workflow: AssetRecord,
    pub definition: WorkflowDef,
    pub ready: bool,
    pub issues: Vec<String>,
    pub steps: Vec<RunPreviewStep>,
}

/// Preview information for one planned step.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct RunPreviewStep {
    pub step_id: String,
    pub node_or_composition_id: String,
    pub target: WorkflowStepTarget,
    pub request_template: Option<WorkflowRequestTemplate>,
    pub cortex: CortexExchange,
    pub rendered_request: Option<serde_json::Value>,
}

/// Derived lifecycle summary for one run.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct RunStatusSummary {
    pub run_id: RunId,
    pub workflow_asset_id: String,
    pub status: RunLifecycleStatus,
    pub total_steps: usize,
    pub planned_steps: usize,
    pub submitted_steps: usize,
    pub cancelled_steps: usize,
    pub succeeded_steps: usize,
    pub failed_steps: usize,
}

impl RunStatusSummary {
    fn from_manifest(manifest: &RunManifest) -> Self {
        let mut planned_steps = 0;
        let mut submitted_steps = 0;
        let mut cancelled_steps = 0;
        let mut succeeded_steps = 0;
        let mut failed_steps = 0;
        for step in &manifest.steps {
            match step.status {
                RunStepStatus::Planned => planned_steps += 1,
                RunStepStatus::Submitted => submitted_steps += 1,
                RunStepStatus::Cancelled => cancelled_steps += 1,
                RunStepStatus::Succeeded => succeeded_steps += 1,
                RunStepStatus::Failed => failed_steps += 1,
            }
        }
        let total_steps = manifest.steps.len();
        let status = if manifest.cancelled {
            RunLifecycleStatus::Cancelled
        } else if failed_steps > 0 {
            RunLifecycleStatus::Failed
        } else if cancelled_steps > 0 && planned_steps == 0 && submitted_steps == 0 {
            RunLifecycleStatus::Cancelled
        } else if total_steps > 0 && succeeded_steps == total_steps {
            RunLifecycleStatus::Succeeded
        } else if submitted_steps > 0 || succeeded_steps > 0 {
            RunLifecycleStatus::Running
        } else {
            RunLifecycleStatus::Planned
        };
        Self {
            run_id: manifest.run_id.clone(),
            workflow_asset_id: manifest.workflow_asset_id.clone(),
            status,
            total_steps,
            planned_steps,
            submitted_steps,
            cancelled_steps,
            succeeded_steps,
            failed_steps,
        }
    }
}

/// Derived lifecycle state for a whole run.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunLifecycleStatus {
    Planned,
    Running,
    Cancelled,
    Succeeded,
    Failed,
}

impl RunLifecycleStatus {
    /// Stable API string for the status.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Planned => "planned",
            Self::Running => "running",
            Self::Cancelled => "cancelled",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
        }
    }
}

/// API-level error.
#[derive(Debug)]
pub enum ApiError {
    InvalidRequest(String),
    NotFound(String),
    Conflict(String),
    Asset(AssetError),
    Io(io::Error),
}

impl ApiError {
    /// HTTP-style status code for future adapters.
    #[must_use]
    pub const fn status_code(&self) -> u16 {
        match self {
            Self::InvalidRequest(_) | Self::Asset(_) => 400,
            Self::NotFound(_) => 404,
            Self::Conflict(_) => 409,
            Self::Io(_) => 500,
        }
    }
}

impl Display for ApiError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidRequest(message) => write!(f, "invalid request: {message}"),
            Self::NotFound(message) => write!(f, "not found: {message}"),
            Self::Conflict(message) => write!(f, "conflict: {message}"),
            Self::Asset(error) => Display::fmt(error, f),
            Self::Io(error) => Display::fmt(error, f),
        }
    }
}

impl std::error::Error for ApiError {}

impl From<AssetError> for ApiError {
    fn from(error: AssetError) -> Self {
        Self::Asset(error)
    }
}

impl From<RunIdError> for ApiError {
    fn from(error: RunIdError) -> Self {
        Self::InvalidRequest(error.to_string())
    }
}

impl From<io::Error> for ApiError {
    fn from(error: io::Error) -> Self {
        if error.kind() == io::ErrorKind::NotFound {
            Self::NotFound(error.to_string())
        } else {
            Self::Io(error)
        }
    }
}

/// Service result.
pub type ApiResult<T> = Result<T, ApiError>;

fn sanitize_id_fragment(value: &str) -> String {
    let mut fragment = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_') {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    fragment.truncate(64);
    while fragment.starts_with(['.', '-', '_']) {
        fragment.remove(0);
    }
    while fragment.ends_with(['.', '-', '_']) {
        fragment.pop();
    }
    if fragment.is_empty() {
        "run".to_owned()
    } else {
        fragment
    }
}

fn generated_run_id(workflow_asset_id: &str) -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let counter = AUTO_RUN_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!(
        "{}-{timestamp}-{}-{counter}",
        sanitize_id_fragment(workflow_asset_id),
        std::process::id()
    )
}

fn string_input<'a>(inputs: &'a serde_json::Value, field: &str) -> ApiResult<&'a str> {
    inputs
        .as_object()
        .and_then(|object| object.get(field))
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            ApiError::InvalidRequest(format!("run input field {field} must be a string"))
        })
}

fn default_runtime_workflow() -> RuntimeWorkflow {
    RuntimeWorkflow {
        id: DEFAULT_WORKFLOW_ID.to_owned(),
        name: "Default Workflow".to_owned(),
        nodes: vec![
            RuntimeWorkflowNode {
                id: "input".to_owned(),
                kind: "input".to_owned(),
                title: Some("Workflow Input".to_owned()),
                position: RuntimePosition { x: 40, y: 120 },
                component: None,
                inputs: Vec::new(),
                outputs: vec![RuntimePort {
                    name: "workflow".to_owned(),
                    ty: "flow".to_owned(),
                }],
            },
            RuntimeWorkflowNode {
                id: "tool".to_owned(),
                kind: "mcp_tool".to_owned(),
                title: Some("MCP Tool".to_owned()),
                position: RuntimePosition { x: 300, y: 120 },
                component: None,
                inputs: vec![RuntimePort {
                    name: "workflow".to_owned(),
                    ty: "flow".to_owned(),
                }],
                outputs: vec![RuntimePort {
                    name: "tool_result".to_owned(),
                    ty: "json".to_owned(),
                }],
            },
            RuntimeWorkflowNode {
                id: "output".to_owned(),
                kind: "output".to_owned(),
                title: Some("Output".to_owned()),
                position: RuntimePosition { x: 560, y: 120 },
                component: None,
                inputs: vec![RuntimePort {
                    name: "tool_result".to_owned(),
                    ty: "json".to_owned(),
                }],
                outputs: Vec::new(),
            },
        ],
        edges: vec![
            RuntimeWorkflowEdge {
                from: RuntimeEndpoint {
                    node: "input".to_owned(),
                    port: "workflow".to_owned(),
                },
                to: RuntimeEndpoint {
                    node: "tool".to_owned(),
                    port: "workflow".to_owned(),
                },
            },
            RuntimeWorkflowEdge {
                from: RuntimeEndpoint {
                    node: "tool".to_owned(),
                    port: "tool_result".to_owned(),
                },
                to: RuntimeEndpoint {
                    node: "output".to_owned(),
                    port: "tool_result".to_owned(),
                },
            },
        ],
    }
}

fn runtime_workflow_check(workflow: &RuntimeWorkflow) -> (Vec<String>, Vec<String>) {
    let mut issues = Vec::new();
    if let Err(error) = validate_id_segment(&workflow.id, "workflow id") {
        issues.push(error.to_string());
    }
    if workflow.name.trim().is_empty() {
        issues.push(format!("workflow {} must have a name", workflow.id));
    }
    if workflow.nodes.is_empty() {
        issues.push(format!(
            "workflow {} must contain at least one node",
            workflow.id
        ));
    }

    let mut nodes = BTreeMap::<&str, &RuntimeWorkflowNode>::new();
    for node in &workflow.nodes {
        if let Err(error) = validate_id_segment(&node.id, "node id") {
            issues.push(error.to_string());
        }
        if node.kind.trim().is_empty() {
            issues.push(format!("node {} must have a kind", node.id));
        }
        if nodes.insert(node.id.as_str(), node).is_some() {
            issues.push(format!("duplicate node id {}", node.id));
        }
        push_duplicate_port_issues(&mut issues, "input", &node.id, &node.inputs);
        push_duplicate_port_issues(&mut issues, "output", &node.id, &node.outputs);
    }

    let mut graph = DiGraph::<&str, ()>::new();
    let mut graph_nodes = BTreeMap::<&str, NodeIndex>::new();
    for node in &workflow.nodes {
        graph_nodes
            .entry(node.id.as_str())
            .or_insert_with(|| graph.add_node(node.id.as_str()));
    }

    for edge in &workflow.edges {
        let Some(from_node) = nodes.get(edge.from.node.as_str()) else {
            issues.push(format!(
                "edge references missing source node {}",
                edge.from.node
            ));
            continue;
        };
        if !from_node
            .outputs
            .iter()
            .any(|port| port.name == edge.from.port)
        {
            issues.push(format!(
                "edge source {}.{} is not an output port",
                edge.from.node, edge.from.port
            ));
        }
        let Some(to_node) = nodes.get(edge.to.node.as_str()) else {
            issues.push(format!(
                "edge references missing target node {}",
                edge.to.node
            ));
            continue;
        };
        if !to_node.inputs.iter().any(|port| port.name == edge.to.port) {
            issues.push(format!(
                "edge target {}.{} is not an input port",
                edge.to.node, edge.to.port
            ));
        }
        if let (Some(from), Some(to)) = (
            graph_nodes.get(edge.from.node.as_str()),
            graph_nodes.get(edge.to.node.as_str()),
        ) {
            graph.add_edge(*from, *to, ());
        }
    }

    let topological_order = match toposort(&graph, None) {
        Ok(order) => order
            .into_iter()
            .filter_map(|node| graph.node_weight(node).copied())
            .map(ToOwned::to_owned)
            .collect(),
        Err(cycle) => {
            let node = graph
                .node_weight(cycle.node_id())
                .copied()
                .unwrap_or("unknown");
            issues.push(format!(
                "workflow {} contains a cycle involving node {node}",
                workflow.id
            ));
            Vec::new()
        }
    };

    (issues, topological_order)
}

fn push_duplicate_port_issues(
    issues: &mut Vec<String>,
    direction: &str,
    node_id: &str,
    ports: &[RuntimePort],
) {
    let mut names = BTreeSet::new();
    for port in ports {
        if port.name.trim().is_empty() {
            issues.push(format!("node {node_id} has an empty {direction} port name"));
        }
        if port.ty.trim().is_empty() {
            issues.push(format!(
                "node {node_id} port {} has an empty type",
                port.name
            ));
        }
        if !names.insert(port.name.as_str()) {
            issues.push(format!(
                "node {node_id} has duplicate {direction} port {}",
                port.name
            ));
        }
    }
}

fn validate_id_segment(value: &str, label: &str) -> ApiResult<()> {
    if value.is_empty()
        || value == "."
        || value == ".."
        || value.contains('/')
        || value.contains('\\')
    {
        return Err(ApiError::InvalidRequest(format!(
            "invalid {label} path segment: {value}"
        )));
    }
    Ok(())
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> ApiResult<T> {
    let file = fs::File::open(path).map_err(ApiError::from)?;
    serde_json::from_reader(file)
        .map_err(|error| ApiError::InvalidRequest(format!("invalid JSON in {:?}: {error}", path)))
}

fn write_json_atomic(path: &Path, value: &impl Serialize) -> ApiResult<()> {
    let parent = path
        .parent()
        .ok_or_else(|| ApiError::InvalidRequest("json path has no parent".to_owned()))?;
    fs::create_dir_all(parent).map_err(ApiError::from)?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| ApiError::InvalidRequest("json path has no file name".to_owned()))?;
    let temp_path = parent.join(format!("{file_name}.tmp"));
    let mut file = fs::File::create(&temp_path).map_err(ApiError::from)?;
    serde_json::to_writer_pretty(&mut file, value)
        .map_err(|error| ApiError::InvalidRequest(format!("failed to encode JSON: {error}")))?;
    file.write_all(b"\n").map_err(ApiError::from)?;
    file.sync_all().map_err(ApiError::from)?;
    drop(file);
    fs::rename(temp_path, path).map_err(ApiError::from)
}

#[cfg(test)]
mod tests {
    use super::{ApiError, ApiService, CreateRunRequest};
    use crate::cortex::CortexHome;
    use crate::runs::{RunStepStatus, RunStore, RuntimeDirs};
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn service_lists_assets_through_backend_discovery() -> Result<(), Box<dyn std::error::Error>> {
        let root = unique_temp_root();
        write_asset(
            &root.join("lightflow").join("workflows").join("demo.rs"),
            "workflow.demo",
            "Demo",
            "Workflow",
        )?;
        write_asset(
            &root.join("lightflow").join("models").join("planner.rs"),
            "model.planner",
            "Planner",
            "Model",
        )?;

        let service = service_for_root(&root);

        assert_eq!(service.list_workflows()?.assets[0].meta.id, "workflow.demo");
        assert_eq!(service.list_models()?.assets[0].meta.id, "model.planner");
        assert!(service.list_nodes()?.assets.is_empty());
        assert!(service.list_compositions()?.assets.is_empty());

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn service_creates_and_reads_xdg_run_manifest() -> Result<(), Box<dyn std::error::Error>> {
        let root = unique_temp_root();
        write_asset(
            &root.join("lightflow").join("workflows").join("demo.rs"),
            "workflow.demo",
            "Demo",
            "Workflow",
        )?;
        write_default_node(&root)?;
        let service = service_for_root(&root);

        let manifest = service.create_run(CreateRunRequest {
            run_id: Some("run-001".to_owned()),
            workflow_asset_id: "workflow.demo".to_owned(),
            inputs: serde_json::json!({"prompt": "hello"}),
        })?;
        let loaded = service.get_run("run-001")?;

        assert_eq!(manifest, loaded);
        assert_eq!(loaded.workflow_asset_id, "workflow.demo");
        assert_eq!(loaded.steps.len(), 1);
        assert_eq!(loaded.steps[0].step_id, "draft");
        assert_eq!(
            service.run_request("run-001")?.inputs,
            serde_json::json!({"prompt": "hello"})
        );
        assert_eq!(service.run_workflow("run-001")?.id, "workflow.demo");
        let status = service.run_status("run-001")?;
        assert_eq!(status.status, super::RunLifecycleStatus::Planned);
        assert_eq!(status.total_steps, 1);
        assert_eq!(status.planned_steps, 1);
        assert_eq!(
            loaded.steps[0].cortex.commit_request,
            root.join("ctx/home/1000/api/openai.chat/inbox/draft.req.json")
        );
        assert!(
            root.join("state/lightflow/runs/run-001/request.json")
                .is_file()
        );
        assert!(
            root.join("state/lightflow/runs/run-001/resolved_workflow.json")
                .is_file()
        );
        assert!(
            fs::read_to_string(root.join("state/lightflow/runs/run-001/events.jsonl"))?
                .contains("\"event\":\"run.created\"")
        );

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn service_rejects_duplicate_run_id_without_overwriting_state()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = unique_temp_root();
        write_asset(
            &root.join("lightflow").join("workflows").join("demo.rs"),
            "workflow.demo",
            "Demo",
            "Workflow",
        )?;
        write_default_node(&root)?;
        let service = service_for_root(&root);
        let manifest = service.create_run(CreateRunRequest {
            run_id: Some("run-001".to_owned()),
            workflow_asset_id: "workflow.demo".to_owned(),
            inputs: serde_json::json!({"prompt": "first"}),
        })?;

        let error = service
            .create_run(CreateRunRequest {
                run_id: Some("run-001".to_owned()),
                workflow_asset_id: "workflow.demo".to_owned(),
                inputs: serde_json::json!({"prompt": "second"}),
            })
            .unwrap_err();

        assert!(matches!(error, ApiError::Conflict(_)));
        assert_eq!(error.status_code(), 409);
        assert_eq!(service.get_run("run-001")?, manifest);
        let request = fs::read_to_string(root.join("state/lightflow/runs/run-001/request.json"))?;
        assert!(request.contains("\"first\""));
        assert!(!request.contains("\"second\""));

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn service_rejects_existing_partial_run_directory() -> Result<(), Box<dyn std::error::Error>> {
        let root = unique_temp_root();
        write_asset(
            &root.join("lightflow").join("workflows").join("demo.rs"),
            "workflow.demo",
            "Demo",
            "Workflow",
        )?;
        write_default_node(&root)?;
        fs::create_dir_all(root.join("state/lightflow/runs/run-001"))?;
        fs::write(
            root.join("state/lightflow/runs/run-001/request.json"),
            "{\"partial\":true}\n",
        )?;
        let service = service_for_root(&root);

        let error = service
            .create_run(CreateRunRequest {
                run_id: Some("run-001".to_owned()),
                workflow_asset_id: "workflow.demo".to_owned(),
                inputs: serde_json::json!({"prompt": "replacement"}),
            })
            .unwrap_err();

        assert!(matches!(error, ApiError::Conflict(_)));
        assert_eq!(error.status_code(), 409);
        assert_eq!(
            fs::read_to_string(root.join("state/lightflow/runs/run-001/request.json"))?,
            "{\"partial\":true}\n"
        );
        assert!(
            !root
                .join("state/lightflow/runs/run-001/manifest.json")
                .exists()
        );

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn service_lists_runs_from_xdg_state_in_id_order() -> Result<(), Box<dyn std::error::Error>> {
        let root = unique_temp_root();
        write_asset(
            &root.join("lightflow").join("workflows").join("demo.rs"),
            "workflow.demo",
            "Demo",
            "Workflow",
        )?;
        write_default_node(&root)?;
        let service = service_for_root(&root);

        assert!(service.list_runs()?.runs.is_empty());

        service.create_run(CreateRunRequest {
            run_id: Some("run-b".to_owned()),
            workflow_asset_id: "workflow.demo".to_owned(),
            inputs: serde_json::Value::Null,
        })?;
        service.create_run(CreateRunRequest {
            run_id: Some("run-a".to_owned()),
            workflow_asset_id: "workflow.demo".to_owned(),
            inputs: serde_json::Value::Null,
        })?;

        let list = service.list_runs()?;

        assert_eq!(list.runs.len(), 2);
        assert_eq!(list.runs[0].run_id.as_str(), "run-a");
        assert_eq!(list.runs[1].run_id.as_str(), "run-b");

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn service_generates_distinct_run_ids_without_explicit_id()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = unique_temp_root();
        write_asset(
            &root.join("lightflow").join("workflows").join("demo.rs"),
            "workflow.demo",
            "Demo",
            "Workflow",
        )?;
        write_default_node(&root)?;
        let service = service_for_root(&root);

        let first = service.create_run(CreateRunRequest {
            run_id: None,
            workflow_asset_id: "workflow.demo".to_owned(),
            inputs: serde_json::json!({"prompt": "first"}),
        })?;
        let second = service.create_run(CreateRunRequest {
            run_id: None,
            workflow_asset_id: "workflow.demo".to_owned(),
            inputs: serde_json::json!({"prompt": "second"}),
        })?;

        assert_ne!(first.run_id, second.run_id);
        assert!(first.run_id.as_str().starts_with("workflow.demo-"));
        assert!(second.run_id.as_str().starts_with("workflow.demo-"));
        assert_eq!(service.get_run(first.run_id.as_str())?, first);
        assert_eq!(service.get_run(second.run_id.as_str())?, second);
        assert!(
            root.join("state/lightflow/runs")
                .join(first.run_id.as_str())
                .join("request.json")
                .is_file()
        );
        assert!(
            root.join("state/lightflow/runs")
                .join(second.run_id.as_str())
                .join("request.json")
                .is_file()
        );

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn service_submits_step_and_refreshes_outbox_state() -> Result<(), Box<dyn std::error::Error>> {
        let root = unique_temp_root();
        write_asset(
            &root.join("lightflow").join("workflows").join("demo.rs"),
            "workflow.demo",
            "Demo",
            "Workflow",
        )?;
        write_default_node(&root)?;
        let service = service_for_root(&root);
        service.create_run(CreateRunRequest {
            run_id: Some("run-001".to_owned()),
            workflow_asset_id: "workflow.demo".to_owned(),
            inputs: serde_json::Value::Null,
        })?;

        let submitted = service.submit_step("run-001", "draft", Some(br#"{"model":"demo"}"#))?;

        assert_eq!(submitted.steps[0].status, RunStepStatus::Submitted);
        let submitted_status = service.run_status("run-001")?;
        assert_eq!(submitted_status.status, super::RunLifecycleStatus::Running);
        assert_eq!(submitted_status.submitted_steps, 1);
        assert_eq!(
            fs::read_to_string(root.join("ctx/home/1000/api/openai.chat/inbox/draft.req.json"))?,
            r#"{"model":"demo"}"#
        );
        assert!(
            !root
                .join("ctx/home/1000/api/openai.chat/inbox/draft.tmp")
                .exists()
        );

        let duplicate_submit = service
            .submit_step("run-001", "draft", Some(br#"{"model":"second"}"#))
            .unwrap_err();
        assert!(matches!(duplicate_submit, ApiError::Conflict(_)));
        assert_eq!(duplicate_submit.status_code(), 409);
        assert_eq!(
            fs::read_to_string(root.join("ctx/home/1000/api/openai.chat/inbox/draft.req.json"))?,
            r#"{"model":"demo"}"#
        );
        let duplicate_generated_submit = service.submit_step("run-001", "draft", None).unwrap_err();
        assert!(matches!(duplicate_generated_submit, ApiError::Conflict(_)));
        assert_eq!(duplicate_generated_submit.status_code(), 409);

        let outbox = root.join("ctx/home/1000/api/openai.chat/outbox");
        fs::create_dir_all(&outbox)?;
        fs::write(outbox.join("draft.resp.json"), "{\"ok\":true}\n")?;
        fs::write(outbox.join("draft.fingerprint"), "fnv1a64:abc\n")?;
        fs::write(
            outbox.join("draft.route.json"),
            "{\"provider\":\"local\",\"model\":\"smollm2:135m\",\"reason\":\"default_provider\"}\n",
        )?;

        let refreshed = service.refresh_run("run-001")?;

        assert_eq!(refreshed.steps[0].status, RunStepStatus::Succeeded);
        let refreshed_status = service.run_status("run-001")?;
        assert_eq!(
            refreshed_status.status,
            super::RunLifecycleStatus::Succeeded
        );
        assert_eq!(refreshed_status.succeeded_steps, 1);
        assert_eq!(
            refreshed.steps[0].fingerprint.as_deref(),
            Some("fnv1a64:abc")
        );
        assert_eq!(refreshed.steps[0].provider_id.as_deref(), Some("local"));
        assert_eq!(refreshed.steps[0].model_id.as_deref(), Some("smollm2:135m"));
        assert_eq!(
            refreshed.steps[0].route_decision.as_deref(),
            Some("default_provider")
        );
        assert_eq!(
            refreshed.steps[0].response_path,
            Some(outbox.join("draft.resp.json"))
        );
        let events = fs::read_to_string(root.join("state/lightflow/runs/run-001/events.jsonl"))?;
        assert!(events.contains("\"event\":\"step.submitted\""));
        assert!(events.contains("\"event\":\"step.succeeded\""));
        let trace = fs::read_to_string(root.join("state/lightflow/runs/run-001/trace.jsonl"))?;
        assert!(trace.contains("\"event\":\"cortex.request.committed\""));
        assert!(trace.contains("\"event\":\"cortex.response.observed\""));
        assert!(trace.contains("draft.req.json"));
        assert!(trace.contains("draft.resp.json"));
        assert_eq!(service.run_events("run-001")?, events);
        assert_eq!(service.run_trace("run-001")?, trace);

        let refreshed_again = service.refresh_run("run-001")?;
        let events_after_second_refresh =
            fs::read_to_string(root.join("state/lightflow/runs/run-001/events.jsonl"))?;
        let trace_after_second_refresh =
            fs::read_to_string(root.join("state/lightflow/runs/run-001/trace.jsonl"))?;
        assert_eq!(refreshed_again.steps[0].status, RunStepStatus::Succeeded);
        assert_eq!(events_after_second_refresh, events);
        assert_eq!(trace_after_second_refresh, trace);

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn service_refresh_records_error_outcome_trace() -> Result<(), Box<dyn std::error::Error>> {
        let root = unique_temp_root();
        write_asset(
            &root.join("lightflow").join("workflows").join("demo.rs"),
            "workflow.demo",
            "Demo",
            "Workflow",
        )?;
        write_default_node(&root)?;
        let service = service_for_root(&root);
        service.create_run(CreateRunRequest {
            run_id: Some("run-001".to_owned()),
            workflow_asset_id: "workflow.demo".to_owned(),
            inputs: serde_json::Value::Null,
        })?;
        service.submit_step("run-001", "draft", Some(br#"{"model":"demo"}"#))?;

        let outbox = root.join("ctx/home/1000/api/openai.chat/outbox");
        fs::create_dir_all(&outbox)?;
        fs::write(
            outbox.join("draft.error"),
            "{\"error\":\"provider down\"}\n",
        )?;
        fs::write(outbox.join("draft.fingerprint"), "fnv1a64:err\n")?;
        fs::write(
            outbox.join("draft.route.json"),
            "{\"provider\":\"local\",\"model\":\"test-model\",\"reason\":\"provider_error\"}\n",
        )?;

        let refreshed = service.refresh_run("run-001")?;

        assert_eq!(refreshed.steps[0].status, RunStepStatus::Failed);
        assert_eq!(
            refreshed.steps[0].error_path,
            Some(outbox.join("draft.error"))
        );
        assert_eq!(
            refreshed.steps[0].fingerprint.as_deref(),
            Some("fnv1a64:err")
        );
        assert_eq!(
            refreshed.steps[0].route_decision.as_deref(),
            Some("provider_error")
        );
        let events = fs::read_to_string(root.join("state/lightflow/runs/run-001/events.jsonl"))?;
        assert!(events.contains("\"event\":\"step.failed\""));
        let trace = fs::read_to_string(root.join("state/lightflow/runs/run-001/trace.jsonl"))?;
        assert!(trace.contains("\"event\":\"cortex.error.observed\""));
        assert!(trace.contains("draft.error"));

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn service_can_submit_generated_request_from_run_inputs()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = unique_temp_root();
        write_templated_workflow_asset(
            &root.join("lightflow").join("workflows").join("demo.rs"),
            "workflow.demo",
        )?;
        write_default_node(&root)?;
        write_default_model(&root)?;
        let service = service_for_root(&root);
        service.create_run(CreateRunRequest {
            run_id: Some("run-001".to_owned()),
            workflow_asset_id: "workflow.demo".to_owned(),
            inputs: serde_json::json!({"prompt": "write the migration plan"}),
        })?;

        let submitted = service.submit_step("run-001", "draft", None)?;

        assert_eq!(submitted.steps[0].status, RunStepStatus::Submitted);
        let body =
            fs::read_to_string(root.join("ctx/home/1000/api/openai.chat/inbox/draft.req.json"))?;
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&body)?,
            serde_json::json!({
                "messages": [
                    {
                        "role": "user",
                        "content": "write the migration plan"
                    }
                ]
            })
        );

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn service_rejects_generated_submit_without_template() -> Result<(), Box<dyn std::error::Error>>
    {
        let root = unique_temp_root();
        write_asset(
            &root.join("lightflow").join("workflows").join("demo.rs"),
            "workflow.demo",
            "Demo",
            "Workflow",
        )?;
        write_default_node(&root)?;
        let service = service_for_root(&root);
        service.create_run(CreateRunRequest {
            run_id: Some("run-001".to_owned()),
            workflow_asset_id: "workflow.demo".to_owned(),
            inputs: serde_json::json!({"prompt": "hello"}),
        })?;

        let error = service.submit_step("run-001", "draft", None).unwrap_err();

        assert!(matches!(error, ApiError::InvalidRequest(_)));

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn service_maps_bad_run_id_to_invalid_request() {
        let root = unique_temp_root();
        let service = service_for_root(&root);

        let error = service
            .create_run(CreateRunRequest {
                run_id: Some("bad/id".to_owned()),
                workflow_asset_id: "workflow.demo".to_owned(),
                inputs: serde_json::Value::Null,
            })
            .unwrap_err();

        assert!(matches!(error, ApiError::InvalidRequest(_)));
        assert_eq!(error.status_code(), 400);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn service_maps_missing_run_to_not_found() {
        let root = unique_temp_root();
        let service = service_for_root(&root);

        let error = service.get_run("missing").unwrap_err();
        let status_error = service.run_status("missing").unwrap_err();
        let request_error = service.run_request("missing").unwrap_err();
        let workflow_error = service.run_workflow("missing").unwrap_err();

        assert!(matches!(error, ApiError::NotFound(_)));
        assert_eq!(error.status_code(), 404);
        assert!(matches!(status_error, ApiError::NotFound(_)));
        assert_eq!(status_error.status_code(), 404);
        assert!(matches!(request_error, ApiError::NotFound(_)));
        assert_eq!(request_error.status_code(), 404);
        assert!(matches!(workflow_error, ApiError::NotFound(_)));
        assert_eq!(workflow_error.status_code(), 404);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn service_maps_missing_event_and_trace_streams_to_not_found() {
        let root = unique_temp_root();
        let service = service_for_root(&root);

        let events_error = service.run_events("missing").unwrap_err();
        let trace_error = service.run_trace("missing").unwrap_err();

        assert!(matches!(events_error, ApiError::NotFound(_)));
        assert_eq!(events_error.status_code(), 404);
        assert!(matches!(trace_error, ApiError::NotFound(_)));
        assert_eq!(trace_error.status_code(), 404);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn service_rejects_workflow_with_missing_model_asset() -> Result<(), Box<dyn std::error::Error>>
    {
        let root = unique_temp_root();
        write_templated_workflow_asset(
            &root.join("lightflow").join("workflows").join("demo.rs"),
            "workflow.demo",
        )?;
        write_default_node(&root)?;
        let service = service_for_root(&root);

        let error = service
            .create_run(CreateRunRequest {
                run_id: Some("run-001".to_owned()),
                workflow_asset_id: "workflow.demo".to_owned(),
                inputs: serde_json::json!({"prompt": "hello"}),
            })
            .unwrap_err();

        assert!(matches!(error, ApiError::InvalidRequest(_)));
        assert!(
            !root
                .join("state/lightflow/runs/run-001/request.json")
                .exists()
        );

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn service_rejects_workflow_with_missing_step_asset() -> Result<(), Box<dyn std::error::Error>>
    {
        let root = unique_temp_root();
        write_asset(
            &root.join("lightflow").join("workflows").join("demo.rs"),
            "workflow.demo",
            "Demo",
            "Workflow",
        )?;
        let service = service_for_root(&root);

        let error = service
            .create_run(CreateRunRequest {
                run_id: Some("run-001".to_owned()),
                workflow_asset_id: "workflow.demo".to_owned(),
                inputs: serde_json::Value::Null,
            })
            .unwrap_err();

        assert!(matches!(error, ApiError::InvalidRequest(_)));
        assert!(
            !root
                .join("state/lightflow/runs/run-001/request.json")
                .exists()
        );

        fs::remove_dir_all(root)?;
        Ok(())
    }

    fn service_for_root(root: &std::path::Path) -> ApiService {
        let dirs = RuntimeDirs::new(
            root.join("cfg"),
            root.join("state"),
            root.join("cache"),
            root.join("runtime"),
        );
        ApiService::with_cortex_home(
            root,
            RunStore::new(dirs),
            CortexHome::new(root.join("ctx"), 1000),
        )
    }

    fn write_asset(
        path: &std::path::Path,
        id: &str,
        title: &str,
        kind: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let parent = path.parent().ok_or("asset path has no parent")?;
        fs::create_dir_all(parent)?;
        fs::write(
            path,
            format!(
                r#"
use lightflow::asset::*;

pub const META: AssetMeta = AssetMeta {{
    id: "{id}",
    title: "{title}",
    kind: AssetKind::{kind},
    description: "Test asset.",
    stability: Stability::Draft,
}};

pub fn define() -> WorkflowDef {{
    workflow(META.id).api_step("draft", "node.llm_prompt", "openai.chat")
}}
"#
            ),
        )?;
        Ok(())
    }

    fn write_default_node(root: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
        write_asset(
            &root.join("lightflow").join("nodes").join("llm_prompt.rs"),
            "node.llm_prompt",
            "LLM Prompt",
            "Node",
        )
    }

    fn write_default_model(root: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
        write_asset(
            &root.join("lightflow").join("models").join("llm_planner.rs"),
            "llm.planner",
            "Planner LLM",
            "Model",
        )
    }

    fn write_templated_workflow_asset(
        path: &std::path::Path,
        id: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let parent = path.parent().ok_or("asset path has no parent")?;
        fs::create_dir_all(parent)?;
        fs::write(
            path,
            format!(
                r#"
use lightflow::asset::*;

pub const META: AssetMeta = AssetMeta {{
    id: "{id}",
    title: "Demo",
    kind: AssetKind::Workflow,
    description: "Test asset.",
    stability: Stability::Draft,
}};

pub fn define() -> WorkflowDef {{
    workflow(META.id)
        .required_model("llm.planner")
        .api_step("draft", "node.llm_prompt", "openai.chat")
        .openai_chat_input("llm.planner", "prompt")
}}
"#
            ),
        )?;
        Ok(())
    }

    fn unique_temp_root() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock must be after unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("lightflow-api-test-{}-{nanos}", std::process::id()))
    }
}
