use super::add::add_dependency;
use super::{CliError, CliResult, run_status};
use crate::api::ApiService;
use crate::workflow::ModelProvider;
use serde_json::json;
use std::process::Command;

mod agent_skills;
mod cache_metadata;
mod hf_downloads;
mod lockfile;
mod model_auto;
mod model_sources;
mod models;
mod modules;
mod options;

use agent_skills::{discover_agent_skills, plan_agent_skills, sync_agent_skills};
use hf_downloads::execute_hf_downloads_parallel;
use lockfile::{LFW_LOCK, verify_locked_downloads, write_lfw_lock};
use model_auto::{AutoModelSelection, HardwareInfo, auto_select_model_variants};
use models::{
    custom_hf_download_plan, hf_download_plan, model_variant_json, prompt_model_selections,
    select_custom_hf_models, select_model_variants,
};
use modules::{module_install_json, module_install_plans};
pub(super) use options::{SyncOptions, parse_sync_options};

pub(super) fn sync_project(
    service: &ApiService,
    options: &SyncOptions,
) -> CliResult<serde_json::Value> {
    let workflows = if let Some(workflow_id) = &options.workflow_id {
        let deps = service.workflow_dependencies(workflow_id)?;
        deps.workflows
            .into_iter()
            .map(|workflow_id| service.get_workflow(&workflow_id))
            .collect::<Result<Vec<_>, _>>()?
    } else {
        service
            .list_workflows()?
            .workflows
            .into_iter()
            .map(|summary| service.get_workflow(&summary.id))
            .collect::<Result<Vec<_>, _>>()?
    };
    let module_installs = module_install_plans(service.repo_root(), &workflows)?;
    let model_requirements = workflows
        .iter()
        .flat_map(|workflow| {
            workflow.models.iter().map(|model| {
                json!({
                    "workflow_id": workflow.id,
                    "id": model.id,
                    "capability": model.capability,
                    "variants": model.variants.iter().map(model_variant_json).collect::<Vec<_>>()
                })
            })
        })
        .collect::<Vec<_>>();
    let hardware = HardwareInfo::detect();
    let mut model_selections = options.model_selections.clone();
    let mut custom_hf_model_selections = options.custom_hf_models.clone();
    let auto_model_selections = if options.auto_model {
        auto_select_model_variants(
            &workflows,
            &hardware,
            &model_selections,
            &custom_hf_model_selections,
        )
    } else {
        Vec::new()
    };
    for selection in &auto_model_selections {
        model_selections.insert(
            selection.requirement_id.clone(),
            selection.variant_id.clone(),
        );
    }
    if options.select_model {
        prompt_model_selections(
            &workflows,
            &mut model_selections,
            &mut custom_hf_model_selections,
        )?;
    }
    let selected_models = select_model_variants(&workflows, &model_selections)?;
    let custom_hf_models = select_custom_hf_models(&workflows, &custom_hf_model_selections)?;
    let mut hf_downloads = selected_models
        .iter()
        .filter(|selection| selection.variant.provider == ModelProvider::HuggingFace)
        .map(|selection| hf_download_plan(selection))
        .collect::<Vec<_>>();
    hf_downloads.extend(
        custom_hf_models
            .iter()
            .map(|selection| custom_hf_download_plan(selection)),
    );
    let unresolved_models = workflows
        .iter()
        .flat_map(|workflow| {
            workflow.models.iter().filter_map(|model| {
                if model_selections.contains_key(&model.id)
                    || custom_hf_model_selections.contains_key(&model.id)
                {
                    return None;
                }
                Some(json!({
                    "workflow_id": workflow.id,
                    "id": model.id,
                    "capability": model.capability,
                    "variants": model.variants.iter().map(model_variant_json).collect::<Vec<_>>(),
                    "reason": if model.variants.is_empty() { "no concrete variants declared" } else { "model variant not selected" }
                }))
            })
        })
        .collect::<Vec<_>>();

    let lock_checks = if options.locked {
        verify_locked_downloads(
            service.repo_root(),
            options.workflow_id.as_deref(),
            &hf_downloads,
        )?
    } else {
        Vec::new()
    };
    let agent_skills = discover_agent_skills(service.repo_root())?;
    let skill_sync = if options.apply {
        sync_agent_skills(service.repo_root(), &agent_skills)?
    } else {
        plan_agent_skills(service.repo_root(), &agent_skills)?
    };
    let mut executed = Vec::new();
    let mut lock_downloads = Vec::new();
    if options.apply {
        if options.locked && !module_installs.is_empty() {
            let missing = module_installs
                .iter()
                .map(|module| module.options.crate_name.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            return Err(CliError::Usage(format!(
                "sync --locked cannot add missing module dependencies: {missing}"
            )));
        }
        for module in &module_installs {
            add_dependency(service.repo_root(), &module.options, false)?;
            executed.push(json!({
                "command": ["lfw", "add"],
                "dependency": module.options.crate_name,
            }));
        }
        if !options.locked {
            run_status(Command::new("cargo").arg("fetch"))?;
            executed.push(json!({ "command": ["cargo", "fetch"] }));
            let locked = execute_hf_downloads_parallel(&hf_downloads)?;
            executed.extend(locked.iter().cloned());
            lock_downloads.extend(locked);
            write_lfw_lock(
                service.repo_root(),
                options.workflow_id.as_deref(),
                &lock_downloads,
            )?;
        }
    }

    Ok(json!({
        "dry_run": !options.apply,
        "workflow_scope": options.workflow_id,
        "lock_file": service.repo_root().join(LFW_LOCK),
        "module_dependencies": {
            "manager": "cargo",
            "command": ["cargo", "fetch"],
            "installs": module_installs.iter().map(module_install_json).collect::<Vec<_>>(),
            "note": "Cargo resolves Rust workflow module dependencies."
        },
        "model_requirements": model_requirements,
        "hardware": hardware.to_json(),
        "auto_model": {
            "enabled": options.auto_model,
            "selections": auto_model_selections.iter().map(AutoModelSelection::to_json).collect::<Vec<_>>(),
        },
        "unresolved_models": unresolved_models,
        "hf_downloads": hf_downloads,
        "locked": {
            "enabled": options.locked,
            "checks": lock_checks,
        },
        "agent_skills": skill_sync,
        "executed": executed
    }))
}
