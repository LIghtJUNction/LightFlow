use crate::workflow::{ExecutionRuntime, WorkflowArtifact};

#[derive(Debug, Clone, PartialEq)]
pub(super) struct ChildWorkflowRun {
    pub(super) leaf: LeafExecution,
    pub(super) attempts: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct LeafExecution {
    pub(super) outputs: serde_json::Map<String, serde_json::Value>,
    pub(super) runtime: Option<ExecutionRuntime>,
    pub(super) artifacts: Vec<WorkflowArtifact>,
}
