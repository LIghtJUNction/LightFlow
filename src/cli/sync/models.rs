use super::model_sources::{
    CustomHfModel, available_variant_summary, hf_download_url, infer_model_format,
    model_download_url, parse_custom_hf_model, parse_hf_url,
};
use crate::cli::{CliError, CliResult};
use crate::workflow::{ModelProvider, ModelRequirement, ModelVariant, WorkflowSpec};
use serde_json::json;
use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Write};
use std::process::Command;

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

#[derive(Debug, Clone, Eq, PartialEq)]
pub(super) struct HardwareInfo {
    total_ram_mb: Option<u64>,
    gpu_vram_mb: Option<u64>,
    gpu_name: Option<String>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(super) struct AutoModelSelection {
    pub(super) requirement_id: String,
    pub(super) variant_id: String,
    reason: String,
}

impl HardwareInfo {
    pub(super) fn detect() -> Self {
        Self {
            total_ram_mb: detect_total_ram_mb(),
            gpu_vram_mb: detect_gpu_vram_mb(),
            gpu_name: detect_gpu_name(),
        }
    }

    pub(super) fn to_json(&self) -> serde_json::Value {
        json!({
            "total_ram_mb": self.total_ram_mb,
            "gpu_vram_mb": self.gpu_vram_mb,
            "gpu_name": self.gpu_name,
        })
    }
}

impl AutoModelSelection {
    pub(super) fn to_json(&self) -> serde_json::Value {
        json!({
            "requirement_id": self.requirement_id,
            "variant_id": self.variant_id,
            "reason": self.reason,
        })
    }
}

fn detect_total_ram_mb() -> Option<u64> {
    if let Ok(value) = std::env::var("LFW_TOTAL_RAM_MB") {
        return value.parse().ok();
    }
    let meminfo = fs::read_to_string("/proc/meminfo").ok()?;
    let kb = meminfo
        .lines()
        .find_map(|line| line.strip_prefix("MemTotal:"))?
        .split_whitespace()
        .next()?
        .parse::<u64>()
        .ok()?;
    Some(kb / 1024)
}

fn detect_gpu_vram_mb() -> Option<u64> {
    if let Ok(value) = std::env::var("LFW_GPU_VRAM_MB") {
        return value.parse().ok();
    }
    let output = Command::new("nvidia-smi")
        .args(["--query-gpu=memory.total", "--format=csv,noheader,nounits"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .find_map(|line| line.trim().parse::<u64>().ok())
}

fn detect_gpu_name() -> Option<String> {
    if let Ok(value) = std::env::var("LFW_GPU_NAME") {
        return Some(value);
    }
    let output = Command::new("nvidia-smi")
        .args(["--query-gpu=name", "--format=csv,noheader"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_owned)
}

pub(super) fn auto_select_model_variants(
    workflows: &[WorkflowSpec],
    hardware: &HardwareInfo,
    explicit: &BTreeMap<String, String>,
    custom: &BTreeMap<String, CustomHfModel>,
) -> Vec<AutoModelSelection> {
    workflows
        .iter()
        .flat_map(|workflow| workflow.models.iter())
        .filter(|model| !explicit.contains_key(&model.id) && !custom.contains_key(&model.id))
        .filter_map(|model| {
            let variant = choose_variant(model, hardware)?;
            Some(AutoModelSelection {
                requirement_id: model.id.clone(),
                variant_id: variant.id.clone(),
                reason: auto_model_reason(hardware, variant),
            })
        })
        .collect()
}

fn choose_variant<'a>(
    model: &'a ModelRequirement,
    hardware: &HardwareInfo,
) -> Option<&'a ModelVariant> {
    if model.variants.is_empty() {
        return None;
    }
    if model
        .variants
        .iter()
        .all(|variant| variant.format != "gguf" && !variant.id.to_ascii_lowercase().contains("q"))
    {
        return model.variants.first();
    }
    let target = target_quant_level(hardware);
    model
        .variants
        .iter()
        .filter_map(|variant| quant_level(variant).map(|quant| (variant, quant)))
        .filter(|(_, quant)| *quant <= target)
        .max_by_key(|(variant, quant)| (*quant, q4_preference(variant)))
        .map(|(variant, _)| variant)
        .or_else(|| {
            model
                .variants
                .iter()
                .filter_map(|variant| quant_level(variant).map(|quant| (variant, quant)))
                .min_by_key(|(_, quant)| *quant)
                .map(|(variant, _)| variant)
        })
        .or_else(|| model.variants.first())
}

fn target_quant_level(hardware: &HardwareInfo) -> u8 {
    match (hardware.gpu_vram_mb, hardware.total_ram_mb) {
        (Some(vram), _) if vram >= 24 * 1024 => 8,
        (Some(vram), _) if vram >= 16 * 1024 => 5,
        (Some(vram), _) if vram >= 10 * 1024 => 4,
        (Some(_), _) => 3,
        (None, Some(ram)) if ram >= 64 * 1024 => 5,
        (None, Some(ram)) if ram >= 24 * 1024 => 4,
        _ => 4,
    }
}

fn quant_level(variant: &ModelVariant) -> Option<u8> {
    let text = format!(
        "{} {} {}",
        variant.id,
        variant.format,
        variant.file.as_deref().unwrap_or("")
    )
    .to_ascii_lowercase();
    if text.contains("q2") {
        Some(2)
    } else if text.contains("q3") {
        Some(3)
    } else if text.contains("q4") {
        Some(4)
    } else if text.contains("q5") {
        Some(5)
    } else if text.contains("q6") {
        Some(6)
    } else if text.contains("q8") {
        Some(8)
    } else if text.contains("f16") || text.contains("bf16") || text.contains("safetensors") {
        Some(16)
    } else {
        None
    }
}

fn q4_preference(variant: &ModelVariant) -> u8 {
    let text =
        format!("{} {}", variant.id, variant.file.as_deref().unwrap_or("")).to_ascii_lowercase();
    if text.contains("q4_k_m") {
        4
    } else if text.contains("q4_k_s") {
        3
    } else if text.contains("q4_1") {
        2
    } else if text.contains("q4_0") {
        1
    } else {
        0
    }
}

fn auto_model_reason(hardware: &HardwareInfo, variant: &ModelVariant) -> String {
    format!(
        "selected {} for detected gpu_vram_mb={:?}, total_ram_mb={:?}",
        variant.id, hardware.gpu_vram_mb, hardware.total_ram_mb
    )
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
