use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct LocalLoopReport {
    pub valid: bool,
    pub project_root: PathBuf,
    pub project_config_path: PathBuf,
    pub project_config_present: bool,
    pub project_config_valid: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_config_error: Option<String>,
    pub project_config_template_command: Vec<String>,
    pub project_config_write_command: Vec<String>,
    pub project_submodule_update_command: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replay_run_id: Option<String>,
    pub issues: Vec<String>,
    pub warning_messages: Vec<String>,
    pub passed: usize,
    pub warnings: usize,
    pub failed: usize,
    pub checks: Vec<LocalLoopCheck>,
    pub next_commands: Vec<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct LocalLoopCheck {
    pub id: &'static str,
    pub status: LocalLoopStatus,
    pub message: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub details: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<usize>,
}

impl LocalLoopCheck {
    pub(super) fn passed(id: &'static str, message: impl Into<String>) -> Self {
        Self {
            id,
            status: LocalLoopStatus::Passed,
            message: message.into(),
            details: Vec::new(),
            path: None,
            count: None,
        }
    }

    pub(super) fn warning(id: &'static str, message: impl Into<String>) -> Self {
        Self {
            id,
            status: LocalLoopStatus::Warning,
            message: message.into(),
            details: Vec::new(),
            path: None,
            count: None,
        }
    }

    pub(super) fn failed(id: &'static str, message: impl Into<String>) -> Self {
        Self {
            id,
            status: LocalLoopStatus::Failed,
            message: message.into(),
            details: Vec::new(),
            path: None,
            count: None,
        }
    }

    pub(super) fn path(mut self, path: impl Into<PathBuf>) -> Self {
        self.path = Some(path.into());
        self
    }

    pub(super) fn count(mut self, count: usize) -> Self {
        self.count = Some(count);
        self
    }

    pub(super) fn details(mut self, details: Vec<String>) -> Self {
        self.details = details;
        self
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct WorkflowPublishCheck {
    pub workflow_id: String,
    pub package: String,
    pub version: String,
    pub workspace: String,
    pub manifest: PathBuf,
    pub publishable: bool,
    pub issues: Vec<String>,
    pub command: Vec<String>,
    pub internal_dependencies: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct WorkflowPublishCatalog {
    pub publishable: bool,
    pub project_root: PathBuf,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_filter_matched: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matched_project_workspace: Option<String>,
    pub total: usize,
    pub publishable_count: usize,
    pub blocked_count: usize,
    pub commands: Vec<Vec<String>>,
    pub checks: Vec<WorkflowPublishCheck>,
    pub issues: Vec<String>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct WorkflowPublishOptions {
    pub project: Option<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ProjectWorkspaceCatalog {
    pub valid: bool,
    pub project_root: PathBuf,
    pub projects_dir: PathBuf,
    pub project_config_path: PathBuf,
    pub project_config_present: bool,
    pub project_config_valid: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_config_error: Option<String>,
    pub project_config_template_command: Vec<String>,
    pub project_config_write_command: Vec<String>,
    pub project_submodule_update_command: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_filter: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_filter_matched: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matched_project_workspace: Option<String>,
    pub dirty_filter: bool,
    pub expected_count: usize,
    pub optional_count: usize,
    pub present_count: usize,
    pub linked_count: usize,
    pub missing_count: usize,
    pub directory_count: usize,
    pub symlink_count: usize,
    pub submodule_count: usize,
    pub not_symlink_count: usize,
    pub broken_count: usize,
    pub workflow_crate_count: usize,
    pub known_workspace_names: Vec<String>,
    pub known_workspace_aliases: BTreeMap<String, String>,
    pub known_project_workspaces: Vec<String>,
    pub known_project_aliases: BTreeMap<String, String>,
    pub known_optional_workspace_names: Vec<String>,
    pub optional_workspace_names: Vec<String>,
    pub default_workflow_sources: Vec<String>,
    pub issues: Vec<String>,
    pub workspaces: Vec<ProjectWorkspaceSummary>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ProjectWorkspaceSummary {
    pub name: String,
    pub label: String,
    pub aliases: Vec<String>,
    pub expected: bool,
    pub optional: bool,
    pub path: PathBuf,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_path: Option<PathBuf>,
    pub exists: bool,
    pub is_symlink: bool,
    pub broken: bool,
    pub workflow_crate_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_dirty: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_changed_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_changed_paths: Option<Vec<PathBuf>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_upstream: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_remote_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_head: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_gitlink_head: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_gitlink_changed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_status_command: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_stage_command: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_commit_command: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_push_command: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_gitlink_stage_command: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_status_error: Option<String>,
    pub issues: Vec<String>,
}

#[derive(Debug, Default, Clone, Eq, PartialEq)]
pub struct ProjectWorkspaceOptions {
    pub dirty_only: bool,
    pub project: Option<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct LoopChangesReport {
    pub valid: bool,
    pub project_root: PathBuf,
    pub issues: Vec<String>,
    pub blockers: Vec<String>,
    pub warning_messages: Vec<String>,
    pub passed: usize,
    pub warnings: usize,
    pub failed: usize,
    pub changed_workflows: Vec<WorkflowChangeSummary>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct WorkflowChangeSummary {
    pub workflow_key: String,
    pub status: LoopChangeStatus,
    pub message: String,
    pub workflow_changed: bool,
    pub skill_changed: bool,
    pub patch_changed: bool,
    pub workflow_paths: Vec<PathBuf>,
    pub skill_paths: Vec<PathBuf>,
    pub patch_paths: Vec<PathBuf>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalLoopStatus {
    Passed,
    Warning,
    Failed,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LoopChangeStatus {
    Passed,
    Warning,
    Failed,
}

#[derive(Debug, Default)]
pub(super) struct WorkflowChangeAccumulator {
    pub(super) workflow_paths: Vec<PathBuf>,
    pub(super) skill_paths: Vec<PathBuf>,
    pub(super) patch_paths: Vec<PathBuf>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(super) enum WorkflowChangeKind {
    Workflow,
    Skill,
    Patch,
}
