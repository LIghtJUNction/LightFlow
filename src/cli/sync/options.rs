use super::model_sources::{CustomHfModel, parse_custom_hf_model, parse_custom_hf_url};
use crate::cli::{CliError, CliResult};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Eq, PartialEq)]
pub(in crate::cli) struct SyncOptions {
    pub(super) workflow_id: Option<String>,
    pub(super) model_selections: BTreeMap<String, String>,
    pub(super) custom_hf_models: BTreeMap<String, CustomHfModel>,
    pub(super) auto_model: bool,
    pub(super) select_model: bool,
    pub(super) locked: bool,
    pub(super) apply: bool,
}

pub(in crate::cli) fn parse_sync_options(args: &[String]) -> CliResult<SyncOptions> {
    let mut workflow_id = None;
    let mut model_selections = BTreeMap::new();
    let mut custom_hf_models = BTreeMap::new();
    let mut auto_model = false;
    let mut select_model = false;
    let mut locked = false;
    let mut apply = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "-h" | "--help" | "help" => return Err(CliError::Usage(sync_usage())),
            "--apply" => {
                apply = true;
                index += 1;
            }
            "--dry-run" => {
                apply = false;
                index += 1;
            }
            "--auto-model" | "--best-model" => {
                auto_model = true;
                index += 1;
            }
            "--select-model" | "--choose-model" => {
                select_model = true;
                index += 1;
            }
            "--locked" => {
                locked = true;
                index += 1;
            }
            "--model" => {
                let value = required_sync_flag_value(args, index, "--model")?;
                let Some((requirement, variant)) = value.split_once('=') else {
                    return Err(CliError::Usage(
                        "--model must use <requirement=variant>".to_owned(),
                    ));
                };
                if requirement.is_empty() || variant.is_empty() {
                    return Err(CliError::Usage(
                        "--model must use <requirement=variant>".to_owned(),
                    ));
                }
                if custom_hf_models.contains_key(requirement) {
                    return Err(CliError::Usage(format!(
                        "model requirement {requirement} cannot use both --model and --hf-model"
                    )));
                }
                model_selections.insert(requirement.to_owned(), variant.to_owned());
                index += 2;
            }
            "--hf-model" | "--custom-model" => {
                let flag = args[index].as_str();
                let value = required_sync_flag_value(args, index, flag)?;
                let (requirement, custom_model) = parse_custom_hf_model(value, flag)?;
                if model_selections.contains_key(&requirement) {
                    return Err(CliError::Usage(format!(
                        "model requirement {requirement} cannot use both --model and {flag}"
                    )));
                }
                custom_hf_models.insert(requirement, custom_model);
                index += 2;
            }
            "--hf-url" => {
                let flag = args[index].as_str();
                let value = required_sync_flag_value(args, index, flag)?;
                let (requirement, custom_model) = parse_custom_hf_url(value, flag)?;
                if model_selections.contains_key(&requirement) {
                    return Err(CliError::Usage(format!(
                        "model requirement {requirement} cannot use both --model and {flag}"
                    )));
                }
                custom_hf_models.insert(requirement, custom_model);
                index += 2;
            }
            value if value.starts_with("--") => {
                return Err(CliError::Usage(sync_usage()));
            }
            value => {
                if workflow_id.is_some() {
                    return Err(CliError::Usage(sync_usage()));
                }
                workflow_id = Some(value.to_owned());
                index += 1;
            }
        }
    }
    Ok(SyncOptions {
        workflow_id,
        model_selections,
        custom_hf_models,
        auto_model,
        select_model,
        locked,
        apply,
    })
}

fn required_sync_flag_value<'a>(
    args: &'a [String],
    index: usize,
    _flag: &str,
) -> CliResult<&'a str> {
    let Some(value) = args.get(index + 1).map(String::as_str) else {
        return Err(CliError::Usage(sync_usage()));
    };
    if value.starts_with("--") {
        return Err(CliError::Usage(sync_usage()));
    }
    Ok(value)
}

pub(super) fn sync_usage() -> String {
    [
        "usage:",
        "  lfw sync [workflow_id] [--model <requirement=variant>] [--hf-model <requirement=format:repo[:file]>] [--hf-url <requirement=url>] [--auto-model|--select-model] [--locked] [--apply]",
        "",
        "Plans or applies workflow dependency synchronization for Cargo modules, model resources, and colocated agent skills.",
        "--model pins a declared model requirement to a named workflow variant.",
        "--hf-model and --hf-url register custom Hugging Face model sources.",
        "--auto-model chooses compatible variants from detected CPU RAM and NVIDIA VRAM; --select-model prompts interactively.",
        "--locked verifies existing lfw.lock model entries instead of downloading new model files.",
        "--apply writes dependency changes, model locks, and agent skill links; dry-run is the default.",
    ]
    .join("\n")
}
