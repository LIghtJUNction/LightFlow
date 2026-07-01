use super::model_sources::CustomHfModel;
use crate::workflow::{ModelRequirement, ModelVariant, WorkflowSpec};
use serde_json::json;
use std::collections::BTreeMap;
use std::fs;
use std::process::Command;

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
