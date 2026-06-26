use super::media;
use crate::api::plan;
use crate::workflow::{WorkflowArtifact, WorkflowSpec};
use serde_json::{Map, Value};

pub(super) struct PreviewTransformArtifact<'a> {
    pub(super) workflow: &'a WorkflowSpec,
    pub(super) input_path: &'a std::path::Path,
    pub(super) mask_path: Option<&'a std::path::Path>,
    pub(super) output_path: &'a std::path::Path,
    pub(super) prompt: &'a str,
    pub(super) seed: u64,
    pub(super) engine: &'a str,
    pub(super) capability: &'a str,
    pub(super) dimensions: Option<(u32, u32)>,
    pub(super) inputs: &'a serde_json::Map<String, serde_json::Value>,
}

pub(super) fn build_preview_transform_artifact(
    spec: &PreviewTransformArtifact<'_>,
) -> WorkflowArtifact {
    let mut metadata = Map::new();
    metadata.insert("engine".to_owned(), spec.engine.into());
    metadata.insert("capability".to_owned(), spec.capability.into());
    metadata.insert("prompt".to_owned(), spec.prompt.to_owned().into());
    metadata.insert("seed".to_owned(), spec.seed.into());
    metadata.insert(
        "source_image_path".to_owned(),
        spec.input_path.display().to_string().into(),
    );

    if let Some(mask_path) = spec.mask_path {
        metadata.insert(
            "mask_path".to_owned(),
            mask_path.display().to_string().into(),
        );
    }

    if let Some((width, height)) = spec.dimensions {
        metadata.insert("width".to_owned(), width.into());
        metadata.insert("height".to_owned(), height.into());
    }

    if let Some(negative) = media::input_string(spec.inputs, "negative") {
        metadata.insert("negative_prompt".to_owned(), negative.into());
    }

    if let Some(model) = media::selected_model(spec.workflow, spec.inputs) {
        metadata.insert("model".to_owned(), model);
    }

    WorkflowArtifact {
        id: "image".to_owned(),
        kind: "image".to_owned(),
        path: spec.output_path.display().to_string(),
        mime_type: "image/png".to_owned(),
        metadata,
    }
}

pub(super) fn image_artifact(
    workflow: &WorkflowSpec,
    path: &std::path::Path,
    prompt: &str,
    width: u32,
    height: u32,
    seed: u64,
    inputs: &serde_json::Map<String, serde_json::Value>,
) -> WorkflowArtifact {
    let mut metadata = Map::new();
    metadata.insert("prompt".to_owned(), prompt.to_owned().into());
    metadata.insert("width".to_owned(), width.into());
    metadata.insert("height".to_owned(), height.into());
    metadata.insert("seed".to_owned(), seed.into());
    metadata.insert("engine".to_owned(), plan::PREVIEW_ENGINE.into());
    metadata.insert(
        "capability".to_owned(),
        plan::IMAGE_GENERATE_CAPABILITY.into(),
    );

    if let Some(negative) = media::input_string(inputs, "negative") {
        metadata.insert("negative_prompt".to_owned(), negative.into());
    }

    if let Some(model) = media::selected_model(workflow, inputs) {
        metadata.insert("model".to_owned(), model);
    }

    WorkflowArtifact {
        id: "image".to_owned(),
        kind: "image".to_owned(),
        path: path.display().to_string(),
        mime_type: "image/png".to_owned(),
        metadata,
    }
}

pub(super) fn image_transform_artifact(
    input_path: &std::path::Path,
    output_path: &std::path::Path,
    engine: &str,
    capability: &str,
    dimensions: Option<(u32, u32)>,
) -> WorkflowArtifact {
    let mut metadata = Map::new();
    metadata.insert("engine".to_owned(), engine.into());
    metadata.insert("capability".to_owned(), capability.into());
    metadata.insert(
        "source_image_path".to_owned(),
        input_path.display().to_string().into(),
    );

    if let Some((width, height)) = dimensions {
        metadata.insert("width".to_owned(), width.into());
        metadata.insert("height".to_owned(), height.into());
    }

    WorkflowArtifact {
        id: "image".to_owned(),
        kind: "image".to_owned(),
        path: output_path.display().to_string(),
        mime_type: "image/png".to_owned(),
        metadata,
    }
}

pub(super) fn image_file_artifact(
    input_path: &std::path::Path,
    output_path: &std::path::Path,
    engine: &str,
    capability: &str,
    dimensions: Option<(u32, u32)>,
) -> WorkflowArtifact {
    image_transform_artifact(input_path, output_path, engine, capability, dimensions)
}

pub(super) fn mask_artifact(
    mask_a_path: &std::path::Path,
    mask_b_path: &std::path::Path,
    output_path: &std::path::Path,
    mode: &str,
    dimensions: Option<(u32, u32)>,
) -> WorkflowArtifact {
    let mut metadata = Map::new();
    metadata.insert(
        "engine".to_owned(),
        Value::String("builtin.mask.compose.v1".to_owned()),
    );
    metadata.insert(
        "capability".to_owned(),
        plan::MASK_COMPOSE_CAPABILITY.into(),
    );
    metadata.insert("mode".to_owned(), mode.to_owned().into());
    metadata.insert(
        "mask_a_path".to_owned(),
        mask_a_path.display().to_string().into(),
    );
    metadata.insert(
        "mask_b_path".to_owned(),
        mask_b_path.display().to_string().into(),
    );

    if let Some((width, height)) = dimensions {
        metadata.insert("width".to_owned(), width.into());
        metadata.insert("height".to_owned(), height.into());
    }

    WorkflowArtifact {
        id: "mask".to_owned(),
        kind: "mask".to_owned(),
        path: output_path.display().to_string(),
        mime_type: "image/png".to_owned(),
        metadata,
    }
}
