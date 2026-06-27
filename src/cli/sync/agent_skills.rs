use super::lockfile::{SkillLockEntry, read_lfw_lock_optional, write_lfw_lock_file};
use crate::cli::{CliError, CliResult};
use serde_json::json;
use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Eq, PartialEq)]
pub(super) struct AgentSkillCandidate {
    name: String,
    source_dir: PathBuf,
}

pub(super) fn discover_agent_skills(root: &Path) -> CliResult<Vec<AgentSkillCandidate>> {
    let mut skills = BTreeMap::<String, AgentSkillCandidate>::new();
    collect_agent_skill_roots(root, &mut skills)?;
    collect_workflow_collection_agent_skills(
        &root.join(".lightflow").join("workflows"),
        &mut skills,
    )?;
    collect_workflow_collection_agent_skills(&root.join("workflows"), &mut skills)?;
    collect_workflow_collection_agent_skills(
        &root.join("lightflow").join("workflows"),
        &mut skills,
    )?;
    if let Ok(lfw_path) = std::env::var("LFW_PATH") {
        for path in std::env::split_paths(&lfw_path) {
            collect_agent_skill_roots(&path, &mut skills)?;
            collect_workflow_collection_agent_skills(&path, &mut skills)?;
            collect_workflow_collection_agent_skills(
                &path.join(".lightflow").join("workflows"),
                &mut skills,
            )?;
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
