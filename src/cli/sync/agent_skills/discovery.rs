use super::{AgentSkillCandidate, normalize_skill_path, skill_lock_key};
use crate::cli::CliResult;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

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
    let Ok(workflows) = fs::read_dir(collection) else {
        return Ok(());
    };
    for workflow in workflows {
        let workflow = workflow?.path();
        if workflow.is_dir() {
            collect_agent_skills_from(&workflow.join(".agent").join("skills"), skills)?;
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
