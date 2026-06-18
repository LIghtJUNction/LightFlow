//! Framework-independent LightFlow backend service.

mod deps;
mod dsl;
mod execution;
mod flux;
#[cfg(feature = "flux-native")]
mod flux_native;
mod llm_rig;
mod model_manager;
mod plan;
mod source;
mod util;
mod validation;
mod writer;

use crate::workflow::{
    WorkflowDependencyReport, WorkflowExecution, WorkflowExecutionOptions, WorkflowList,
    WorkflowSpec, WorkflowSummary, WorkflowValidation,
};
use deps::dependency_report;
use execution::execute_workflow_spec as execute_workflow_spec_impl;
use source::read_workflow_sources;
use std::collections::BTreeMap;
use std::fmt::{Display, Formatter};
use std::io;
use std::path::{Path, PathBuf};
use util::{validate_id_segment, workflow_crate_dir_name};
use validation::{validate_workflow_shape, validate_workflow_spec};
use writer::{workflow_source, write_text_atomic};

pub(super) const WORKFLOW_DIR: &str = "workflows";
pub(super) const LEGACY_LIGHTFLOW_DIR: &str = "lightflow";

/// Backend service state independent of any web framework.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ApiService {
    repo_root: PathBuf,
    workflow_paths: Vec<PathBuf>,
}

impl ApiService {
    /// Create a service rooted at a LightFlow repository.
    #[must_use]
    pub fn new(repo_root: impl Into<PathBuf>) -> Self {
        Self {
            repo_root: repo_root.into(),
            workflow_paths: Vec::new(),
        }
    }

    /// Add workflow search paths. Each path can point at a workflow collection,
    /// a LightFlow project root, or one workflow crate.
    #[must_use]
    pub fn with_workflow_paths(mut self, workflow_paths: Vec<PathBuf>) -> Self {
        self.workflow_paths = workflow_paths;
        self
    }

    /// Repository root used for project file discovery.
    #[must_use]
    pub fn repo_root(&self) -> &Path {
        &self.repo_root
    }

    /// List workflow specs.
    pub fn list_workflows(&self) -> ApiResult<WorkflowList> {
        let workflows = self
            .workflow_specs()?
            .into_values()
            .map(WorkflowSummary::from)
            .collect();
        Ok(WorkflowList { workflows })
    }

    /// Read one workflow spec.
    pub fn get_workflow(&self, workflow_id: &str) -> ApiResult<WorkflowSpec> {
        self.workflow_specs()?
            .remove(workflow_id)
            .ok_or_else(|| ApiError::NotFound(format!("workflow {workflow_id}")))
    }

    /// Save one workflow spec under
    /// `workflows/<category>/<short-name>/src/lib.rs`.
    pub fn save_workflow(&self, workflow: WorkflowSpec) -> ApiResult<WorkflowSpec> {
        let validation = self.validate_workflow(&workflow);
        if !validation.valid {
            return Err(ApiError::InvalidRequest(validation.issues.join("; ")));
        }
        let path = self.workflow_path(&workflow.id, workflow.category.as_deref())?;
        write_text_atomic(&path, &workflow_source(&workflow))?;
        Ok(workflow)
    }

    /// Validate a workflow against current workflow specs.
    pub fn validate_workflow(&self, workflow: &WorkflowSpec) -> WorkflowValidation {
        let mut workflows = self.workflow_specs().unwrap_or_default();
        workflows.insert(workflow.id.clone(), workflow.clone());
        let mut validation = validate_workflow_spec(workflow, &workflows);
        let dependencies = dependency_report(&workflow.id, &workflows);
        for cycle in dependencies.cycles {
            validation
                .issues
                .push(format!("workflow dependency cycle: {}", cycle.join(" -> ")));
        }
        for mismatch in dependencies.version_mismatches {
            validation.issues.push(format!(
                "workflow {} requires version {} but found {}",
                mismatch.workflow_id, mismatch.required, mismatch.found
            ));
        }
        validation.valid = validation.issues.is_empty();
        validation
    }

    /// Resolve the recursive workflow dependencies for a workflow.
    pub fn workflow_dependencies(&self, workflow_id: &str) -> ApiResult<WorkflowDependencyReport> {
        let workflows = self.workflow_specs()?;
        if !workflows.contains_key(workflow_id) {
            return Err(ApiError::NotFound(format!("workflow {workflow_id}")));
        }
        Ok(dependency_report(workflow_id, &workflows))
    }

    /// Execute a workflow using the current lightweight graph runner.
    pub fn execute_workflow(
        &self,
        workflow_id: &str,
        options: WorkflowExecutionOptions,
    ) -> ApiResult<WorkflowExecution> {
        let workflows = self.workflow_specs()?;
        let workflow = workflows
            .get(workflow_id)
            .ok_or_else(|| ApiError::NotFound(format!("workflow {workflow_id}")))?;
        execute_workflow_spec_impl(&self.repo_root, workflow, &workflows, options)
    }

    /// Execute an explicit workflow spec while resolving referenced workflows
    /// from the service's project and global workflow paths.
    pub fn execute_workflow_spec(
        &self,
        workflow: &WorkflowSpec,
        options: WorkflowExecutionOptions,
    ) -> ApiResult<WorkflowExecution> {
        let mut workflows = self.workflow_specs()?;
        workflows.insert(workflow.id.clone(), workflow.clone());
        execute_workflow_spec_impl(&self.repo_root, workflow, &workflows, options)
    }

    fn workflow_specs(&self) -> ApiResult<BTreeMap<String, WorkflowSpec>> {
        let mut workflows = BTreeMap::new();
        for workflow in read_workflow_sources(&self.repo_root, &self.workflow_paths)? {
            validate_workflow_shape(&workflow)?;
            workflows.entry(workflow.id.clone()).or_insert(workflow);
        }
        Ok(workflows)
    }

    fn workflow_path(&self, workflow_id: &str, category: Option<&str>) -> ApiResult<PathBuf> {
        validate_id_segment(workflow_id, "workflow id")?;
        let path = self.repo_root.join(WORKFLOW_DIR);
        let Some(category) = category else {
            return Err(ApiError::InvalidRequest(
                "workflow category is required for local workflow files".to_owned(),
            ));
        };
        validate_id_segment(category, "workflow category")?;
        Ok(path
            .join(category)
            .join(workflow_crate_dir_name(workflow_id))
            .join("src")
            .join("lib.rs"))
    }
}

/// API-level error.
#[derive(Debug)]
pub enum ApiError {
    InvalidRequest(String),
    NotFound(String),
    Io(io::Error),
}

impl ApiError {
    /// HTTP-style status code for adapters.
    #[must_use]
    pub const fn status_code(&self) -> u16 {
        match self {
            Self::InvalidRequest(_) => 400,
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
            Self::Io(error) => Display::fmt(error, f),
        }
    }
}

impl std::error::Error for ApiError {}

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
