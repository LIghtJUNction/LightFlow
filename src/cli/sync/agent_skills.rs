use super::lockfile::{SkillLockEntry, read_lfw_lock_optional, write_lfw_lock_file};
use crate::cli::CliResult;
use serde_json::json;
use std::path::{Path, PathBuf};

mod discovery;
mod install;

use install::{global_agent_skill_dir, install_agent_skill, prompt_agent_skill_install};

#[derive(Debug, Clone, Eq, PartialEq)]
pub(super) struct AgentSkillCandidate {
    pub(super) name: String,
    pub(super) source_dir: PathBuf,
}

pub(super) fn discover_agent_skills(root: &Path) -> CliResult<Vec<AgentSkillCandidate>> {
    discovery::discover_agent_skills(root)
}

fn normalize_skill_path(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

pub(super) fn plan_agent_skills(
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

pub(super) fn sync_agent_skills(
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

fn agent_skill_json(candidate: &AgentSkillCandidate) -> serde_json::Value {
    json!({
        "name": candidate.name,
        "source": candidate.source_dir,
    })
}

fn skill_lock_key(name: &str, source_dir: &Path) -> String {
    format!("{name}::{}", source_dir.display())
}
