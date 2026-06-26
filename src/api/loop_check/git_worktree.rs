use std::path::{Path, PathBuf};
use std::process::Command;

pub(super) fn git_changed_paths(root: &Path) -> Result<Vec<PathBuf>, String> {
    let output = Command::new("git")
        .args(["-C"])
        .arg(root)
        .args(["status", "--porcelain=v1", "-z", "--untracked-files=all"])
        .output()
        .map_err(|error| format!("git status failed: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "git status failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let mut paths = Vec::new();
    let mut fields = output.stdout.split(|byte| *byte == 0);
    while let Some(field) = fields.next() {
        if field.is_empty() || field.len() < 4 {
            continue;
        }
        let status = &field[..2];
        let path = PathBuf::from(String::from_utf8_lossy(&field[3..]).to_string());
        paths.push(path);
        if matches!(status, b"R " | b" R" | b"RR" | b"C " | b" C") {
            let _ = fields.next();
        }
    }
    Ok(paths)
}

pub(super) fn git_current_branch(root: &Path) -> Result<String, String> {
    git_output(root, ["rev-parse", "--abbrev-ref", "HEAD"])
}

pub(super) fn git_current_upstream(root: &Path) -> Result<String, String> {
    git_output(
        root,
        ["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"],
    )
}

pub(super) fn git_origin_remote_url(root: &Path) -> Result<String, String> {
    git_output(root, ["remote", "get-url", "origin"])
}

pub(super) fn git_short_head(root: &Path) -> Result<String, String> {
    git_output(root, ["rev-parse", "--short", "HEAD"])
}

pub(super) fn git_full_head(root: &Path) -> Result<String, String> {
    git_output(root, ["rev-parse", "HEAD"])
}

pub(super) fn parent_gitlink_full_head(root: &Path, path: &Path) -> Result<Option<String>, String> {
    let output = Command::new("git")
        .args(["-C"])
        .arg(root)
        .args(["ls-files", "-s", "--"])
        .arg(path)
        .output()
        .map_err(|error| format!("git ls-files failed: {error}"))?;
    if !output.status.success() {
        return Ok(None);
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let mut parts = text.split_whitespace();
    let Some(mode) = parts.next() else {
        return Ok(None);
    };
    let Some(hash) = parts.next() else {
        return Ok(None);
    };
    if mode == "160000" {
        Ok(Some(hash.to_owned()))
    } else {
        Ok(None)
    }
}

pub(super) fn short_commit(commit: &str) -> String {
    commit.chars().take(7).collect()
}

fn git_output<const N: usize>(root: &Path, args: [&str; N]) -> Result<String, String> {
    let output = Command::new("git")
        .args(["-C"])
        .arg(root)
        .args(args)
        .output()
        .map_err(|error| format!("git command failed: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "git command failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}
