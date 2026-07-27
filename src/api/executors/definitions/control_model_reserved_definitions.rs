use super::super::{ExecutorAvailability, ExecutorDefinition};
use super::matchers::matches_never;
use crate::api::plan::{DataPolicy, ExecutionRecipe};

pub(super) static CONTROL_MODEL_RESERVED_EXECUTORS: [ExecutorDefinition; 2] = [
    ExecutorDefinition {
        id: "lightflow.onnx.executor.v1",
        kind: "reserved",
        capabilities: &["lightflow.onnx"],
        features: &[],
        env: None,
        command_env: None,
        visible: true,
        availability: ExecutorAvailability::Unavailable,
        recipe: ExecutionRecipe::Passthrough,
        data_policy: DataPolicy::JsonValues,
        atoms: &[],
        plans_models: false,
        matcher: matches_never,
    },
    ExecutorDefinition {
        id: "lightflow.candle.executor.v1",
        kind: "reserved",
        capabilities: &["lightflow.candle"],
        features: &["gguf"],
        env: None,
        command_env: None,
        visible: true,
        availability: ExecutorAvailability::Unavailable,
        recipe: ExecutionRecipe::Passthrough,
        data_policy: DataPolicy::JsonValues,
        atoms: &[],
        plans_models: true,
        matcher: matches_never,
    },
];
