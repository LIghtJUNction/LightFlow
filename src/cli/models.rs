use super::{CliError, CliResult};
use crate::api::{ApiService, ModelListOptions, ModelStatusFilter};
use std::process::{Command, Stdio};

const HF_CACHE_LIST_SCRIPT: &str = r#"
import json
from huggingface_hub import scan_cache_dir

info = scan_cache_dir()
repos = []
for repo in sorted(info.repos, key=lambda item: item.repo_id.lower()):
    repos.append({
        "repo_id": repo.repo_id,
        "repo_type": repo.repo_type,
        "cache_id": repo.cache_id,
        "path": str(repo.repo_path),
        "size_bytes": repo.size_on_disk,
        "size": repo.size_on_disk_str,
        "files": repo.nb_files,
        "revisions": len(repo.revisions),
        "last_accessed": repo.last_accessed_str,
        "last_modified": repo.last_modified_str,
    })
print(json.dumps({
    "cache_size_bytes": info.size_on_disk,
    "cache_size": info.size_on_disk_str,
    "repos": repos,
    "warnings": [str(warning) for warning in info.warnings],
}))
"#;

const HF_CACHE_DOWNLOAD_SCRIPT: &str = r#"
import json
import sys
from huggingface_hub import hf_hub_download, snapshot_download

repo_id = sys.argv[1]
filename = sys.argv[2] if len(sys.argv) > 2 else None
if filename:
    path = hf_hub_download(repo_id=repo_id, filename=filename)
else:
    path = snapshot_download(repo_id=repo_id)
print(json.dumps({"repo": repo_id, "file": filename, "path": path}))
"#;

const HF_CACHE_RM_SCRIPT: &str = r#"
import json
import shutil
import sys
from huggingface_hub import scan_cache_dir

target = sys.argv[1]
info = scan_cache_dir()
removed = []
for repo in info.repos:
    if target in (repo.repo_id, repo.cache_id, str(repo.repo_path)):
        removed.append({
            "repo_id": repo.repo_id,
            "cache_id": repo.cache_id,
            "path": str(repo.repo_path),
            "size_bytes": repo.size_on_disk,
            "size": repo.size_on_disk_str,
        })
        shutil.rmtree(repo.repo_path)
if not removed:
    print(f"cache entry not found: {target}", file=sys.stderr)
    raise SystemExit(2)
print(json.dumps({"removed": removed}))
"#;

const HF_CACHE_PRUNE_SCRIPT: &str = r#"
import json
from huggingface_hub import scan_cache_dir

info = scan_cache_dir()
detached = []
for repo in info.repos:
    for revision in repo.revisions:
        if not revision.refs:
            detached.append(revision.commit_hash)
strategy = info.delete_revisions(*detached)
strategy.execute()
print(json.dumps({
    "removed_revisions": detached,
    "expected_freed_size_bytes": strategy.expected_freed_size,
    "expected_freed_size": strategy.expected_freed_size_str,
}))
"#;

pub(super) fn manage_models(service: &ApiService, args: &[String]) -> CliResult<serde_json::Value> {
    let Some(command) = args.first().map(String::as_str) else {
        return Err(CliError::Usage(models_usage()));
    };
    match command {
        "list" | "ls" => {
            if args
                .get(1)
                .is_some_and(|arg| matches!(arg.as_str(), "-h" | "--help" | "help"))
            {
                return Err(CliError::Usage(models_usage()));
            }
            ensure_no_extra_args(args, 1, "models list")?;
            run_python_json(HF_CACHE_LIST_SCRIPT, &[], false)
        }
        "requirements" | "reqs" | "catalog" => {
            let filter = parse_requirements_filter(&args[1..])?;
            let catalog = service.list_models_with_options(&filter)?;
            serde_json::to_value(catalog).map_err(CliError::from)
        }
        "download" | "dl" => download_model(&args[1..]),
        "rm" | "remove" | "clean" => {
            if args
                .get(1)
                .is_some_and(|arg| matches!(arg.as_str(), "-h" | "--help" | "help"))
            {
                return Err(CliError::Usage(models_usage()));
            }
            let Some(target) = args.get(1).map(String::as_str) else {
                return Err(CliError::Usage(models_usage()));
            };
            ensure_no_extra_args(args, 2, "models rm")?;
            run_python_json(HF_CACHE_RM_SCRIPT, &[target], false)
        }
        "prune" => {
            if args
                .get(1)
                .is_some_and(|arg| matches!(arg.as_str(), "-h" | "--help" | "help"))
            {
                return Err(CliError::Usage(models_usage()));
            }
            ensure_no_extra_args(args, 1, "models prune")?;
            run_python_json(HF_CACHE_PRUNE_SCRIPT, &[], false)
        }
        "-h" | "--help" | "help" => Err(CliError::Usage(models_usage())),
        _ => Err(CliError::Usage(models_usage())),
    }
}

fn download_model(args: &[String]) -> CliResult<serde_json::Value> {
    if args
        .first()
        .is_some_and(|arg| matches!(arg.as_str(), "-h" | "--help" | "help"))
    {
        return Err(CliError::Usage(models_usage()));
    }
    let Some(target) = args.first().map(String::as_str) else {
        return Err(CliError::Usage(models_usage()));
    };
    let (repo, file, consumed) = if target.starts_with("https://huggingface.co/")
        || target.starts_with("http://huggingface.co/")
    {
        let (repo, file) = parse_hf_url(target)?;
        (repo, file, 1)
    } else {
        let file = args.get(1).map(String::as_str).map(str::to_owned);
        (
            target.to_owned(),
            file,
            if args.get(1).is_some() { 2 } else { 1 },
        )
    };
    ensure_no_extra_args(args, consumed, "models download")?;
    eprintln!(
        "Downloading Hugging Face model: {}",
        file.as_ref()
            .map(|file| format!("{repo}/{file}"))
            .unwrap_or_else(|| repo.clone())
    );
    let mut script_args = vec![repo.as_str()];
    if let Some(file) = file.as_deref() {
        script_args.push(file);
    }
    run_python_json(HF_CACHE_DOWNLOAD_SCRIPT, &script_args, true)
}

fn run_python_json(
    script: &str,
    args: &[&str],
    inherit_stderr: bool,
) -> CliResult<serde_json::Value> {
    let mut command = Command::new("python3");
    command.arg("-c").arg(script).args(args);
    if inherit_stderr {
        command.stderr(Stdio::inherit());
    } else {
        command.stderr(Stdio::piped());
    }
    let output = command.output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(CliError::Usage(format!(
            "python huggingface_hub command failed with status {}\n{}",
            output.status, stderr
        )));
    }
    serde_json::from_slice(&output.stdout).map_err(CliError::from)
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

fn ensure_no_extra_args(args: &[String], max_len: usize, _command: &str) -> CliResult<()> {
    if args.get(max_len).is_some() {
        return Err(CliError::Usage(models_usage()));
    }
    Ok(())
}

fn parse_requirements_filter(args: &[String]) -> CliResult<ModelListOptions> {
    let mut workflow_id = None;
    let mut status = ModelStatusFilter::All;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "-h" | "--help" | "help" => return Err(CliError::Usage(models_usage())),
            "--workflow" | "-w" => {
                index += 1;
                let Some(value) = args.get(index).filter(|value| !value.starts_with('-')) else {
                    return Err(CliError::Usage(models_usage()));
                };
                set_workflow_filter(&mut workflow_id, value)?;
            }
            "--status" => {
                index += 1;
                let Some(value) = args.get(index).filter(|value| !value.starts_with('-')) else {
                    return Err(CliError::Usage(models_usage()));
                };
                status = parse_model_status(value)?;
            }
            "--blocked" => status = ModelStatusFilter::Blocked,
            "--available" => status = ModelStatusFilter::Available,
            "--all" => status = ModelStatusFilter::All,
            arg if arg.starts_with('-') => {
                return Err(CliError::Usage(models_usage()));
            }
            arg => set_workflow_filter(&mut workflow_id, arg)?,
        }
        index += 1;
    }
    Ok(ModelListOptions {
        workflow_id,
        status,
    })
}

fn set_workflow_filter(workflow_id: &mut Option<String>, value: &str) -> CliResult<()> {
    if workflow_id.is_some() {
        return Err(CliError::Usage(format!(
            "unexpected argument for models requirements: {value}"
        )));
    }
    *workflow_id = Some(value.to_owned());
    Ok(())
}

fn parse_model_status(value: &str) -> CliResult<ModelStatusFilter> {
    ModelStatusFilter::parse(value).ok_or_else(|| {
        CliError::Usage(format!(
            "unsupported model status {value}; expected all, available, or blocked"
        ))
    })
}

fn models_usage() -> String {
    [
        "usage:",
        "  lfw models list",
        "  lfw models requirements [workflow_id|--workflow <workflow_id>] [--status all|available|blocked]",
        "  lfw models requirements [workflow_id] --blocked",
        "  lfw models download <repo> [file]",
        "  lfw models download <huggingface-file-url>",
        "  lfw models rm <repo|cache_id|path>",
        "  lfw models prune",
        "",
        "Inspects workflow model requirements and manages the local Hugging Face cache.",
        "Use requirements before running model-backed workflows to find missing locks,",
        "blocked requirements, and the sync/verify commands needed to make them runnable.",
    ]
    .join("\n")
}
