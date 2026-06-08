//! OpenAPI-facing backend boundary.
//!
//! This module contains framework-independent service operations. HTTP, Unix
//! socket, or CLI frontends should call this layer instead of parsing assets or
//! run files themselves.

use crate::asset::{
    AssetError, AssetRecord, WorkflowDef, WorkflowRequestTemplate, WorkflowStepTarget,
    read_workflow_def,
};
use crate::cortex::{CortexHome, StepId, ThreadId, ToolId};
use crate::runs::{RunEvent, RunId, RunIdError, RunManifest, RunStepRecord, RunStore, RunTrace};
use crate::{compositions, models, nodes, workflows};
use cortex_core::ApiFormat;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::BTreeSet;
use std::fmt::{Display, Formatter};
use std::io;
use std::path::{Path, PathBuf};

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

    /// Create an initial run manifest under XDG state.
    pub fn create_run(&self, request: CreateRunRequest) -> ApiResult<RunManifest> {
        let run_id = self.planned_run_id(&request)?;
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
            workflow: workflow,
            definition,
            ready: issues.is_empty(),
            issues,
            steps,
        })
    }

    /// Read an existing run manifest.
    pub fn get_run(&self, run_id: &str) -> ApiResult<RunManifest> {
        let run_id = RunId::new(run_id.to_owned()).map_err(ApiError::from)?;
        self.runs.get_manifest(&run_id).map_err(ApiError::from)
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
        let body: Cow<'_, [u8]> = match body {
            Some(body) => Cow::Borrowed(body),
            None => Cow::Owned(self.render_step_request(&run_id, step_id)?),
        };
        let mut manifest = self.runs.get_manifest(&run_id).map_err(ApiError::from)?;
        let step = manifest
            .steps
            .iter_mut()
            .find(|step| step.step_id == step_id)
            .ok_or_else(|| ApiError::NotFound(format!("run step {step_id}")))?;
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
                let status_event = match outcome {
                    crate::cortex::CortexOutcome::Response { .. } => "step.succeeded",
                    crate::cortex::CortexOutcome::Error { .. } => "step.failed",
                };
                step.apply_outcome(outcome);
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
            }
        }
        self.runs.put_manifest(&manifest).map_err(ApiError::from)?;
        Ok(manifest)
    }

    /// Read run events as JSONL.
    pub fn run_events(&self, run_id: &str) -> ApiResult<String> {
        let run_id = RunId::new(run_id.to_owned()).map_err(ApiError::from)?;
        self.runs.events(&run_id).map_err(ApiError::from)
    }

    /// Read run trace as JSONL.
    pub fn run_trace(&self, run_id: &str) -> ApiResult<String> {
        let run_id = RunId::new(run_id.to_owned()).map_err(ApiError::from)?;
        self.runs.trace(&run_id).map_err(ApiError::from)
    }

    fn workflow_asset(&self, workflow_asset_id: &str) -> ApiResult<AssetRecord> {
        workflows::discover(&self.repo_root)?
            .into_iter()
            .find(|record| record.meta.id == workflow_asset_id)
            .ok_or_else(|| ApiError::NotFound(format!("workflow asset {workflow_asset_id}")))
    }

    fn planned_run_id(&self, request: &CreateRunRequest) -> ApiResult<RunId> {
        let run_id = request.run_id.clone().unwrap_or_else(|| {
            format!(
                "{}-{}",
                sanitize_id_fragment(&request.workflow_asset_id),
                std::process::id()
            )
        });
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
                if !definition.required_models.iter().any(|required| required == model_alias) {
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
                match request.inputs.as_object().and_then(|object| object.get(input_field)) {
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

/// Request body for `POST /runs`.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct CreateRunRequest {
    pub run_id: Option<String>,
    pub workflow_asset_id: String,
    #[serde(default)]
    pub inputs: serde_json::Value,
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

/// API-level error.
#[derive(Debug)]
pub enum ApiError {
    InvalidRequest(String),
    NotFound(String),
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
            Self::Io(_) => 500,
        }
    }
}

impl Display for ApiError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidRequest(message) => write!(f, "invalid request: {message}"),
            Self::NotFound(message) => write!(f, "not found: {message}"),
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

fn string_input<'a>(inputs: &'a serde_json::Value, field: &str) -> ApiResult<&'a str> {
    inputs
        .as_object()
        .and_then(|object| object.get(field))
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            ApiError::InvalidRequest(format!("run input field {field} must be a string"))
        })
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
        assert_eq!(
            fs::read_to_string(root.join("ctx/home/1000/api/openai.chat/inbox/draft.req.json"))?,
            r#"{"model":"demo"}"#
        );
        assert!(
            !root
                .join("ctx/home/1000/api/openai.chat/inbox/draft.tmp")
                .exists()
        );

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
        assert!(trace.contains("draft.req.json"));
        assert_eq!(service.run_events("run-001")?, events);
        assert_eq!(service.run_trace("run-001")?, trace);

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

        assert!(matches!(error, ApiError::NotFound(_)));
        assert_eq!(error.status_code(), 404);

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
