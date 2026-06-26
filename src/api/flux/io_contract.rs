use super::{FluxBackend, FluxRunRequest};
use crate::api::media_paths::{MediaKind, MediaPathProvider, expand_tilde};
use crate::api::{ApiError, ApiResult};
use crate::workflow::{WorkflowArtifact, WorkflowSpec};
use std::path::{Path, PathBuf};

pub(super) fn required_input_path(
    inputs: &serde_json::Map<String, serde_json::Value>,
    name: &str,
) -> ApiResult<PathBuf> {
    let path = input_string(inputs, name)
        .map(|path| expand_tilde(PathBuf::from(path)))
        .ok_or_else(|| {
            ApiError::InvalidRequest(format!("{name} is required for FLUX generation"))
        })?;
    if !path.is_file() {
        return Err(ApiError::InvalidRequest(format!(
            "{name} does not point to a file: {}",
            path.display()
        )));
    }
    Ok(path)
}

pub(super) fn input_string(
    inputs: &serde_json::Map<String, serde_json::Value>,
    name: &str,
) -> Option<String> {
    inputs.get(name).and_then(|value| match value {
        serde_json::Value::String(value) => Some(value.clone()),
        value if !value.is_null() => Some(value.to_string()),
        _ => None,
    })
}

pub(super) fn input_i32(
    inputs: &serde_json::Map<String, serde_json::Value>,
    name: &str,
) -> Option<i32> {
    inputs
        .get(name)
        .and_then(serde_json::Value::as_i64)
        .and_then(|value| i32::try_from(value).ok())
}

pub(super) fn input_i64(
    inputs: &serde_json::Map<String, serde_json::Value>,
    name: &str,
) -> Option<i64> {
    inputs.get(name).and_then(serde_json::Value::as_i64)
}

pub(super) fn input_count(inputs: &serde_json::Map<String, serde_json::Value>) -> usize {
    ["count", "num_images", "batch_count"]
        .into_iter()
        .find_map(|name| input_i32(inputs, name))
        .unwrap_or(1)
        .clamp(1, 256) as usize
}

pub(super) fn input_f32(
    inputs: &serde_json::Map<String, serde_json::Value>,
    name: &str,
) -> Option<f32> {
    inputs
        .get(name)
        .and_then(serde_json::Value::as_f64)
        .map(|value| value as f32)
}

pub(super) fn output_paths(
    root: &Path,
    workflow: &WorkflowSpec,
    inputs: &serde_json::Map<String, serde_json::Value>,
    seed: u64,
    count: usize,
) -> Vec<PathBuf> {
    if let Some(template) = input_string(inputs, "output_template")
        .or_else(|| input_string(inputs, "output_path").filter(|path| path_contains_template(path)))
    {
        return (1..=count)
            .map(|index| {
                expand_tilde(PathBuf::from(render_output_template(
                    &template,
                    workflow,
                    index,
                    seed.saturating_add(index as u64 - 1),
                )))
            })
            .collect();
    }

    if let Some(path) = input_string(inputs, "output_path").map(PathBuf::from) {
        let path = expand_tilde(path);
        if count == 1 {
            return vec![path];
        }
        return (1..=count)
            .map(|index| indexed_output_path(&path, index))
            .collect();
    }

    let base = MediaPathProvider::new(root).default_output_path(
        MediaKind::Image,
        workflow,
        if count == 1 {
            format!("{seed}.png")
        } else {
            "{seed}-{index:03}.png".to_owned()
        },
    );
    output_paths(
        root,
        workflow,
        &serde_json::Map::from_iter([(
            "output_template".to_owned(),
            base.display().to_string().into(),
        )]),
        seed,
        count,
    )
}

fn path_contains_template(path: &str) -> bool {
    path.contains("{index") || path.contains("{seed}") || path.contains("{workflow_id}")
}

fn render_output_template(
    template: &str,
    workflow: &WorkflowSpec,
    index: usize,
    seed: u64,
) -> String {
    let mut output = template
        .replace("{index}", &index.to_string())
        .replace("{index0}", &(index - 1).to_string())
        .replace("{seed}", &seed.to_string())
        .replace("{workflow_id}", &workflow.id);
    for width in 1..=9 {
        let placeholder = format!("{{index:0{width}}}");
        let value = format!("{index:0width$}");
        output = output.replace(&placeholder, &value);
    }
    output
}

fn indexed_output_path(path: &Path, index: usize) -> PathBuf {
    let parent = path.parent().unwrap_or_else(|| Path::new(""));
    let stem = path
        .file_stem()
        .and_then(std::ffi::OsStr::to_str)
        .unwrap_or("image");
    let extension = path
        .extension()
        .and_then(std::ffi::OsStr::to_str)
        .unwrap_or("png");
    parent.join(format!("{stem}-{index:03}.{extension}"))
}

pub(super) fn flux_artifact(
    workflow: &WorkflowSpec,
    request: &FluxRunRequest<'_>,
    backend: FluxBackend,
    index: usize,
    count: usize,
) -> WorkflowArtifact {
    let mut metadata = serde_json::Map::new();
    metadata.insert("capability".to_owned(), request.task.capability().into());
    metadata.insert("engine".to_owned(), backend.engine().into());
    metadata.insert("task".to_owned(), request.task.as_str().into());
    metadata.insert("workflow_id".to_owned(), workflow.id.clone().into());
    metadata.insert("prompt".to_owned(), request.prompt.to_owned().into());
    metadata.insert("width".to_owned(), request.width.into());
    metadata.insert("height".to_owned(), request.height.into());
    metadata.insert("seed".to_owned(), request.seed.into());
    metadata.insert("index".to_owned(), index.into());
    metadata.insert("count".to_owned(), count.into());
    metadata.insert("steps".to_owned(), request.steps.into());
    metadata.insert("guidance".to_owned(), request.guidance.into());
    metadata.insert("strength".to_owned(), request.strength.into());
    if let Some(image_path) = request.image_path {
        metadata.insert(
            "source_image_path".to_owned(),
            image_path.display().to_string().into(),
        );
    }
    if let Some(mask_path) = request.mask_path {
        metadata.insert(
            "mask_path".to_owned(),
            mask_path.display().to_string().into(),
        );
    }
    metadata.insert(
        "flux_model".to_owned(),
        request.models.diffusion_model.display().to_string().into(),
    );
    metadata.insert(
        "llm_model".to_owned(),
        request.models.llm.display().to_string().into(),
    );
    metadata.insert(
        "vae_model".to_owned(),
        request.models.vae.display().to_string().into(),
    );

    WorkflowArtifact {
        id: "image".to_owned(),
        kind: "image".to_owned(),
        path: request.output_path.display().to_string(),
        mime_type: "image/png".to_owned(),
        metadata,
    }
}
