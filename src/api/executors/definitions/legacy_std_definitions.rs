use super::super::{ExecutorAvailability, ExecutorDefinition};
use super::matchers::matches_never;
use crate::api::plan::{DataPolicy, ExecutionRecipe};

macro_rules! unavailable {
    ($id:literal, $capability:literal, $policy:expr) => {
        ExecutorDefinition {
            id: $id,
            kind: "reserved",
            capabilities: &[$capability],
            features: &[],
            env: None,
            command_env: None,
            visible: true,
            availability: ExecutorAvailability::Unavailable,
            recipe: ExecutionRecipe::Unavailable,
            data_policy: $policy,
            atoms: &[],
            plans_models: false,
            matcher: matches_never,
        }
    };
}

pub(super) static LEGACY_STD_EXECUTORS: [ExecutorDefinition; 20] = [
    unavailable!(
        "builtin.image.invert.v1",
        "lightflow.image.invert",
        DataPolicy::ArtifactHandles
    ),
    unavailable!(
        "builtin.image.load.v1",
        "lightflow.image.load",
        DataPolicy::ArtifactHandles
    ),
    unavailable!(
        "builtin.image.save.v1",
        "lightflow.image.save",
        DataPolicy::ArtifactHandles
    ),
    unavailable!(
        "builtin.image.resize.v1",
        "lightflow.image.resize",
        DataPolicy::ArtifactHandles
    ),
    unavailable!(
        "builtin.image.crop.v1",
        "lightflow.image.crop",
        DataPolicy::ArtifactHandles
    ),
    unavailable!(
        "builtin.image.upscale.v1",
        "lightflow.image.upscale",
        DataPolicy::ArtifactHandles
    ),
    unavailable!(
        "builtin.mask.compose.v1",
        "lightflow.mask.compose",
        DataPolicy::ArtifactHandles
    ),
    unavailable!(
        "builtin.json.extract.v1",
        "lightflow.json.extract",
        DataPolicy::JsonValues
    ),
    unavailable!(
        "builtin.control.if.v1",
        "lightflow.control.if",
        DataPolicy::JsonValues
    ),
    unavailable!(
        "builtin.control.switch.v1",
        "lightflow.control.switch",
        DataPolicy::JsonValues
    ),
    unavailable!(
        "builtin.control.merge.v1",
        "lightflow.control.merge",
        DataPolicy::JsonValues
    ),
    unavailable!(
        "builtin.control.split.v1",
        "lightflow.control.split",
        DataPolicy::JsonValues
    ),
    unavailable!(
        "builtin.model.select.v1",
        "lightflow.model.select",
        DataPolicy::JsonValues
    ),
    unavailable!(
        "builtin.model.lock.check.v1",
        "lightflow.model.lock.check",
        DataPolicy::JsonValues
    ),
    unavailable!(
        "builtin.llm.classify.v1",
        "lightflow.llm.classify",
        DataPolicy::JsonValues
    ),
    unavailable!(
        "builtin.llm.structured_output.v1",
        "lightflow.llm.structured_output",
        DataPolicy::JsonValues
    ),
    unavailable!(
        "builtin.llm.mock.v1",
        "lightflow.llm.generate",
        DataPolicy::JsonValues
    ),
    unavailable!(
        "builtin.text.concat.v1",
        "lightflow.text.concat",
        DataPolicy::JsonValues
    ),
    unavailable!(
        "builtin.text.template.v1",
        "lightflow.text.template",
        DataPolicy::JsonValues
    ),
    unavailable!(
        "builtin.text.regex.v1",
        "lightflow.text.regex",
        DataPolicy::JsonValues
    ),
];
