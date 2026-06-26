use super::{ApiError, ApiResult};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use toml_edit::{DocumentMut, Item};

pub(super) const PROJECT_WORKSPACE_CONFIG: &str = "lightflow-projects.toml";

pub(super) fn project_workspace_config_path(root: &Path) -> PathBuf {
    root.join("projects").join(PROJECT_WORKSPACE_CONFIG)
}

pub(super) fn expected_project_workspace_names(root: &Path) -> ApiResult<Vec<String>> {
    configured_project_names(root, "workspaces", "expected")?
        .map_or_else(|| Ok(default_expected_project_workspace_names()), Ok)
}

pub(super) fn optional_project_workspace_names(root: &Path) -> ApiResult<Vec<String>> {
    configured_project_names(root, "workspaces", "optional")?
        .map_or_else(|| Ok(default_optional_project_workspace_names()), Ok)
}

pub(super) fn default_project_workflow_sources(root: &Path) -> ApiResult<Vec<String>> {
    configured_project_names(root, "workflows", "default_sources")?
        .map_or_else(|| Ok(default_project_workflow_source_names()), Ok)
}

fn configured_project_names(
    root: &Path,
    table_name: &str,
    key: &str,
) -> ApiResult<Option<Vec<String>>> {
    let path = project_workspace_config_path(root);
    if !path.exists() {
        return Ok(None);
    }

    let source = fs::read_to_string(&path).map_err(ApiError::Io)?;
    let document = source.parse::<DocumentMut>().map_err(|error| {
        ApiError::InvalidRequest(format!("{} is not valid TOML: {error}", path.display()))
    })?;
    let Some(table) = document.get(table_name).and_then(Item::as_table_like) else {
        return Ok(None);
    };
    let Some(item) = table.get(key) else {
        return Ok(None);
    };
    let Some(array) = item.as_array() else {
        return Err(ApiError::InvalidRequest(format!(
            "{} [{table_name}].{key} must be an array of strings",
            path.display()
        )));
    };

    let mut names = Vec::new();
    let mut seen = BTreeSet::new();
    for value in array {
        let Some(name) = value.as_str() else {
            return Err(ApiError::InvalidRequest(format!(
                "{} [{table_name}].{key} must contain only strings",
                path.display()
            )));
        };
        validate_project_workspace_name(&path, table_name, key, name)?;
        if seen.insert(name.to_owned()) {
            names.push(name.to_owned());
        }
    }
    Ok(Some(names))
}

fn validate_project_workspace_name(
    path: &Path,
    table_name: &str,
    key: &str,
    name: &str,
) -> ApiResult<()> {
    if name.trim() != name {
        return Err(ApiError::InvalidRequest(format!(
            "{} [{table_name}].{key} entries must not have leading or trailing whitespace: {name:?}",
            path.display()
        )));
    }
    if name.is_empty() {
        return Err(ApiError::InvalidRequest(format!(
            "{} [{table_name}].{key} must not contain empty names",
            path.display()
        )));
    }
    if name == "." || name == ".." || name.contains('/') || name.contains('\\') {
        return Err(ApiError::InvalidRequest(format!(
            "{} [{table_name}].{key} entries must be project directory names under projects/, not paths: {name:?}",
            path.display()
        )));
    }
    Ok(())
}

pub(super) fn default_expected_project_workspace_names() -> Vec<String> {
    ["lightflow-flux", "lightflow-std", "lightflow-rig"]
        .into_iter()
        .map(ToOwned::to_owned)
        .collect()
}

pub(super) fn default_optional_project_workspace_names() -> Vec<String> {
    Vec::new()
}

pub(super) fn default_project_workflow_source_names() -> Vec<String> {
    ["lightflow-std"]
        .into_iter()
        .map(ToOwned::to_owned)
        .collect()
}

pub(super) fn project_submodule_update_command<'a>(
    names: impl IntoIterator<Item = &'a str>,
) -> Vec<String> {
    let mut unique_names = BTreeSet::new();
    unique_names.extend(names);
    let mut command = vec![
        "git".to_owned(),
        "submodule".to_owned(),
        "update".to_owned(),
        "--init".to_owned(),
        "--recursive".to_owned(),
    ];
    command.extend(
        unique_names
            .into_iter()
            .map(|name| format!("projects/{name}")),
    );
    command
}

pub(super) fn project_config_template_command() -> Vec<String> {
    vec![
        "lfw".to_owned(),
        "dev".to_owned(),
        "project-config-template".to_owned(),
    ]
}

pub(super) fn project_config_write_command() -> Vec<String> {
    let mut command = project_config_template_command();
    command.push("--write".to_owned());
    command
}

#[cfg(test)]
mod tests {
    use super::{
        project_config_template_command, project_config_write_command,
        project_submodule_update_command,
    };

    #[test]
    fn project_setup_commands_are_stable_and_deduped() {
        assert_eq!(
            project_config_template_command(),
            vec!["lfw", "dev", "project-config-template"]
        );
        assert_eq!(
            project_config_write_command(),
            vec!["lfw", "dev", "project-config-template", "--write"]
        );
        assert_eq!(
            project_submodule_update_command([
                "lightflow-std",
                "lightflow-flux",
                "lightflow-std",
                "lightflow-rig",
            ]),
            vec![
                "git",
                "submodule",
                "update",
                "--init",
                "--recursive",
                "projects/lightflow-flux",
                "projects/lightflow-rig",
                "projects/lightflow-std",
            ]
        );
    }
}
