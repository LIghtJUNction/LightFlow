use crate::api::read_workflow_source;
use crate::cli::CliResult;
use std::fs;
use std::path::{Path, PathBuf};

pub(super) fn find_workflow_crate_dir(
    root: &Path,
    workflow_id: &str,
) -> CliResult<Option<PathBuf>> {
    let mut collections = vec![
        root.join(".lightflow").join("workflows"),
        root.join("workflows"),
        root.join("lightflow").join("workflows"),
    ];
    let projects = root.join("projects");
    let mut project_roots = read_sorted_dirs(&projects)?;
    for preferred in preferred_project_names(workflow_id) {
        let project = projects.join(&preferred);
        if project.is_dir() {
            collections.push(project.join(".lightflow").join("workflows"));
            collections.push(project.join("workflows"));
            project_roots
                .retain(|root| root.file_name().and_then(|name| name.to_str()) != Some(&preferred));
        }
    }
    for project in project_roots {
        collections.push(project.join(".lightflow").join("workflows"));
        collections.push(project.join("workflows"));
    }

    for collection in collections {
        for crate_dir in workflow_crates_in_collection(&collection)? {
            let lib = crate_dir.join("src").join("lib.rs");
            let Ok(workflow) = read_workflow_source(&lib) else {
                continue;
            };
            if workflow.id == workflow_id {
                return Ok(Some(crate_dir));
            }
        }
    }

    Ok(None)
}

fn preferred_project_names(workflow_id: &str) -> Vec<String> {
    let mut names = Vec::new();
    if workflow_id == "lightflow.text_prompt" || workflow_id.starts_with("lightflow.") {
        names.push("lightflow-std".to_owned());
    }
    if workflow_id.starts_with("lightflow.flux_") {
        names.insert(0, "lightflow-flux".to_owned());
    }
    if workflow_id.starts_with("lightflow.rig_") {
        names.insert(0, "lightflow-rig".to_owned());
    }
    names.dedup();
    names
}

fn workflow_crates_in_collection(collection: &Path) -> CliResult<Vec<PathBuf>> {
    let mut crates = Vec::new();
    for crate_dir in read_sorted_dirs(collection)? {
        if !crate_dir.is_dir() {
            continue;
        }
        if crate_dir.join("Cargo.toml").is_file() && crate_dir.join("src").join("lib.rs").is_file()
        {
            crates.push(crate_dir);
        }
    }
    crates.sort();
    Ok(crates)
}

fn read_sorted_dirs(path: &Path) -> CliResult<Vec<PathBuf>> {
    let Ok(entries) = fs::read_dir(path) else {
        return Ok(Vec::new());
    };
    let mut paths = entries
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<Vec<_>, _>>()?;
    paths.sort();
    Ok(paths)
}

#[cfg(test)]
mod tests {
    use super::preferred_project_names;

    #[test]
    fn flux_workflow_prefers_flux_project() {
        assert_eq!(
            preferred_project_names("lightflow.flux_text_to_image")[0],
            "lightflow-flux"
        );
    }

    #[test]
    fn rig_workflow_prefers_rig_project() {
        assert_eq!(
            preferred_project_names("lightflow.rig_llm")[0],
            "lightflow-rig"
        );
    }
}
