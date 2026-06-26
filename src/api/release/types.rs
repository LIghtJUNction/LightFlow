use serde::Serialize;
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ReleaseCheckOptions {
    pub apply: bool,
    pub workflow_id: String,
    pub project: Option<String>,
    pub profile: CheckProfile,
}

impl Default for ReleaseCheckOptions {
    fn default() -> Self {
        Self {
            apply: false,
            workflow_id: "lightflow.text_plan".to_owned(),
            project: None,
            profile: CheckProfile::Release,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ReleaseCheckReport {
    pub profile: CheckProfile,
    pub dry_run: bool,
    pub valid: bool,
    pub project_root: PathBuf,
    pub workflow_id: String,
    pub project_config_path: PathBuf,
    pub project_config_present: bool,
    pub project_config_valid: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_config_error: Option<String>,
    pub project_config_template_command: Vec<String>,
    pub project_config_write_command: Vec<String>,
    pub project_submodule_update_command: Vec<String>,
    pub default_workflow_sources: Vec<String>,
    pub known_optional_workspace_names: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_filter_matched: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matched_project_workspace: Option<String>,
    pub known_project_workspaces: Vec<String>,
    pub known_project_aliases: BTreeMap<String, String>,
    pub issues: Vec<String>,
    pub warnings: Vec<String>,
    pub passed: usize,
    pub warning_count: usize,
    pub failed: usize,
    pub planned: usize,
    pub skipped: usize,
    pub checks: Vec<ReleaseCheck>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckProfile {
    Development,
    Release,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReleaseCheck {
    pub id: &'static str,
    pub kind: ReleaseCheckKind,
    pub status: ReleaseCheckStatus,
    pub message: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub details: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stdout_tail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stderr_tail: Option<String>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReleaseCheckKind {
    Command,
    Artifact,
    Document,
    Review,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReleaseCheckStatus {
    Planned,
    Passed,
    Warning,
    Failed,
    Skipped,
}
