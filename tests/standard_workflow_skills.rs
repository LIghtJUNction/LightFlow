use std::fs;
use std::path::{Path, PathBuf};

#[test]
fn repository_workflow_crates_have_agent_skills() -> Result<(), Box<dyn std::error::Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut workflow_crates = Vec::new();
    collect_workflow_crates(root, root, &mut workflow_crates)?;
    workflow_crates.sort();

    assert!(
        !workflow_crates.is_empty(),
        "repository should contain workflow crates"
    );

    let mut missing = Vec::new();
    for crate_dir in workflow_crates {
        let source = fs::read_to_string(crate_dir.join("src/lib.rs"))?;
        let workflow_id = workflow_id_from_source(&source).unwrap_or_else(|| {
            panic!(
                "workflow crate {} should declare workflow(\"...\")",
                crate_dir.display()
            )
        });
        let skill_root = crate_dir.join(".agent/skills");
        let mut found = false;
        if let Ok(entries) = fs::read_dir(&skill_root) {
            for entry in entries.flatten() {
                let skill_path = entry.path().join("SKILL.md");
                let Ok(skill) = fs::read_to_string(&skill_path) else {
                    continue;
                };
                if skill_has_frontmatter(&skill)
                    && skill.contains(&workflow_id)
                    && skill.contains("lfw run")
                    && skill.contains(&format!("/workflows/{workflow_id}/run"))
                {
                    found = true;
                    break;
                }
            }
        }
        if !found {
            missing.push(format!("{} ({workflow_id})", crate_dir.display()));
        }
    }

    assert!(
        missing.is_empty(),
        "workflow crates missing agent skills with CLI and API examples:\n{}",
        missing.join("\n")
    );
    Ok(())
}

fn skill_has_frontmatter(source: &str) -> bool {
    let mut lines = source.lines();
    if lines.next() != Some("---") {
        return false;
    }
    let mut has_name = false;
    let mut has_description = false;
    let mut has_version = false;
    for line in lines {
        if line == "---" {
            return has_name && has_description && has_version;
        }
        has_name |= line.starts_with("name:");
        has_description |= line.starts_with("description:");
        has_version |= line.starts_with("version:");
    }
    false
}

fn collect_workflow_crates(
    root: &Path,
    current: &Path,
    crates: &mut Vec<PathBuf>,
) -> Result<(), Box<dyn std::error::Error>> {
    for entry in fs::read_dir(current)? {
        let entry = entry?;
        if entry.file_type()?.is_symlink() {
            continue;
        }
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let relative = path.strip_prefix(root)?;
        if relative
            .components()
            .any(|component| matches!(component.as_os_str().to_str(), Some("target" | "vendor")))
        {
            continue;
        }
        if path.join("Cargo.toml").exists()
            && path.join("src/lib.rs").exists()
            && relative
                .components()
                .any(|component| matches!(component.as_os_str().to_str(), Some("workflows")))
        {
            crates.push(path);
            continue;
        }
        collect_workflow_crates(root, &path, crates)?;
    }
    Ok(())
}

fn workflow_id_from_source(source: &str) -> Option<String> {
    let start = source.find("workflow(\"")? + "workflow(\"".len();
    let end = source[start..].find('"')?;
    Some(source[start..start + end].to_owned())
}
