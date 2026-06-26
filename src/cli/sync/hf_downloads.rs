use super::cache_metadata::{
    extract_hf_paths, extract_hf_paths_from_text, file_size, hf_snapshot_revision, sha256_file,
};
use crate::cli::{CliError, CliResult};
use serde_json::json;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;

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

pub(super) fn execute_hf_downloads_parallel(
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
