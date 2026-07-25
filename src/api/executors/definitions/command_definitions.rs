use super::super::{ExecutorAvailability, ExecutorDefinition};
use super::matchers::matches_command;
use crate::api::executors::COMMAND_RUNNER_ENV;
use crate::api::plan::{COMMAND_ENGINE, COMMAND_RUN_CAPABILITY, DataPolicy, ExecutionRecipe};

pub(super) static COMMAND_EXECUTORS: [ExecutorDefinition; 1] = [ExecutorDefinition {
    id: COMMAND_ENGINE,
    kind: "external",
    capabilities: &[COMMAND_RUN_CAPABILITY],
    features: &[],
    env: Some(COMMAND_RUNNER_ENV),
    command_env: Some(COMMAND_RUNNER_ENV),
    visible: true,
    availability: ExecutorAvailability::CommandRunner,
    recipe: ExecutionRecipe::ExternalCommand,
    data_policy: DataPolicy::ArtifactHandles,
    atoms: &[
        ("lightflow.atom.command.start", COMMAND_RUN_CAPABILITY),
        (
            "lightflow.atom.command.exchange_json",
            COMMAND_RUN_CAPABILITY,
        ),
        (
            "lightflow.atom.command.collect_artifacts",
            COMMAND_RUN_CAPABILITY,
        ),
    ],
    plans_models: false,
    matcher: matches_command,
}];
