use super::add::{AddDependencyOptions, DependencySource, add_dependency};
use super::{CliError, CliResult, required_flag_value, run_status};
use crate::api::ApiService;
use crate::workflow::{
    CargoDependency, CargoDependencySource, ModelProvider, ModelRequirement, ModelVariant,
    WorkflowSpec,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::io::{self, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use toml_edit::{DocumentMut, Item};

const LFW_LOCK: &str = "lfw.lock";
const HF_HUB_DOWNLOAD_SCRIPT: &str = r#"
import json
import sys
from huggingface_hub import hf_hub_download, snapshot_download

repo_id = sys.argv[1]
filename = sys.argv[2] if len(sys.argv) > 2 else None
try:
    if filename:
        path = hf_hub_download(repo_id=repo_id, filename=filename)
    else:
        path = snapshot_download(repo_id=repo_id)
    print(json.dumps({"path": path}))
except Exception as error:
    print(f"Error: {error}", file=sys.stderr)
    raise SystemExit(1)
"#;

#[derive(Debug, Clone, Eq, PartialEq)]
pub(super) struct SyncOptions {
    pub(super) workflow_id: Option<String>,
    pub(super) model_selections: BTreeMap<String, String>,
    pub(super) custom_hf_models: BTreeMap<String, CustomHfModel>,
    pub(super) auto_model: bool,
    pub(super) select_model: bool,
    pub(super) locked: bool,
    pub(super) apply: bool,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(super) struct CustomHfModel {
    pub(super) format: String,
    pub(super) repo: String,
    pub(super) file: Option<String>,
}

pub(super) fn parse_sync_options(args: &[String]) -> CliResult<SyncOptions> {
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
                let value = required_flag_value(args, index, "--model")?;
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
                let value = required_flag_value(args, index, flag)?;
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
                let value = required_flag_value(args, index, flag)?;
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
                return Err(CliError::Usage(format!(
                    "unexpected argument for sync: {value}"
                )));
            }
            value => {
                if workflow_id.is_some() {
                    return Err(CliError::Usage(format!(
                        "unexpected argument for sync: {value}"
                    )));
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

fn parse_custom_hf_model(value: &str, flag: &str) -> CliResult<(String, CustomHfModel)> {
    let Some((requirement, spec)) = value.split_once('=') else {
        return Err(CliError::Usage(format!(
            "{flag} must use <requirement=format:repo[:file]>"
        )));
    };
    let Some((format, location)) = spec.split_once(':') else {
        return Err(CliError::Usage(format!(
            "{flag} must use <requirement=format:repo[:file]>"
        )));
    };
    let (repo, file) = parse_hf_location(location)?;
    if requirement.is_empty() || format.is_empty() || repo.is_empty() {
        return Err(CliError::Usage(format!(
            "{flag} must use <requirement=format:repo[:file]>"
        )));
    }
    Ok((
        requirement.to_owned(),
        CustomHfModel {
            format: format.to_owned(),
            repo: repo.to_owned(),
            file,
        },
    ))
}

fn parse_custom_hf_url(value: &str, flag: &str) -> CliResult<(String, CustomHfModel)> {
    let Some((requirement, url)) = value.split_once('=') else {
        return Err(CliError::Usage(format!(
            "{flag} must use <requirement=url>"
        )));
    };
    let (repo, file) = parse_hf_url(url)?;
    let format = file
        .as_deref()
        .and_then(infer_model_format)
        .unwrap_or("custom")
        .to_owned();
    Ok((requirement.to_owned(), CustomHfModel { format, repo, file }))
}

fn parse_hf_location(location: &str) -> CliResult<(String, Option<String>)> {
    if location.starts_with("https://huggingface.co/")
        || location.starts_with("http://huggingface.co/")
    {
        return parse_hf_url(location);
    }
    let (repo, file) = match location.split_once(':') {
        Some((repo, file)) => (
            repo.to_owned(),
            Some(file.to_owned()).filter(|file| !file.is_empty()),
        ),
        None => (location.to_owned(), None),
    };
    Ok((repo, file))
}

fn parse_hf_url(url: &str) -> CliResult<(String, Option<String>)> {
    let path = url
        .strip_prefix("https://huggingface.co/")
        .or_else(|| url.strip_prefix("http://huggingface.co/"))
        .ok_or_else(|| CliError::Usage(format!("unsupported Hugging Face URL: {url}")))?;
    if let Some((repo, rest)) = path
        .split_once("/resolve/")
        .or_else(|| path.split_once("/blob/"))
    {
        let file = rest
            .split_once('/')
            .map(|(_, file)| file)
            .filter(|file| !file.is_empty())
            .ok_or_else(|| {
                CliError::Usage(format!(
                    "Hugging Face file URL is missing a filename: {url}"
                ))
            })?;
        return Ok((repo.to_owned(), Some(file.to_owned())));
    }
    Ok((path.trim_end_matches('/').to_owned(), None))
}

fn infer_model_format(file: &str) -> Option<&'static str> {
    let lower = file.to_ascii_lowercase();
    if lower.ends_with(".gguf") {
        Some("gguf")
    } else if lower.ends_with(".safetensors") {
        Some("safetensors")
    } else if lower.ends_with(".bin") {
        Some("bin")
    } else {
        None
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LfwLock {
    version: u32,
    #[serde(default)]
    models: BTreeMap<String, serde_json::Value>,
    #[serde(default)]
    skills: BTreeMap<String, SkillLockEntry>,
}

impl Default for LfwLock {
    fn default() -> Self {
        Self {
            version: 2,
            models: BTreeMap::new(),
            skills: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
struct SkillLockEntry {
    source: String,
    choice: String,
    target: Option<String>,
    link: Option<String>,
}

fn prompt_model_selections(
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

fn execute_hf_downloads_parallel(
    downloads: &[serde_json::Value],
) -> CliResult<Vec<serde_json::Value>> {
    let mut handles = Vec::new();
    for (index, download) in downloads.iter().cloned().enumerate() {
        handles.push(thread::spawn(move || {
            execute_hf_download(&download).map(|locked| (index, locked))
        }));
    }
    let mut locked = Vec::new();
    for handle in handles {
        let result = handle
            .join()
            .map_err(|_| CliError::Usage("hf download worker panicked".to_owned()))??;
        locked.push(result);
    }
    locked.sort_by_key(|(index, _)| *index);
    Ok(locked.into_iter().map(|(_, download)| download).collect())
}

fn execute_hf_download(download: &serde_json::Value) -> CliResult<serde_json::Value> {
    let repo = download["repo"]
        .as_str()
        .ok_or_else(|| CliError::Usage("invalid hf download plan".to_owned()))?;
    let mut process = Command::new("python3");
    process.arg("-c").arg(HF_HUB_DOWNLOAD_SCRIPT).arg(repo);
    if let Some(file) = download["file"].as_str() {
        process.arg(file);
    }
    if let Some(target) = hf_download_target(download) {
        eprintln!("Downloading Hugging Face model: {target}");
    }
    process.stdout(Stdio::piped()).stderr(Stdio::inherit());
    let mut child = process.spawn()?;
    let mut stdout = Vec::new();
    if let Some(mut child_stdout) = child.stdout.take() {
        child_stdout.read_to_end(&mut stdout)?;
    }
    let status = child.wait()?;

    let hf_output = serde_json::from_slice::<serde_json::Value>(&stdout).ok();
    let mut local_paths = hf_output.as_ref().map(extract_hf_paths).unwrap_or_default();
    if local_paths.is_empty() {
        local_paths = extract_hf_paths_from_text(&String::from_utf8_lossy(&stdout));
    }
    if !status.success() && local_paths.is_empty() {
        return Err(CliError::Usage(hf_download_failure_message(
            download, status, "",
        )));
    }
    let (sha256, size_bytes, snapshot_revision) = if local_paths.len() == 1 {
        let path = Path::new(&local_paths[0]);
        (
            sha256_file(path)?,
            file_size(path)?,
            hf_snapshot_revision(path).map(str::to_owned),
        )
    } else {
        (None, None, None)
    };

    let mut executed = download.clone();
    if let Some(object) = executed.as_object_mut() {
        object.insert(
            "hf_output".to_owned(),
            hf_output.unwrap_or(serde_json::Value::Null),
        );
        object.insert("local_paths".to_owned(), json!(local_paths));
        object.insert("sha256".to_owned(), json!(sha256));
        object.insert("size_bytes".to_owned(), json!(size_bytes));
        object.insert("snapshot_revision".to_owned(), json!(snapshot_revision));
        object.insert(
            "hash_algorithm".to_owned(),
            if sha256.is_some() {
                json!("sha256")
            } else {
                serde_json::Value::Null
            },
        );
    }
    Ok(executed)
}

fn hf_download_target(download: &serde_json::Value) -> Option<String> {
    let repo = download["repo"].as_str()?;
    match download["file"].as_str() {
        Some(file) => Some(format!("{repo}/{file}")),
        None => Some(repo.to_owned()),
    }
}

fn hf_download_failure_message(
    download: &serde_json::Value,
    status: std::process::ExitStatus,
    stderr: &str,
) -> String {
    let stderr = stderr.trim_end();
    let mut message = if stderr.is_empty() {
        format!("command failed with status {status}")
    } else {
        format!("command failed with status {status}\n{stderr}")
    };
    let repo = download["repo"].as_str();
    let file = download["file"].as_str();
    let download_url = download["download_url"].as_str();
    if repo.is_some() || file.is_some() || download_url.is_some() {
        message.push_str("\n\nHugging Face download target:");
        if let Some(repo) = repo {
            message.push_str(&format!("\n  repo: {repo}"));
            message.push_str(&format!("\n  repo_url: https://huggingface.co/{repo}"));
        }
        if let Some(file) = file {
            message.push_str(&format!("\n  file: {file}"));
        }
        if let Some(download_url) = download_url {
            message.push_str(&format!("\n  file_url: {download_url}"));
        }
    }
    if is_hf_browser_approval_error(stderr) {
        message.push_str(
            "\n\nThis Hugging Face repository appears to require browser approval. \
Log in to Hugging Face, open the repo_url above, accept the access terms, then rerun `lfw sync`.",
        );
    } else if repo.is_some() {
        message.push_str(
            "\n\nIf this Hugging Face repository requires browser approval, \
log in to Hugging Face, open the repo_url above, accept the access terms, then rerun `lfw sync`.",
        );
    }
    message
}

fn is_hf_browser_approval_error(stderr: &str) -> bool {
    let lower = stderr.to_ascii_lowercase();
    lower.contains("requires approval") || lower.contains("access denied")
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct AgentSkillCandidate {
    name: String,
    source_dir: PathBuf,
}

fn discover_agent_skills(root: &Path) -> CliResult<Vec<AgentSkillCandidate>> {
    let mut skills = BTreeMap::<String, AgentSkillCandidate>::new();
    collect_agent_skill_roots(root, &mut skills)?;
    collect_workflow_collection_agent_skills(&root.join("workflows"), &mut skills)?;
    collect_workflow_collection_agent_skills(
        &root.join("lightflow").join("workflows"),
        &mut skills,
    )?;
    if let Ok(lfw_path) = std::env::var("LFW_PATH") {
        for path in std::env::split_paths(&lfw_path) {
            collect_agent_skill_roots(&path, &mut skills)?;
            collect_workflow_collection_agent_skills(&path, &mut skills)?;
            collect_workflow_collection_agent_skills(&path.join("workflows"), &mut skills)?;
            collect_workflow_collection_agent_skills(
                &path.join("lightflow").join("workflows"),
                &mut skills,
            )?;
        }
    }
    Ok(skills.into_values().collect())
}

fn collect_agent_skill_roots(
    root: &Path,
    skills: &mut BTreeMap<String, AgentSkillCandidate>,
) -> CliResult<()> {
    collect_agent_skills_from(&root.join(".agent").join("skills"), skills)
}

fn collect_workflow_collection_agent_skills(
    collection: &Path,
    skills: &mut BTreeMap<String, AgentSkillCandidate>,
) -> CliResult<()> {
    let Ok(categories) = fs::read_dir(collection) else {
        return Ok(());
    };
    for category in categories {
        let category = category?.path();
        if !category.is_dir() {
            continue;
        }
        let Ok(workflows) = fs::read_dir(&category) else {
            continue;
        };
        for workflow in workflows {
            let workflow = workflow?.path();
            if workflow.is_dir() {
                collect_agent_skills_from(&workflow.join(".agent").join("skills"), skills)?;
            }
        }
    }
    Ok(())
}

fn collect_agent_skills_from(
    skills_dir: &Path,
    skills: &mut BTreeMap<String, AgentSkillCandidate>,
) -> CliResult<()> {
    let Ok(entries) = fs::read_dir(skills_dir) else {
        return Ok(());
    };
    for entry in entries {
        let source_dir = entry?.path();
        if !source_dir.is_dir() || !source_dir.join("SKILL.md").is_file() {
            continue;
        }
        let Some(name) = source_dir.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let source_dir = normalize_skill_path(&source_dir);
        let key = skill_lock_key(name, &source_dir);
        skills.entry(key).or_insert_with(|| AgentSkillCandidate {
            name: name.to_owned(),
            source_dir,
        });
    }
    Ok(())
}

fn normalize_skill_path(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

fn plan_agent_skills(
    root: &Path,
    candidates: &[AgentSkillCandidate],
) -> CliResult<serde_json::Value> {
    let lock = read_lfw_lock_optional(root)?;
    let pending = candidates
        .iter()
        .filter(|candidate| {
            !lock
                .skills
                .contains_key(&skill_lock_key(&candidate.name, &candidate.source_dir))
        })
        .map(agent_skill_json)
        .collect::<Vec<_>>();
    let locked = candidates
        .iter()
        .filter_map(|candidate| {
            let key = skill_lock_key(&candidate.name, &candidate.source_dir);
            lock.skills.get(&key).map(|entry| {
                json!({
                    "key": key,
                    "name": candidate.name,
                    "source": candidate.source_dir,
                    "choice": entry.choice,
                    "target": entry.target,
                    "link": entry.link,
                })
            })
        })
        .collect::<Vec<_>>();
    Ok(json!({
        "available": candidates.iter().map(agent_skill_json).collect::<Vec<_>>(),
        "pending": pending,
        "locked": locked,
        "installed": [],
        "skipped": [],
    }))
}

fn sync_agent_skills(
    root: &Path,
    candidates: &[AgentSkillCandidate],
) -> CliResult<serde_json::Value> {
    let mut lock = read_lfw_lock_optional(root)?;
    let mut installed = Vec::new();
    let mut skipped = Vec::new();
    let mut locked = Vec::new();
    let mut changed = false;
    for candidate in candidates {
        let key = skill_lock_key(&candidate.name, &candidate.source_dir);
        if let Some(entry) = lock.skills.get(&key) {
            locked.push(json!({
                "key": key,
                "name": candidate.name,
                "source": candidate.source_dir,
                "choice": entry.choice,
                "target": entry.target,
                "link": entry.link,
            }));
            continue;
        }
        match prompt_agent_skill_install(root, candidate)? {
            AgentSkillChoice::Project => {
                let target = root.join(".agents").join("skills");
                let link = install_agent_skill(candidate, &target)?;
                lock.skills.insert(
                    key.clone(),
                    SkillLockEntry {
                        source: candidate.source_dir.display().to_string(),
                        choice: "project".to_owned(),
                        target: Some(target.display().to_string()),
                        link: Some(link.display().to_string()),
                    },
                );
                installed.push(json!({
                    "key": key,
                    "name": candidate.name,
                    "source": candidate.source_dir,
                    "target": target,
                    "link": link,
                    "scope": "project",
                }));
                changed = true;
            }
            AgentSkillChoice::Global => {
                let target = global_agent_skill_dir()?;
                let link = install_agent_skill(candidate, &target)?;
                lock.skills.insert(
                    key.clone(),
                    SkillLockEntry {
                        source: candidate.source_dir.display().to_string(),
                        choice: "global".to_owned(),
                        target: Some(target.display().to_string()),
                        link: Some(link.display().to_string()),
                    },
                );
                installed.push(json!({
                    "key": key,
                    "name": candidate.name,
                    "source": candidate.source_dir,
                    "target": target,
                    "link": link,
                    "scope": "global",
                }));
                changed = true;
            }
            AgentSkillChoice::Skip => {
                lock.skills.insert(
                    key.clone(),
                    SkillLockEntry {
                        source: candidate.source_dir.display().to_string(),
                        choice: "skip".to_owned(),
                        target: None,
                        link: None,
                    },
                );
                skipped.push(json!({
                    "key": key,
                    "name": candidate.name,
                    "source": candidate.source_dir,
                }));
                changed = true;
            }
        }
    }
    if changed {
        write_lfw_lock_file(root, &lock)?;
    }
    Ok(json!({
        "available": candidates.iter().map(agent_skill_json).collect::<Vec<_>>(),
        "pending": [],
        "locked": locked,
        "installed": installed,
        "skipped": skipped,
    }))
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum AgentSkillChoice {
    Project,
    Global,
    Skip,
}

fn prompt_agent_skill_install(
    root: &Path,
    candidate: &AgentSkillCandidate,
) -> CliResult<AgentSkillChoice> {
    let project = root.join(".agents").join("skills");
    let global = global_agent_skill_dir()?;
    let mut stderr = io::stderr();
    writeln!(
        stderr,
        "\nInstall agent skill {} from {}?",
        candidate.name,
        candidate.source_dir.display()
    )?;
    writeln!(stderr, "  p. project ({})", project.display())?;
    writeln!(stderr, "  g. global ({})", global.display())?;
    writeln!(stderr, "  s. skip")?;
    write!(stderr, "Choice for skill {} [s]: ", candidate.name)?;
    stderr.flush()?;
    let mut choice = String::new();
    io::stdin().read_line(&mut choice)?;
    match choice.trim().to_ascii_lowercase().as_str() {
        "p" | "project" => Ok(AgentSkillChoice::Project),
        "g" | "global" => Ok(AgentSkillChoice::Global),
        "" | "s" | "skip" => Ok(AgentSkillChoice::Skip),
        value => Err(CliError::Usage(format!(
            "invalid choice for agent skill {}: {value}",
            candidate.name
        ))),
    }
}

fn install_agent_skill(candidate: &AgentSkillCandidate, target: &Path) -> CliResult<PathBuf> {
    fs::create_dir_all(target)?;
    let link = target.join(&candidate.name);
    if link.exists() {
        if fs::read_link(&link)
            .map(|existing| normalize_skill_path(&existing) == candidate.source_dir)
            .unwrap_or(false)
        {
            return Ok(link);
        }
        return Err(CliError::Usage(format!(
            "agent skill target already exists: {}",
            link.display()
        )));
    }
    symlink_dir(&candidate.source_dir, &link)?;
    Ok(link)
}

#[cfg(unix)]
fn symlink_dir(source: &Path, link: &Path) -> CliResult<()> {
    std::os::unix::fs::symlink(source, link).map_err(CliError::from)
}

#[cfg(windows)]
fn symlink_dir(source: &Path, link: &Path) -> CliResult<()> {
    std::os::windows::fs::symlink_dir(source, link).map_err(CliError::from)
}

fn global_agent_skill_dir() -> CliResult<PathBuf> {
    let home = std::env::var_os("HOME")
        .ok_or_else(|| CliError::Usage("HOME is required for global agent skills".to_owned()))?;
    Ok(PathBuf::from(home).join(".agents").join("skills"))
}

fn agent_skill_json(candidate: &AgentSkillCandidate) -> serde_json::Value {
    json!({
        "name": candidate.name,
        "source": candidate.source_dir,
    })
}

fn skill_lock_key(name: &str, source_dir: &Path) -> String {
    format!("{name}::{}", source_dir.display())
}

fn write_lfw_lock(
    root: &Path,
    workflow_scope: Option<&str>,
    downloads: &[serde_json::Value],
) -> CliResult<()> {
    if downloads.is_empty() {
        return Ok(());
    }
    let mut lock = read_lfw_lock_optional(root)?;
    for download in downloads {
        let requirement_id = download["requirement_id"].as_str().unwrap_or("unknown");
        let key = lock_key(workflow_scope, requirement_id);
        let mut entry = download.clone();
        if let Some(object) = entry.as_object_mut() {
            object.insert("workflow_scope".to_owned(), json!(workflow_scope));
        }
        lock.models.insert(key, entry);
    }
    write_lfw_lock_file(root, &lock)
}

fn write_lfw_lock_file(root: &Path, lock: &LfwLock) -> CliResult<()> {
    let path = root.join(LFW_LOCK);
    let mut bytes = serde_json::to_vec_pretty(&lock)?;
    bytes.push(b'\n');
    fs::write(path, bytes)?;
    Ok(())
}

fn read_lfw_lock(root: &Path) -> CliResult<LfwLock> {
    let path = root.join(LFW_LOCK);
    if !path.exists() {
        return Err(CliError::Usage(format!(
            "sync --locked requires {}",
            path.display()
        )));
    }
    Ok(serde_json::from_slice::<LfwLock>(&fs::read(&path)?)?)
}

fn read_lfw_lock_optional(root: &Path) -> CliResult<LfwLock> {
    let path = root.join(LFW_LOCK);
    if path.exists() {
        Ok(serde_json::from_slice::<LfwLock>(&fs::read(&path)?)?)
    } else {
        Ok(LfwLock::default())
    }
}

fn verify_locked_downloads(
    root: &Path,
    workflow_scope: Option<&str>,
    downloads: &[serde_json::Value],
) -> CliResult<Vec<serde_json::Value>> {
    if downloads.is_empty() {
        return Ok(Vec::new());
    }
    let lock = read_lfw_lock(root)?;
    downloads
        .iter()
        .map(|download| verify_locked_download(workflow_scope, download, &lock))
        .collect()
}

fn verify_locked_download(
    workflow_scope: Option<&str>,
    download: &serde_json::Value,
    lock: &LfwLock,
) -> CliResult<serde_json::Value> {
    let requirement_id = download["requirement_id"]
        .as_str()
        .ok_or_else(|| CliError::Usage("invalid hf download plan".to_owned()))?;
    let key = lock_key(workflow_scope, requirement_id);
    let entry = lock.models.get(&key).ok_or_else(|| {
        CliError::Usage(format!(
            "sync --locked is missing model lock entry for {key}"
        ))
    })?;
    for field in ["repo", "file", "variant_id", "format"] {
        if entry.get(field) != download.get(field) {
            return Err(CliError::Usage(format!(
                "sync --locked model lock mismatch for {key}: field {field}"
            )));
        }
    }
    let local_path = entry["local_paths"]
        .as_array()
        .and_then(|paths| paths.first())
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            CliError::Usage(format!(
                "sync --locked model lock entry for {key} has no local path"
            ))
        })?;
    let local_path = Path::new(local_path);
    if !local_path.is_file() {
        return Err(CliError::Usage(format!(
            "sync --locked cached model file is missing for {key}: {}",
            local_path.display()
        )));
    }
    if let Some(expected_size) = entry["size_bytes"].as_u64() {
        let actual_size = fs::metadata(local_path)?.len();
        if actual_size != expected_size {
            return Err(CliError::Usage(format!(
                "sync --locked size mismatch for {key}: expected {expected_size}, got {actual_size}"
            )));
        }
    }
    if let Some(expected_sha256) = entry["sha256"].as_str() {
        let actual_sha256 = sha256_file(local_path)?.ok_or_else(|| {
            CliError::Usage(format!(
                "sync --locked cannot hash cached model file for {key}"
            ))
        })?;
        if actual_sha256 != expected_sha256 {
            return Err(CliError::Usage(format!(
                "sync --locked sha256 mismatch for {key}"
            )));
        }
    }
    Ok(json!({
        "requirement_id": requirement_id,
        "key": key,
        "status": "verified",
        "local_path": local_path.to_string_lossy(),
        "sha256": entry["sha256"].clone(),
        "size_bytes": entry["size_bytes"].clone(),
        "snapshot_revision": entry["snapshot_revision"].clone(),
    }))
}

fn lock_key(workflow_scope: Option<&str>, requirement_id: &str) -> String {
    format!("{}::{requirement_id}", workflow_scope.unwrap_or("*"))
}

fn extract_hf_paths(value: &serde_json::Value) -> Vec<String> {
    match value {
        serde_json::Value::Object(object) => object
            .get("path")
            .and_then(serde_json::Value::as_str)
            .map(|path| vec![path.to_owned()])
            .unwrap_or_default(),
        serde_json::Value::Array(values) => values
            .iter()
            .filter_map(|value| {
                value
                    .get("path")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned)
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn extract_hf_paths_from_text(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| line.trim().strip_prefix("path: "))
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(str::to_owned)
        .collect()
}

fn sha256_file(path: &Path) -> CliResult<Option<String>> {
    if !path.is_file() {
        return Ok(None);
    }
    let file = fs::File::open(path)?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 1024 * 1024];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(Some(hex_lower(&hasher.finalize())))
}

fn file_size(path: &Path) -> CliResult<Option<u64>> {
    if !path.is_file() {
        return Ok(None);
    }
    Ok(Some(fs::metadata(path)?.len()))
}

fn hf_snapshot_revision(path: &Path) -> Option<&str> {
    let mut previous_was_snapshots = false;
    for component in path.components() {
        let text = component.as_os_str().to_str()?;
        if previous_was_snapshots {
            return Some(text);
        }
        previous_was_snapshots = text == "snapshots";
    }
    None
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct HardwareInfo {
    total_ram_mb: Option<u64>,
    gpu_vram_mb: Option<u64>,
    gpu_name: Option<String>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct AutoModelSelection {
    requirement_id: String,
    variant_id: String,
    reason: String,
}

impl HardwareInfo {
    fn detect() -> Self {
        Self {
            total_ram_mb: detect_total_ram_mb(),
            gpu_vram_mb: detect_gpu_vram_mb(),
            gpu_name: detect_gpu_name(),
        }
    }

    fn to_json(&self) -> serde_json::Value {
        json!({
            "total_ram_mb": self.total_ram_mb,
            "gpu_vram_mb": self.gpu_vram_mb,
            "gpu_name": self.gpu_name,
        })
    }
}

impl AutoModelSelection {
    fn to_json(&self) -> serde_json::Value {
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

fn auto_select_model_variants(
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

#[derive(Debug, Clone, Eq, PartialEq)]
struct ModuleInstallPlan {
    workflow_id: String,
    required_by: String,
    options: AddDependencyOptions,
}

fn module_install_plans(
    root: &Path,
    workflows: &[WorkflowSpec],
) -> CliResult<Vec<ModuleInstallPlan>> {
    let installed = installed_dependency_names(root)?;
    let mut plans = BTreeMap::<String, ModuleInstallPlan>::new();
    for workflow in workflows {
        for dependency in &workflow.dependencies {
            let Some(install) = &dependency.install else {
                continue;
            };
            if installed.contains_key(&install.crate_name)
                || plans.contains_key(&install.crate_name)
            {
                continue;
            }
            plans.insert(
                install.crate_name.clone(),
                ModuleInstallPlan {
                    workflow_id: dependency.workflow_id.clone(),
                    required_by: workflow.id.clone(),
                    options: install_to_add_dependency(install),
                },
            );
        }
    }
    Ok(plans.into_values().collect())
}

fn installed_dependency_names(root: &Path) -> CliResult<BTreeMap<String, ()>> {
    let manifest_path = root.join("Cargo.toml");
    if !manifest_path.exists() {
        return Ok(BTreeMap::new());
    }
    let source = fs::read_to_string(&manifest_path)?;
    let document = source
        .parse::<DocumentMut>()
        .map_err(|error| CliError::Usage(format!("invalid Cargo manifest: {error}")))?;
    let mut installed = BTreeMap::new();
    collect_dependency_names(document.get("dependencies"), &mut installed);
    collect_dependency_names(
        document
            .get("workspace")
            .and_then(|workspace| workspace.get("dependencies")),
        &mut installed,
    );
    Ok(installed)
}

fn collect_dependency_names(dependencies: Option<&Item>, installed: &mut BTreeMap<String, ()>) {
    let Some(dependencies) = dependencies.and_then(Item::as_table_like) else {
        return;
    };
    for (name, _dependency) in dependencies.iter() {
        installed.insert(name.to_owned(), ());
    }
}

fn install_to_add_dependency(install: &CargoDependency) -> AddDependencyOptions {
    AddDependencyOptions {
        crate_name: install.crate_name.clone(),
        source: match &install.source {
            Some(CargoDependencySource::Path(path)) => DependencySource::Path(path.clone()),
            Some(CargoDependencySource::Git(git)) => DependencySource::Git(git.clone()),
            None => DependencySource::Registry,
        },
        version: install.version.clone(),
        package: install.package.clone(),
        global: false,
        editable: false,
    }
}

fn module_install_json(module: &ModuleInstallPlan) -> serde_json::Value {
    json!({
        "workflow_id": module.workflow_id,
        "required_by": module.required_by,
        "dependency": module.options.crate_name,
        "version": module.options.version,
        "source": match &module.options.source {
            DependencySource::Registry => json!({ "registry": "crates.io" }),
            DependencySource::Path(path) => json!({ "path": path }),
            DependencySource::Git(git) => json!({ "git": git }),
        },
        "package": module.options.package,
        "editable": module.options.editable,
    })
}

struct SelectedModel<'a> {
    requirement_id: &'a str,
    variant: &'a ModelVariant,
}

struct SelectedCustomHfModel<'a> {
    requirement_id: &'a str,
    model: &'a CustomHfModel,
}

fn select_model_variants<'a>(
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

fn select_custom_hf_models<'a>(
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

fn model_variant_json(variant: &ModelVariant) -> serde_json::Value {
    json!({
        "id": variant.id,
        "provider": variant.provider.as_str(),
        "format": variant.format,
        "repo": variant.repo,
        "file": variant.file,
        "download_url": model_download_url(variant),
    })
}

fn hf_download_plan(selection: &SelectedModel<'_>) -> serde_json::Value {
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

fn custom_hf_download_plan(selection: &SelectedCustomHfModel<'_>) -> serde_json::Value {
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

fn available_variant_summary(model: &crate::workflow::ModelRequirement) -> String {
    if model.variants.is_empty() {
        return "none declared".to_owned();
    }
    model
        .variants
        .iter()
        .map(|variant| {
            format!(
                "{} ({}, {})",
                variant.id,
                variant.format,
                model_download_url(variant).unwrap_or_else(|| format!(
                    "{}:{}",
                    variant.provider.as_str(),
                    variant.repo
                ))
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn model_download_url(variant: &ModelVariant) -> Option<String> {
    match variant.provider {
        ModelProvider::HuggingFace => Some(hf_download_url(&variant.repo, variant.file.as_deref())),
    }
}

fn hf_download_url(repo: &str, file: Option<&str>) -> String {
    match file {
        Some(file) => format!(
            "https://huggingface.co/{repo}/resolve/main/{}",
            percent_encode_hf_path(file)
        ),
        None => format!("https://huggingface.co/{repo}"),
    }
}

fn percent_encode_hf_path(path: &str) -> String {
    let mut encoded = String::new();
    for byte in path.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => {
                encoded.push(byte as char)
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}
