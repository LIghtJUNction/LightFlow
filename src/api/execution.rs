use super::ApiResult;
use crate::api::source::WorkflowOrigin;
use crate::workflow::{WorkflowExecution, WorkflowExecutionOptions, WorkflowSpec};
use std::collections::BTreeMap;
use std::path::Path;

mod artifacts;
mod context;
mod image;
mod leaf;
mod media;
mod png;
mod types;

pub(super) fn execute_workflow_spec(
    root: &Path,
    workflow: &WorkflowSpec,
    workflows: &BTreeMap<String, WorkflowSpec>,
    origins: &BTreeMap<String, WorkflowOrigin>,
    options: WorkflowExecutionOptions,
) -> ApiResult<WorkflowExecution> {
    context::execute_workflow_spec(root, workflow, workflows, origins, options)
}
