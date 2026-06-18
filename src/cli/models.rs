use super::{CliError, CliResult};
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

pub(super) fn manage_models(args: &[String]) -> CliResult<serde_json::Value> {
    let Some(command) = args.first().map(String::as_str) else {
        return Err(CliError::Usage(models_usage()));
    };
    match command {
        "list" | "ls" => {
            ensure_no_extra_args(args, 1, "models list")?;
            run_python_json(HF_CACHE_LIST_SCRIPT, &[], false)
        }
        "download" | "dl" => download_model(&args[1..]),
        "rm" | "remove" | "clean" => {
            let target = required_arg(args, 1, "cache entry")?;
            ensure_no_extra_args(args, 2, "models rm")?;
            run_python_json(HF_CACHE_RM_SCRIPT, &[target], false)
        }
        "prune" => {
            ensure_no_extra_args(args, 1, "models prune")?;
            run_python_json(HF_CACHE_PRUNE_SCRIPT, &[], false)
        }
        "-h" | "--help" | "help" => Err(CliError::Usage(models_usage())),
        _ => Err(CliError::Usage(models_usage())),
    }
}

fn download_model(args: &[String]) -> CliResult<serde_json::Value> {
    let target = required_arg(args, 0, "repo or Hugging Face URL")?;
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

fn required_arg<'a>(args: &'a [String], index: usize, label: &str) -> CliResult<&'a str> {
    args.get(index)
        .map(String::as_str)
        .ok_or_else(|| CliError::Usage(format!("missing {label}")))
}

fn ensure_no_extra_args(args: &[String], max_len: usize, command: &str) -> CliResult<()> {
    if let Some(extra) = args.get(max_len) {
        return Err(CliError::Usage(format!(
            "unexpected argument for {command}: {extra}"
        )));
    }
    Ok(())
}

fn models_usage() -> String {
    [
        "usage:",
        "  lfw models list",
        "  lfw models download <repo> [file]",
        "  lfw models download <huggingface-file-url>",
        "  lfw models rm <repo|cache_id|path>",
        "  lfw models prune",
    ]
    .join("\n")
}
