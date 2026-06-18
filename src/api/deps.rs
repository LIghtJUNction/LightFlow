use super::util::referenced_workflow_ids;
use crate::workflow::{
    ResolvedWorkflowDependency, WorkflowDependencyReport, WorkflowSpec, WorkflowVersionMismatch,
};
use semver::Version;
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn dependency_report(
    workflow_id: &str,
    workflows: &BTreeMap<String, WorkflowSpec>,
) -> WorkflowDependencyReport {
    let mut collector = DependencyCollector {
        workflows,
        states: BTreeMap::new(),
        stack: Vec::new(),
        workflow_ids: BTreeSet::new(),
        resolved: BTreeMap::new(),
        workflow_order: Vec::new(),
        missing_workflows: BTreeSet::new(),
        version_mismatches: Vec::new(),
        cycles: Vec::new(),
    };
    collector.visit_workflow(workflow_id);
    let missing_workflows = collector.missing_workflows.into_iter().collect::<Vec<_>>();
    let complete = missing_workflows.is_empty()
        && collector.version_mismatches.is_empty()
        && collector.cycles.is_empty();
    WorkflowDependencyReport {
        workflow_id: workflow_id.to_owned(),
        complete,
        workflows: collector.workflow_ids.into_iter().collect(),
        resolved: collector.resolved.into_values().collect(),
        workflow_order: collector.workflow_order,
        missing_workflows,
        version_mismatches: collector.version_mismatches,
        cycles: collector.cycles,
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum VisitState {
    Visiting,
    Visited,
}

struct DependencyCollector<'a> {
    workflows: &'a BTreeMap<String, WorkflowSpec>,
    states: BTreeMap<String, VisitState>,
    stack: Vec<String>,
    workflow_ids: BTreeSet<String>,
    resolved: BTreeMap<String, ResolvedWorkflowDependency>,
    workflow_order: Vec<String>,
    missing_workflows: BTreeSet<String>,
    version_mismatches: Vec<WorkflowVersionMismatch>,
    cycles: Vec<Vec<String>>,
}

impl DependencyCollector<'_> {
    fn visit_workflow(&mut self, workflow_id: &str) {
        match self.states.get(workflow_id).copied() {
            Some(VisitState::Visited) => return,
            Some(VisitState::Visiting) => {
                if let Some(index) = self.stack.iter().position(|id| id == workflow_id) {
                    let mut cycle = self.stack[index..].to_vec();
                    cycle.push(workflow_id.to_owned());
                    if !self.cycles.contains(&cycle) {
                        self.cycles.push(cycle);
                    }
                }
                return;
            }
            None => {}
        }

        let Some(workflow) = self.workflows.get(workflow_id) else {
            self.missing_workflows.insert(workflow_id.to_owned());
            return;
        };

        self.workflow_ids.insert(workflow_id.to_owned());
        self.resolved.insert(
            workflow_id.to_owned(),
            ResolvedWorkflowDependency {
                workflow_id: workflow_id.to_owned(),
                version: workflow.version.clone(),
            },
        );
        self.states
            .insert(workflow_id.to_owned(), VisitState::Visiting);
        self.stack.push(workflow_id.to_owned());

        for dependency in &workflow.dependencies {
            self.record_workflow_requirement(
                &dependency.workflow_id,
                dependency.version.as_deref(),
                workflow_id,
            );
            self.visit_workflow(&dependency.workflow_id);
        }

        for node in &workflow.nodes {
            for referenced in referenced_workflow_ids(node) {
                self.record_workflow_requirement(referenced, None, workflow_id);
                self.visit_workflow(referenced);
            }
        }

        self.stack.pop();
        self.states
            .insert(workflow_id.to_owned(), VisitState::Visited);
        if !self
            .workflow_order
            .iter()
            .any(|ordered| ordered == workflow_id)
        {
            self.workflow_order.push(workflow_id.to_owned());
        }
    }

    fn record_workflow_requirement(
        &mut self,
        workflow_id: &str,
        required: Option<&str>,
        required_by: &str,
    ) {
        let Some(workflow) = self.workflows.get(workflow_id) else {
            self.missing_workflows.insert(workflow_id.to_owned());
            return;
        };
        if let Some(required) = required
            && !version_satisfies(&workflow.version, required)
        {
            self.version_mismatches.push(WorkflowVersionMismatch {
                workflow_id: workflow_id.to_owned(),
                required: required.to_owned(),
                found: workflow.version.clone(),
                required_by: required_by.to_owned(),
            });
        }
    }
}

fn version_satisfies(found: &str, required: &str) -> bool {
    if required == "*" {
        return true;
    }
    let Ok(found) = Version::parse(found) else {
        return false;
    };
    let Ok(required) = Version::parse(required) else {
        return false;
    };
    found == required
}

pub(super) fn is_supported_version_requirement(required: &str) -> bool {
    required == "*" || Version::parse(required).is_ok()
}
