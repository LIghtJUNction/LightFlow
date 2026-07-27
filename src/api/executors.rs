use super::plan::{
    DataPolicy, ExecutionRecipe, PREVIEW_EDIT_ENGINE, PREVIEW_ENGINE, PREVIEW_INPAINT_ENGINE,
};
use crate::workflow::WorkflowSpec;

mod definitions;
use definitions::executor_definitions;
use serde::Serialize;
use std::env;
use std::ffi::OsString;
use std::path::PathBuf;

pub(super) const COMMAND_RUNNER_ENV: &str = "LIGHTFLOW_COMMAND_RUNNER";

#[derive(Debug, Clone, Serialize)]
pub struct ExecutorInfo {
    pub id: &'static str,
    pub kind: &'static str,
    pub status: &'static str,
    pub status_reason: String,
    pub capabilities: Vec<&'static str>,
    pub available: bool,
    pub data_policy: &'static str,
    pub plans_models: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub features: Vec<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExecutorCatalog {
    pub executors: Vec<ExecutorInfo>,
}

pub(super) struct ExecutorDefinition {
    pub(super) id: &'static str,
    pub(super) kind: &'static str,
    pub(super) capabilities: &'static [&'static str],
    features: &'static [&'static str],
    env: Option<&'static str>,
    command_env: Option<&'static str>,
    visible: bool,
    availability: ExecutorAvailability,
    pub(super) recipe: ExecutionRecipe,
    pub(super) data_policy: DataPolicy,
    pub(super) atoms: &'static [(&'static str, &'static str)],
    pub(super) plans_models: bool,
    matcher: fn(&WorkflowSpec) -> bool,
}

impl ExecutorDefinition {
    pub(super) fn info(&self) -> ExecutorInfo {
        ExecutorInfo {
            id: self.id,
            kind: self.kind,
            status: self.status(),
            status_reason: self.availability.reason(self.features),
            capabilities: self.capabilities.to_vec(),
            available: self.availability.available(),
            data_policy: data_policy_name(self.data_policy),
            plans_models: self.plans_models,
            features: self.features.to_vec(),
            env: self.env,
            command: self.command_env.and_then(|name| env::var(name).ok()),
        }
    }

    fn status(&self) -> &'static str {
        match self.id {
            PREVIEW_ENGINE | PREVIEW_EDIT_ENGINE | PREVIEW_INPAINT_ENGINE => "preview",
            _ => self.kind,
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum ExecutorAvailability {
    Always,
    EndpointCheckedAtRun,
    Unavailable,
    CommandRunner,
}

impl ExecutorAvailability {
    fn available(self) -> bool {
        match self {
            Self::Always | Self::EndpointCheckedAtRun => true,
            Self::Unavailable => false,
            Self::CommandRunner => validated_command_runner_path().is_ok(),
        }
    }

    fn reason(self, _features: &[&'static str]) -> String {
        match self {
            Self::Always => "available in this build".to_owned(),
            Self::EndpointCheckedAtRun => "executor available; endpoint checked at run".to_owned(),
            Self::Unavailable => {
                "reserved executor contract; not runnable in this build".to_owned()
            }
            Self::CommandRunner => validated_command_runner_path()
                .map(|path| runner_available_reason(COMMAND_RUNNER_ENV, &path))
                .unwrap_or_else(|reason| reason),
        }
    }
}

pub(super) fn validated_command_runner_path() -> Result<PathBuf, String> {
    validate_executable_path(COMMAND_RUNNER_ENV, env::var_os(COMMAND_RUNNER_ENV))
}

fn runner_available_reason(environment: &str, path: &std::path::Path) -> String {
    format!("{environment} points to executable file {}", path.display())
}

fn validate_executable_path(environment: &str, value: Option<OsString>) -> Result<PathBuf, String> {
    let Some(value) = value else {
        return Err(format!("set {environment} to enable this executor"));
    };
    if value.is_empty() {
        return Err(format!("{environment} is empty"));
    }

    let path = PathBuf::from(value);
    let metadata = path
        .metadata()
        .map_err(|_| format!("{environment} does not point to a file: {}", path.display()))?;
    if !metadata.is_file() {
        return Err(format!(
            "{environment} does not point to a file: {}",
            path.display()
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o111 == 0 {
            return Err(format!(
                "{environment} is not executable: {}",
                path.display()
            ));
        }
    }
    path.canonicalize().map_err(|error| {
        format!(
            "failed to canonicalize executable from {environment}: {}: {error}",
            path.display()
        )
    })
}

pub(super) const fn data_policy_name(data_policy: DataPolicy) -> &'static str {
    match data_policy {
        DataPolicy::JsonValues => "json_values",
        DataPolicy::ArtifactHandles => "artifact_handles",
        DataPolicy::DeviceResidentPreferred => "device_resident_preferred",
    }
}

pub fn executor_registry() -> Vec<ExecutorInfo> {
    executor_definitions()
        .into_iter()
        .filter(|executor| executor.visible)
        .map(ExecutorDefinition::info)
        .collect()
}

pub(super) fn select_leaf_executor(workflow: &WorkflowSpec) -> Option<&'static ExecutorDefinition> {
    executor_definitions()
        .into_iter()
        .find(|executor| (executor.matcher)(workflow))
}

pub(super) fn executor_by_id(id: &str) -> Option<&'static ExecutorDefinition> {
    executor_definitions()
        .into_iter()
        .find(|executor| executor.id == id)
}

#[cfg(test)]
mod tests {
    use super::{executor_registry, validate_executable_path};
    use std::ffi::OsString;

    #[test]
    fn removed_builtin_std_engines_remain_reserved_and_unavailable() {
        let expected = [
            "builtin.image.invert.v1",
            "builtin.image.load.v1",
            "builtin.image.save.v1",
            "builtin.image.resize.v1",
            "builtin.image.crop.v1",
            "builtin.image.upscale.v1",
            "builtin.mask.compose.v1",
            "builtin.json.extract.v1",
            "builtin.control.if.v1",
            "builtin.control.switch.v1",
            "builtin.control.merge.v1",
            "builtin.control.split.v1",
            "builtin.model.select.v1",
            "builtin.model.lock.check.v1",
            "builtin.llm.classify.v1",
            "builtin.llm.structured_output.v1",
            "builtin.llm.mock.v1",
            "builtin.text.concat.v1",
            "builtin.text.template.v1",
            "builtin.text.regex.v1",
        ];
        let registry = executor_registry();

        for id in expected {
            let executor = registry
                .iter()
                .find(|executor| executor.id == id)
                .unwrap_or_else(|| panic!("missing compatibility executor {id}"));
            assert_eq!(executor.kind, "reserved", "{id}");
            assert!(!executor.available, "{id}");
        }
    }

    #[test]
    fn executable_path_reports_missing_and_non_file_values() {
        assert_eq!(
            validate_executable_path("LIGHTFLOW_TEST_RUNNER", None).unwrap_err(),
            "set LIGHTFLOW_TEST_RUNNER to enable this executor"
        );
        assert_eq!(
            validate_executable_path("LIGHTFLOW_TEST_RUNNER", Some(OsString::new())).unwrap_err(),
            "LIGHTFLOW_TEST_RUNNER is empty"
        );

        let directory = tempfile::tempdir().expect("tempdir");
        assert!(
            validate_executable_path(
                "LIGHTFLOW_TEST_RUNNER",
                Some(directory.path().as_os_str().to_owned())
            )
            .unwrap_err()
            .contains("does not point to a file")
        );
    }

    #[cfg(unix)]
    #[test]
    fn executable_path_requires_unix_execute_permission() {
        use std::fs;
        use std::os::unix::fs::PermissionsExt;

        let directory = tempfile::tempdir().expect("tempdir");
        let runner = directory.path().join("runner");
        fs::write(&runner, "#!/bin/sh\nexit 0\n").expect("write runner");

        let error =
            validate_executable_path("LIGHTFLOW_TEST_RUNNER", Some(runner.as_os_str().to_owned()))
                .unwrap_err();
        assert!(error.contains("is not executable"));

        fs::set_permissions(&runner, fs::Permissions::from_mode(0o700))
            .expect("make runner executable");
        assert_eq!(
            validate_executable_path("LIGHTFLOW_TEST_RUNNER", Some(runner.as_os_str().to_owned()))
                .expect("executable runner"),
            runner
        );
    }
}
