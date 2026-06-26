use super::ApiResult;
use crate::workflow::{WorkflowExecution, WorkflowExecutionOptions, WorkflowSpec};
use std::collections::BTreeMap;
use std::path::Path;

mod artifacts;
mod context;
mod image;
mod leaf;
mod media;
mod png;
mod text;
mod types;

pub(super) fn execute_workflow_spec(
    root: &Path,
    workflow: &WorkflowSpec,
    workflows: &BTreeMap<String, WorkflowSpec>,
    options: WorkflowExecutionOptions,
) -> ApiResult<WorkflowExecution> {
    context::execute_workflow_spec(root, workflow, workflows, options)
}
