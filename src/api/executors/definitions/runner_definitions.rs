use super::super::{ExecutorAvailability, ExecutorDefinition};
use super::matchers::matches_runner;
use crate::api::plan::{DataPolicy, ExecutionRecipe, RUNNER_CAPABILITY, RUNNER_ENGINE};

pub(super) static RUNNER_EXECUTORS: [ExecutorDefinition; 1] = [ExecutorDefinition {
    id: RUNNER_ENGINE,
    kind: "runner",
    capabilities: &[RUNNER_CAPABILITY],
    features: &[],
    env: None,
    command_env: None,
    visible: true,
    availability: ExecutorAvailability::Always,
    recipe: ExecutionRecipe::Runner,
    data_policy: DataPolicy::ArtifactHandles,
    atoms: &[
        ("lightflow.atom.runner.start", RUNNER_CAPABILITY),
        ("lightflow.atom.runner.exchange_json", RUNNER_CAPABILITY),
        ("lightflow.atom.runner.validate_response", RUNNER_CAPABILITY),
    ],
    plans_models: false,
    matcher: matches_runner,
}];
