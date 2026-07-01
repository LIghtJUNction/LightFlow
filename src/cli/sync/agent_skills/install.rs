use super::{AgentSkillCandidate, AgentSkillChoice, normalize_skill_path};
use crate::cli::{CliError, CliResult};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

pub(super) fn prompt_agent_skill_install(
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

pub(super) fn install_agent_skill(
    candidate: &AgentSkillCandidate,
    target: &Path,
) -> CliResult<PathBuf> {
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

pub(super) fn global_agent_skill_dir() -> CliResult<PathBuf> {
    let home = std::env::var_os("HOME")
        .ok_or_else(|| CliError::Usage("HOME is required for global agent skills".to_owned()))?;
    Ok(PathBuf::from(home).join(".agents").join("skills"))
}
