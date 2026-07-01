use super::ModelLockFingerprint;
use crate::api::nodes::model_lock_read::{model_lock_status, read_model_lock};
use crate::workflow::WorkflowSpec;
use std::collections::BTreeMap;
use std::path::Path;

pub(in crate::api) fn model_lock_fingerprints(
    root: &Path,
    workflows: &BTreeMap<String, WorkflowSpec>,
    execution: &serde_json::Value,
) -> Vec<ModelLockFingerprint> {
    let lock = read_model_lock(root);
    let mut fingerprints = Vec::new();
    let mut contexts = Vec::new();
    collect_model_contexts(execution, None, &mut contexts);
    for context in contexts {
        let Some(workflow) = workflows.get(&context.workflow_id) else {
            continue;
        };
        for requirement in &workflow.models {
            fingerprints.push(ModelLockFingerprint {
                stage_index: context.stage_index,
                workflow_id: workflow.id.clone(),
                node_id: context.node_id.clone(),
                requirement_id: requirement.id.clone(),
                lock: model_lock_status(&lock, &workflow.id, &requirement.id),
            });
        }
    }
    fingerprints
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct ModelContext {
    stage_index: Option<usize>,
    workflow_id: String,
    node_id: Option<String>,
}

fn collect_model_contexts(
    execution: &serde_json::Value,
    stage_index: Option<usize>,
    contexts: &mut Vec<ModelContext>,
) {
    if execution
        .get("pipeline")
        .and_then(serde_json::Value::as_bool)
        == Some(true)
    {
        for (index, stage) in execution
            .get("stages")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .enumerate()
        {
            collect_model_contexts(stage, Some(index), contexts);
        }
        return;
    }

    if let Some(workflow_id) = execution
        .get("workflow_id")
        .and_then(serde_json::Value::as_str)
    {
        contexts.push(ModelContext {
            stage_index,
            workflow_id: workflow_id.to_owned(),
            node_id: None,
        });
    }

    for node in execution
        .get("nodes")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
    {
        let workflow_id = node
            .get("selected_workflow_id")
            .and_then(serde_json::Value::as_str)
            .or_else(|| node.get("workflow_id").and_then(serde_json::Value::as_str));
        let Some(workflow_id) = workflow_id else {
            continue;
        };
        contexts.push(ModelContext {
            stage_index,
            workflow_id: workflow_id.to_owned(),
            node_id: node
                .get("node_id")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned),
        });
    }
}
