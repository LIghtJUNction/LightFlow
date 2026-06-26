use super::plan::{
    DataPolicy, ExecutionRecipe, LLM_MOCK_ENGINE, PREVIEW_EDIT_ENGINE, PREVIEW_ENGINE,
    PREVIEW_INPAINT_ENGINE,
};
use crate::workflow::WorkflowSpec;

mod definitions;
use definitions::executor_definitions;
use serde::Serialize;
use std::env;

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
            LLM_MOCK_ENGINE => "mock",
            _ => self.kind,
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum ExecutorAvailability {
    Always,
    Unavailable,
    EnvPresent(&'static str),
    Feature(bool),
}

impl ExecutorAvailability {
    fn available(self) -> bool {
        match self {
            Self::Always => true,
            Self::Unavailable => false,
            Self::EnvPresent(name) => env::var(name).is_ok(),
            Self::Feature(enabled) => enabled,
        }
    }

    fn reason(self, features: &[&'static str]) -> String {
        match self {
            Self::Always => "available in this build".to_owned(),
            Self::Unavailable => {
                "reserved executor contract; not runnable in this build".to_owned()
            }
            Self::EnvPresent(name) => {
                if env::var(name).is_ok() {
                    format!("{name} is set")
                } else {
                    format!("set {name} to enable this executor")
                }
            }
            Self::Feature(true) => {
                let feature = features.first().copied().unwrap_or("required");
                format!("feature {feature} is enabled")
            }
            Self::Feature(false) => {
                let feature = features.first().copied().unwrap_or("required");
                format!("build with --features {feature} to enable this executor")
            }
        }
    }
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
