use super::{
    ModelCatalog, ModelListOptions, ModelLockState, ModelStatusFilter, NodeModelBinding,
    NodeModelCard, PortDirection,
};
use crate::api::nodes::model_lock_read::{model_lock_status, read_model_lock};
use crate::workflow::WorkflowSpec;
use std::collections::BTreeMap;
use std::path::Path;

pub(in crate::api) fn model_catalog(
    root: &Path,
    workflows: &BTreeMap<String, WorkflowSpec>,
    options: &ModelListOptions,
) -> ModelCatalog {
    let lock = read_model_lock(root);
    let mut models = Vec::new();
    let mut issues = Vec::new();
    for workflow in workflows.values() {
        if options
            .workflow_id
            .as_deref()
            .is_some_and(|workflow_id| workflow.id != workflow_id)
        {
            continue;
        }
        for requirement in &workflow.models {
            let lock = model_lock_status(&lock, &workflow.id, &requirement.id);
            let blocked = lock.status != ModelLockState::Available;
            match options.status {
                ModelStatusFilter::All => {}
                ModelStatusFilter::Available if blocked => continue,
                ModelStatusFilter::Blocked if !blocked => continue,
                ModelStatusFilter::Available | ModelStatusFilter::Blocked => {}
            }
            if lock.status != ModelLockState::Available {
                issues.push(format!(
                    "{}: model lock is {}",
                    lock.key,
                    lock.status.as_str()
                ));
            }
            models.push(NodeModelCard {
                workflow_id: workflow.id.clone(),
                workflow_name: workflow.name.clone(),
                category: workflow.category.clone(),
                requirement: requirement.clone(),
                bindings: model_bindings(workflow, &requirement.id),
                lock,
                sync_command: model_sync_command(&workflow.id),
                verify_command: model_verify_command(&workflow.id),
            });
        }
    }
    let total = models.len();
    let blocked_count = issues.len();
    ModelCatalog {
        total,
        available_count: total.saturating_sub(blocked_count),
        blocked_count,
        issues,
        models,
    }
}

fn model_sync_command(workflow_id: &str) -> Vec<String> {
    vec![
        "lfw".to_owned(),
        "sync".to_owned(),
        workflow_id.to_owned(),
        "--auto-model".to_owned(),
        "--apply".to_owned(),
    ]
}

fn model_verify_command(workflow_id: &str) -> Vec<String> {
    vec![
        "lfw".to_owned(),
        "sync".to_owned(),
        workflow_id.to_owned(),
        "--locked".to_owned(),
        "--apply".to_owned(),
    ]
}

fn model_bindings(workflow: &WorkflowSpec, requirement_id: &str) -> Vec<NodeModelBinding> {
    let mut bindings = Vec::new();
    for port in &workflow.inputs {
        if port.model_requirement.as_deref() == Some(requirement_id) {
            bindings.push(NodeModelBinding {
                direction: PortDirection::Input,
                port: port.name.clone(),
            });
        }
    }
    for port in &workflow.outputs {
        if port.model_requirement.as_deref() == Some(requirement_id) {
            bindings.push(NodeModelBinding {
                direction: PortDirection::Output,
                port: port.name.clone(),
            });
        }
    }
    bindings
}
