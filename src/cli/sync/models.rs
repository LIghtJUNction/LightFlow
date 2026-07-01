use super::model_sources::{
    CustomHfModel, available_variant_summary, hf_download_url, infer_model_format,
    model_download_url, parse_custom_hf_model, parse_hf_url,
};
use crate::cli::{CliError, CliResult};
use crate::workflow::{ModelProvider, ModelRequirement, ModelVariant, WorkflowSpec};
use serde_json::json;
use std::collections::BTreeMap;
use std::io::{self, Write};

pub(super) fn prompt_model_selections(
    workflows: &[WorkflowSpec],
    model_selections: &mut BTreeMap<String, String>,
    custom_hf_models: &mut BTreeMap<String, CustomHfModel>,
) -> CliResult<()> {
    let stdin = io::stdin();
    let mut stderr = io::stderr();
    for model in workflows.iter().flat_map(|workflow| workflow.models.iter()) {
        if model_selections.contains_key(&model.id) || custom_hf_models.contains_key(&model.id) {
            continue;
        }
        writeln!(
            stderr,
            "\nSelect model for {} ({})",
            model.id, model.capability
        )?;
        for (index, variant) in model.variants.iter().enumerate() {
            writeln!(
                stderr,
                "  {}. {} [{}] {}",
                index + 1,
                variant.id,
                variant.format,
                model_download_url(variant).unwrap_or_else(|| variant.repo.clone())
            )?;
        }
        writeln!(
            stderr,
            "  c. custom Hugging Face model URL or format:repo[:file]"
        )?;
        write!(stderr, "Choice for {}: ", model.id)?;
        stderr.flush()?;
        let mut choice = String::new();
        stdin.read_line(&mut choice)?;
        let choice = choice.trim();
        if choice.eq_ignore_ascii_case("c") {
            write!(stderr, "Custom model for {}: ", model.id)?;
            stderr.flush()?;
            let mut custom = String::new();
            stdin.read_line(&mut custom)?;
            let custom = custom.trim();
            let custom_model = if custom.starts_with("http://") || custom.starts_with("https://") {
                let (repo, file) = parse_hf_url(custom)?;
                CustomHfModel {
                    format: file
                        .as_deref()
                        .and_then(infer_model_format)
                        .unwrap_or("custom")
                        .to_owned(),
                    repo,
                    file,
                }
            } else {
                parse_custom_hf_model(&format!("{}={custom}", model.id), "--select-model")?.1
            };
            custom_hf_models.insert(model.id.clone(), custom_model);
            continue;
        }
        let selected = choice.parse::<usize>().map_err(|_| {
            CliError::Usage(format!(
                "invalid choice for model requirement {}: {choice}",
                model.id
            ))
        })?;
        let Some(variant) = model.variants.get(selected.saturating_sub(1)) else {
            return Err(CliError::Usage(format!(
                "invalid choice for model requirement {}: {choice}",
                model.id
            )));
        };
        model_selections.insert(model.id.clone(), variant.id.clone());
    }
    Ok(())
}

pub(super) struct SelectedModel<'a> {
    requirement_id: &'a str,
    pub(super) variant: &'a ModelVariant,
}

pub(super) struct SelectedCustomHfModel<'a> {
    requirement_id: &'a str,
    model: &'a CustomHfModel,
}

pub(super) fn select_model_variants<'a>(
    workflows: &'a [WorkflowSpec],
    selections: &BTreeMap<String, String>,
) -> CliResult<Vec<SelectedModel<'a>>> {
    let mut selected = Vec::new();
    for (requirement_id, variant_id) in selections {
        let Some(model) = workflows
            .iter()
            .flat_map(|workflow| workflow.models.iter())
            .find(|model| model.id == *requirement_id)
        else {
            let available = workflows
                .iter()
                .flat_map(|workflow| workflow.models.iter())
                .map(|model| model.id.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            return Err(CliError::Usage(format!(
                "unknown model requirement: {requirement_id}. available requirements: {available}"
            )));
        };
        let Some(variant) = model
            .variants
            .iter()
            .find(|variant| variant.id == *variant_id)
        else {
            return Err(CliError::Usage(format!(
                "unknown variant {variant_id} for model requirement {requirement_id}. available variants: {}",
                available_variant_summary(model)
            )));
        };
        selected.push(SelectedModel {
            requirement_id: &model.id,
            variant,
        });
    }
    Ok(selected)
}

pub(super) fn select_custom_hf_models<'a>(
    workflows: &'a [WorkflowSpec],
    selections: &'a BTreeMap<String, CustomHfModel>,
) -> CliResult<Vec<SelectedCustomHfModel<'a>>> {
    let mut selected = Vec::new();
    for (requirement_id, model) in selections {
        let Some(requirement) = find_model_requirement(workflows, requirement_id) else {
            return Err(CliError::Usage(format!(
                "unknown model requirement: {requirement_id}. available requirements: {}",
                available_requirement_summary(workflows)
            )));
        };
        selected.push(SelectedCustomHfModel {
            requirement_id: &requirement.id,
            model,
        });
    }
    Ok(selected)
}

fn find_model_requirement<'a>(
    workflows: &'a [WorkflowSpec],
    requirement_id: &str,
) -> Option<&'a ModelRequirement> {
    workflows
        .iter()
        .flat_map(|workflow| workflow.models.iter())
        .find(|model| model.id == requirement_id)
}

fn available_requirement_summary(workflows: &[WorkflowSpec]) -> String {
    let available = workflows
        .iter()
        .flat_map(|workflow| workflow.models.iter())
        .map(|model| model.id.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    if available.is_empty() {
        "none".to_owned()
    } else {
        available
    }
}

pub(super) fn model_variant_json(variant: &ModelVariant) -> serde_json::Value {
    json!({
        "id": variant.id,
        "provider": variant.provider.as_str(),
        "format": variant.format,
        "repo": variant.repo,
        "file": variant.file,
        "download_url": model_download_url(variant),
    })
}

pub(super) fn hf_download_plan(selection: &SelectedModel<'_>) -> serde_json::Value {
    let mut command = vec![
        "hf".to_owned(),
        "download".to_owned(),
        selection.variant.repo.clone(),
    ];
    if let Some(file) = &selection.variant.file {
        command.push(file.clone());
    }
    json!({
        "requirement_id": selection.requirement_id,
        "variant_id": selection.variant.id,
        "custom": false,
        "provider": selection.variant.provider.as_str(),
        "format": selection.variant.format,
        "repo": selection.variant.repo,
        "file": selection.variant.file,
        "download_url": model_download_url(selection.variant),
        "command": command,
    })
}

pub(super) fn custom_hf_download_plan(selection: &SelectedCustomHfModel<'_>) -> serde_json::Value {
    let mut command = vec![
        "hf".to_owned(),
        "download".to_owned(),
        selection.model.repo.clone(),
    ];
    if let Some(file) = &selection.model.file {
        command.push(file.clone());
    }
    json!({
        "requirement_id": selection.requirement_id,
        "variant_id": "custom",
        "custom": true,
        "provider": ModelProvider::HuggingFace.as_str(),
        "format": selection.model.format,
        "repo": selection.model.repo,
        "file": selection.model.file,
        "download_url": hf_download_url(&selection.model.repo, selection.model.file.as_deref()),
        "command": command,
    })
}
