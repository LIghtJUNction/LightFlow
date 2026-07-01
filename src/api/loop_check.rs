use super::{
    ApiError, ApiResult, ApiService,
    project_config::{
        default_expected_project_workspace_names, default_optional_project_workspace_names,
        default_project_workflow_source_names, default_project_workflow_sources,
        expected_project_workspace_names, optional_project_workspace_names,
        project_config_template_command, project_config_write_command,
        project_submodule_update_command, project_workspace_config_path,
    },
};

mod agent_skill_checks;
mod check_messages;
mod git_worktree;
mod local_readiness_checks;
mod loop_changes;
mod loop_report;
mod project_workspace_catalog;
mod project_workspace_inspection;
mod project_workspaces;
mod publish_readiness;
mod repository_checks;
mod selected_publish;
mod selected_workflow;
mod types;
mod workflow_crates;
mod workflow_publish_catalog;

use agent_skill_checks::push_agent_skill_check;
use git_worktree::{
    git_changed_paths, git_current_branch, git_current_upstream, git_full_head,
    git_origin_remote_url, git_short_head, parent_gitlink_full_head, short_commit,
};
use local_readiness_checks::{
    push_executor_check, push_model_readiness_check, push_patch_registry_check,
    push_run_history_check, push_workflow_discovery_check,
};
use loop_changes::loop_changes_across_project_set;
use loop_report::{latest_completed_run_id, loop_check_messages, next_commands};
use project_workspace_inspection::discover_present_project_workspaces;
pub(super) use project_workspaces::project_git_status_issues;
use project_workspaces::{
    filter_dirty_project_workspaces, filter_project_workspaces, matched_project_workspace,
    project_workspace_filter_alias_choices, project_workspace_filter_choices, project_workspaces,
};
use repository_checks::{
    push_document_checks, push_project_set_check, push_source_change_safety_check,
};
use selected_publish::selected_local_publish_plan_count;
use selected_workflow::{push_selected_replay_required_check, push_selected_workflow_checks};
pub use types::{
    LocalLoopCheck, LocalLoopReport, LocalLoopStatus, LoopChangeStatus, LoopChangesReport,
    ProjectWorkspaceCatalog, ProjectWorkspaceOptions, ProjectWorkspaceSummary,
    WorkflowChangeSummary, WorkflowPublishCatalog, WorkflowPublishCheck, WorkflowPublishOptions,
};
use types::{WorkflowChangeAccumulator, WorkflowChangeKind};
use workflow_publish_catalog::{
    push_publish_check, workflow_publish_check_for_service, workflow_publish_checks_with_options,
};

impl ApiService {
    pub fn local_loop_check(&self, workflow_id: Option<&str>) -> ApiResult<LocalLoopReport> {
        self.local_loop_check_with_options(workflow_id, false)
    }

    pub fn local_loop_check_with_options(
        &self,
        workflow_id: Option<&str>,
        require_selected_replay: bool,
    ) -> ApiResult<LocalLoopReport> {
        let root = self.repo_root();
        let project_catalog = self.project_workspaces()?;
        let mut checks = Vec::new();
        push_document_checks(root, &mut checks)?;
        push_project_set_check(root, &mut checks);
        push_source_change_safety_check(root, &mut checks)?;
        push_workflow_discovery_check(self, &mut checks);
        push_agent_skill_check(root, &mut checks)?;
        push_executor_check(self, &mut checks);
        push_model_readiness_check(self, &mut checks);
        push_run_history_check(self, &mut checks);
        push_patch_registry_check(self, &mut checks);
        push_publish_check(self, &mut checks)?;
        if let Some(workflow_id) = workflow_id {
            push_selected_workflow_checks(self, workflow_id, &mut checks);
            if require_selected_replay {
                push_selected_replay_required_check(workflow_id, &mut checks);
            }
        }

        let passed = checks
            .iter()
            .filter(|check| check.status == LocalLoopStatus::Passed)
            .count();
        let warnings = checks
            .iter()
            .filter(|check| check.status == LocalLoopStatus::Warning)
            .count();
        let failed = checks
            .iter()
            .filter(|check| check.status == LocalLoopStatus::Failed)
            .count();
        let issues = loop_check_messages(&checks, LocalLoopStatus::Failed);
        let warning_messages = loop_check_messages(&checks, LocalLoopStatus::Warning);
        let valid = failed == 0;
        let replay_run_id =
            workflow_id.and_then(|workflow_id| latest_completed_run_id(self, workflow_id));
        let replay_selector = replay_run_id.clone().unwrap_or_else(|| {
            if workflow_id.is_some() {
                "<run_id>".to_owned()
            } else {
                "last".to_owned()
            }
        });
        let command_workflow_id = workflow_id.unwrap_or("<workflow_id>");
        let selected_has_local_publish_graph = workflow_id.is_some_and(|workflow_id| {
            selected_local_publish_plan_count(self, workflow_id).is_some_and(|count| count > 1)
        });
        Ok(LocalLoopReport {
            valid,
            project_root: root.to_path_buf(),
            project_config_path: project_catalog.project_config_path,
            project_config_present: project_catalog.project_config_present,
            project_config_valid: project_catalog.project_config_valid,
            project_config_error: project_catalog.project_config_error,
            project_config_template_command: project_catalog.project_config_template_command,
            project_config_write_command: project_catalog.project_config_write_command,
            project_submodule_update_command: project_catalog.project_submodule_update_command,
            workflow_id: workflow_id.map(ToOwned::to_owned),
            replay_run_id,
            issues,
            warning_messages,
            passed,
            warnings,
            failed,
            checks,
            next_commands: next_commands(
                command_workflow_id,
                &replay_selector,
                workflow_id,
                selected_has_local_publish_graph,
            ),
        })
    }

    pub fn workflow_publish_check(&self, workflow_id: &str) -> ApiResult<WorkflowPublishCheck> {
        workflow_publish_check_for_service(self, workflow_id)
    }

    pub fn workflow_publish_checks(&self) -> ApiResult<WorkflowPublishCatalog> {
        self.workflow_publish_checks_with_options(&WorkflowPublishOptions::default())
    }

    pub fn workflow_publish_checks_with_options(
        &self,
        options: &WorkflowPublishOptions,
    ) -> ApiResult<WorkflowPublishCatalog> {
        workflow_publish_checks_with_options(self, options)
    }

    pub fn workflow_publish_checks_for_project(
        &self,
        project: &str,
    ) -> ApiResult<WorkflowPublishCatalog> {
        self.workflow_publish_checks_with_options(&WorkflowPublishOptions {
            project: Some(project.to_owned()),
        })
    }

    pub fn local_loop_changes(&self) -> ApiResult<LoopChangesReport> {
        loop_changes_across_project_set(self.repo_root())
    }

    pub fn project_workspaces(&self) -> ApiResult<ProjectWorkspaceCatalog> {
        self.project_workspaces_with_options(ProjectWorkspaceOptions::default())
    }

    pub fn project_workspaces_with_options(
        &self,
        options: ProjectWorkspaceOptions,
    ) -> ApiResult<ProjectWorkspaceCatalog> {
        let mut catalog = project_workspaces(self.repo_root())?;
        if let Some(project) = options.project.as_deref() {
            let known = project_workspace_filter_choices(&catalog);
            let aliases = project_workspace_filter_alias_choices(&catalog);
            let matched_project = matched_project_workspace(&catalog, project);
            catalog.project_filter = Some(project.to_owned());
            catalog.project_filter_matched = Some(matched_project.is_some());
            catalog.matched_project_workspace = matched_project;
            if !filter_project_workspaces(&mut catalog, project) {
                catalog.valid = false;
                catalog.issues.push(format!(
                    "project workspace filter matched no workspace: {project}; known workspaces: {known}; known aliases: {aliases}"
                ));
            }
        }
        if options.dirty_only {
            catalog.dirty_filter = true;
            filter_dirty_project_workspaces(&mut catalog);
        }
        Ok(catalog)
    }
}

#[cfg(test)]
mod loop_change_tests;
#[cfg(test)]
mod model_readiness_tests;
#[cfg(test)]
mod run_history_tests;
#[cfg(test)]
mod selected_publish_tests;
#[cfg(test)]
mod test_support;
#[cfg(test)]
mod workflow_publish_catalog_tests;
