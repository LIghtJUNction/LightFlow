//! Framework-independent LightFlow backend service.

mod agent_skill;
mod deps;
mod dsl;
mod error;
mod execution;
mod executors;
mod flux;
#[cfg(feature = "flux-native")]
mod flux_native;
#[cfg(feature = "flux-native")]
mod flux_native_session;
mod history;
mod llm_rig;
mod loop_check;
pub mod media_paths;
mod model_backend;
mod model_manager;
mod nodes;
mod patch_service;
mod patches;
mod plan;
mod project_config;
mod project_filter;
mod publish_checks;
mod release;
mod replay_fingerprints;
mod run_history_service;
mod service;
mod source;
mod util;
mod validation;
mod workflow_metadata;
mod writer;

use crate::workflow::{
    WorkflowDependencyReport, WorkflowExecution, WorkflowExecutionOptions, WorkflowList,
    WorkflowSpec, WorkflowSummary, WorkflowValidation,
};
pub(crate) use agent_skill::agent_skill_issues;
use deps::dependency_report;
pub(crate) use dsl::read_workflow_source;
use execution::execute_workflow_spec as execute_workflow_spec_impl;
pub(crate) use project_filter::project_filter_matches;
pub(crate) use publish_checks::{
    CargoManifestReadError, cargo_manifest_api_error, cargo_publish_command,
    internal_path_dependency_packages, package_field_value, publish_issues, read_cargo_manifest,
    read_workspace_cargo_manifest,
};
use source::read_workflow_sources;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use util::{validate_id_segment, workflow_crate_dir_name};
use validation::{validate_workflow_shape, validate_workflow_spec};
pub(crate) use workflow_metadata::{
    categorized_workflow_manifest_path, workflow_id_from_manifest, workflow_placeholder_issues,
    workflow_publish_metadata_issues,
};
use writer::{workflow_source, write_text_atomic};

pub(super) const WORKFLOW_DIR: &str = "workflows";
pub(super) const PROJECT_LIGHTFLOW_DIR: &str = ".lightflow";
pub(super) const LEGACY_LIGHTFLOW_DIR: &str = "lightflow";

pub use error::{ApiError, ApiResult};
pub use executors::{ExecutorCatalog, ExecutorInfo, executor_registry};
#[cfg(test)]
pub(crate) use history::write_history_fixture;
pub use history::{
    ArtifactCatalog, ArtifactListOptions, RecordedRun, RemovedRun, RunArtifact, RunCatalog,
    RunEvents, RunListOptions, RunStageRecord, RunTrace,
};
pub use loop_check::{
    LocalLoopCheck, LocalLoopReport, LocalLoopStatus, LoopChangeStatus, LoopChangesReport,
    ProjectWorkspaceCatalog, ProjectWorkspaceOptions, ProjectWorkspaceSummary,
    WorkflowChangeSummary, WorkflowPublishCatalog, WorkflowPublishCheck, WorkflowPublishOptions,
};
pub use nodes::{
    ModelCatalog, ModelListOptions, ModelLockFingerprint, ModelLockState, ModelLockStatus,
    ModelStatusFilter, NodeCard, NodeCatalog, NodeModelBinding, NodeModelCard, PortDirection,
};
pub use patches::{
    PatchCatalog, PatchSummary, PatchValidation, RegisteredPatch, RemovedPatch, SavedPatch,
};
pub use plan::{
    WorkflowPlan, WorkflowPlanAtom, WorkflowPlanNode, WorkflowPlannedModel, WorkflowRuntimePlan,
};
pub use release::{CheckProfile, ReleaseCheck, ReleaseCheckOptions, ReleaseCheckReport};
pub use service::ApiService;

impl ApiService {
    /// List workflow specs.
    pub fn list_workflows(&self) -> ApiResult<WorkflowList> {
        let workflows = self
            .workflow_specs()?
            .into_values()
            .map(WorkflowSummary::from)
            .collect();
        Ok(WorkflowList { workflows })
    }

    /// List workflow-backed nodes with editor-facing schema, runtime, and
    /// validation metadata.
    pub fn list_nodes(&self) -> ApiResult<NodeCatalog> {
        let workflows = self.workflow_specs()?;
        let executors = executor_registry();
        Ok(nodes::node_catalog(&workflows, &executors, |workflow| {
            node_validation_summary(workflow, &workflows)
        }))
    }

    /// Read one workflow-backed node card.
    pub fn get_node(&self, workflow_id: &str) -> ApiResult<NodeCard> {
        let workflows = self.workflow_specs()?;
        let executors = executor_registry();
        nodes::get_node_card(&workflows, &executors, workflow_id, |workflow| {
            node_validation_summary(workflow, &workflows)
        })
    }

    /// List runtime executors in the shared backend registry.
    pub fn list_executors(&self) -> ExecutorCatalog {
        ExecutorCatalog {
            executors: executor_registry(),
        }
    }

    /// List model requirements declared by available nodes.
    pub fn list_models(&self) -> ApiResult<ModelCatalog> {
        self.list_models_with_options(&ModelListOptions::default())
    }

    /// List model requirements declared by available nodes with optional filters.
    pub fn list_models_with_options(&self, options: &ModelListOptions) -> ApiResult<ModelCatalog> {
        let workflows = self.workflow_specs()?;
        if let Some(workflow_id) = options.workflow_id.as_deref()
            && !workflows.contains_key(workflow_id)
        {
            return Err(ApiError::NotFound(format!("workflow {workflow_id}")));
        }
        Ok(nodes::model_catalog(&self.repo_root, &workflows, options))
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

    /// Validate a workflow against current workflow specs.
    pub fn validate_workflow(&self, workflow: &WorkflowSpec) -> WorkflowValidation {
        let mut workflows = self.workflow_specs().unwrap_or_default();
        workflows.insert(workflow.id.clone(), workflow.clone());
        let mut validation = validate_workflow_spec(workflow, &workflows);
        let dependencies = dependency_report(&workflow.id, &workflows);
        validation.issues.extend(dependency_issues(&dependencies));
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

    /// Build the executor/model plan for a workflow without executing it.
    pub fn plan_workflow(&self, workflow_id: &str) -> ApiResult<WorkflowPlan> {
        let workflows = self.workflow_specs()?;
        let workflow = workflows
            .get(workflow_id)
            .ok_or_else(|| ApiError::NotFound(format!("workflow {workflow_id}")))?;
        validate_workflow_dependencies(workflow_id, &workflows)?;
        plan::build_workflow_plan(workflow, &workflows)
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
        validate_workflow_dependencies(workflow_id, &workflows)?;
        validate_execution_options(workflow, &workflows, &options)?;
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
        validate_workflow_dependencies(&workflow.id, &workflows)?;
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
}

fn validate_execution_options(
    workflow: &WorkflowSpec,
    workflows: &BTreeMap<String, WorkflowSpec>,
    options: &WorkflowExecutionOptions,
) -> ApiResult<()> {
    let node_ids = workflow
        .nodes
        .iter()
        .map(|node| node.id.as_str())
        .collect::<BTreeSet<_>>();
    let mut issues = Vec::new();

    for node_id in &options.disabled_nodes {
        if !node_ids.contains(node_id.as_str()) {
            issues.push(format!(
                "disabled node {node_id} does not match any node in workflow {}",
                workflow.id
            ));
        }
    }
    for node_id in &options.enabled_nodes {
        if !node_ids.contains(node_id.as_str()) {
            issues.push(format!(
                "enabled node {node_id} does not match any node in workflow {}",
                workflow.id
            ));
        }
    }
    if let Some(patch) = &options.patch {
        let validation = patches::validate_patch_for_workflow(patch, workflow, workflows);
        issues.extend(validation.issues);
    }

    if issues.is_empty() {
        Ok(())
    } else {
        Err(ApiError::InvalidRequest(format!(
            "invalid execution options for workflow {}: {}",
            workflow.id,
            issues.join("; ")
        )))
    }
}

fn node_validation_summary(
    workflow: &WorkflowSpec,
    workflows: &BTreeMap<String, WorkflowSpec>,
) -> nodes::NodeValidationSummary {
    let mut validation = validate_workflow_spec(workflow, workflows);
    let dependencies = dependency_report(&workflow.id, workflows);
    validation.issues.extend(dependency_issues(&dependencies));
    validation.valid = validation.issues.is_empty();
    nodes::NodeValidationSummary {
        valid: validation.valid,
        issues: validation.issues,
    }
}

fn validate_workflow_dependencies(
    workflow_id: &str,
    workflows: &BTreeMap<String, WorkflowSpec>,
) -> ApiResult<()> {
    let dependencies = dependency_report(workflow_id, workflows);
    let issues = dependency_issues(&dependencies);
    if issues.is_empty() {
        Ok(())
    } else {
        Err(ApiError::InvalidRequest(issues.join("; ")))
    }
}

fn dependency_issues(dependencies: &WorkflowDependencyReport) -> Vec<String> {
    let mut issues = Vec::new();
    for missing in &dependencies.missing_workflows {
        issues.push(format!("workflow dependency missing: {missing}"));
    }
    for cycle in &dependencies.cycles {
        issues.push(format!("workflow dependency cycle: {}", cycle.join(" -> ")));
    }
    for mismatch in &dependencies.version_mismatches {
        issues.push(format!(
            "workflow {} requires version {} but found {}",
            mismatch.workflow_id, mismatch.required, mismatch.found
        ));
    }
    issues
}
