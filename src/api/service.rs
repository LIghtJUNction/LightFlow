use super::project_config;
use std::path::{Path, PathBuf};

/// Backend service state independent of any web framework.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ApiService {
    pub(super) repo_root: PathBuf,
    pub(super) workflow_paths: Vec<PathBuf>,
}

impl ApiService {
    /// Create a service rooted at a LightFlow repository.
    #[must_use]
    pub fn new(repo_root: impl Into<PathBuf>) -> Self {
        Self {
            repo_root: repo_root.into(),
            workflow_paths: Vec::new(),
        }
    }

    /// Add workflow search paths. Each path can point at a workflow collection,
    /// a LightFlow project root, or one workflow crate.
    #[must_use]
    pub fn with_workflow_paths(mut self, workflow_paths: Vec<PathBuf>) -> Self {
        self.workflow_paths = workflow_paths;
        self
    }

    /// Repository root used for project file discovery.
    #[must_use]
    pub fn repo_root(&self) -> &Path {
        &self.repo_root
    }

    /// Path to the project workspace configuration file.
    #[must_use]
    pub fn project_workspace_config_path(&self) -> PathBuf {
        project_config::project_workspace_config_path(self.repo_root())
    }

    /// Compatibility defaults used when the project workspace config is absent or being repaired.
    #[must_use]
    pub fn default_project_config_values(&self) -> (Vec<String>, Vec<String>, Vec<String>) {
        (
            project_config::default_expected_project_workspace_names(),
            project_config::default_optional_project_workspace_names(),
            project_config::default_project_workflow_source_names(),
        )
    }

    /// Command that initializes the configured project submodules.
    pub fn project_submodule_update_command<'a>(
        &self,
        names: impl IntoIterator<Item = &'a str>,
    ) -> Vec<String> {
        project_config::project_submodule_update_command(names)
    }

    /// Command that prints the effective project workspace config template.
    #[must_use]
    pub fn project_config_template_command(&self) -> Vec<String> {
        project_config::project_config_template_command()
    }

    /// Command that writes the project workspace config template.
    #[must_use]
    pub fn project_config_write_command(&self) -> Vec<String> {
        project_config::project_config_write_command()
    }
}
