use super::util::{XdgUserDirectory, lightflow_xdg_user_dir};
use crate::workflow::WorkflowSpec;
use std::path::{Path, PathBuf};

/// Media output family used to resolve the matching XDG user directory.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum MediaKind {
    Image,
    Video,
    Music,
}

impl MediaKind {
    fn xdg_user_directory(self) -> XdgUserDirectory {
        match self {
            Self::Image => XdgUserDirectory::Pictures,
            Self::Video => XdgUserDirectory::Videos,
            Self::Music => XdgUserDirectory::Music,
        }
    }
}

/// Resolves LightFlow media artifact paths through XDG user directories.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct MediaPathProvider {
    root: PathBuf,
}

impl MediaPathProvider {
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    #[must_use]
    pub fn media_dir(&self, kind: MediaKind) -> PathBuf {
        lightflow_xdg_user_dir(&self.root, kind.xdg_user_directory())
    }

    #[must_use]
    pub fn workflow_dir(&self, kind: MediaKind, workflow: &WorkflowSpec) -> PathBuf {
        self.media_dir(kind).join(workflow_path_segment(workflow))
    }

    #[must_use]
    pub fn explicit_output_path(&self, path: impl AsRef<str>) -> PathBuf {
        expand_tilde(PathBuf::from(path.as_ref()))
    }

    #[must_use]
    pub fn default_output_path(
        &self,
        kind: MediaKind,
        workflow: &WorkflowSpec,
        file_name: impl AsRef<Path>,
    ) -> PathBuf {
        self.workflow_dir(kind, workflow).join(file_name)
    }

    #[must_use]
    pub fn output_path_or_default(
        &self,
        explicit_path: Option<&str>,
        kind: MediaKind,
        workflow: &WorkflowSpec,
        file_name: impl AsRef<Path>,
    ) -> PathBuf {
        explicit_path
            .map(|path| self.explicit_output_path(path))
            .unwrap_or_else(|| self.default_output_path(kind, workflow, file_name))
    }
}

#[must_use]
pub fn workflow_path_segment(workflow: &WorkflowSpec) -> String {
    workflow.id.replace('.', "_")
}

#[must_use]
pub fn expand_tilde(path: PathBuf) -> PathBuf {
    let Some(path_text) = path.to_str() else {
        return path;
    };
    if path_text == "~" {
        return std::env::var_os("HOME").map(PathBuf::from).unwrap_or(path);
    }
    if let Some(rest) = path_text.strip_prefix("~/")
        && let Some(home) = std::env::var_os("HOME")
    {
        return PathBuf::from(home).join(rest);
    }
    path
}
