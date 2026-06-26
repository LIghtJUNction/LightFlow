use super::super::add::{AddDependencyOptions, DependencySource};
use crate::cli::{CliError, CliResult};
use crate::workflow::{CargoDependency, CargoDependencySource, WorkflowSpec};
use serde_json::json;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use toml_edit::{DocumentMut, Item};

#[derive(Debug, Clone, Eq, PartialEq)]
pub(super) struct ModuleInstallPlan {
    workflow_id: String,
    required_by: String,
    pub(super) options: AddDependencyOptions,
}

pub(super) fn module_install_plans(
    root: &Path,
    workflows: &[WorkflowSpec],
) -> CliResult<Vec<ModuleInstallPlan>> {
    let installed = installed_dependency_names(root)?;
    let mut plans = BTreeMap::<String, ModuleInstallPlan>::new();
    for workflow in workflows {
        for dependency in &workflow.dependencies {
            let Some(install) = &dependency.install else {
                continue;
            };
            if installed.contains_key(&install.crate_name)
                || plans.contains_key(&install.crate_name)
            {
                continue;
            }
            plans.insert(
                install.crate_name.clone(),
                ModuleInstallPlan {
                    workflow_id: dependency.workflow_id.clone(),
                    required_by: workflow.id.clone(),
                    options: install_to_add_dependency(install),
                },
            );
        }
    }
    Ok(plans.into_values().collect())
}

fn installed_dependency_names(root: &Path) -> CliResult<BTreeMap<String, ()>> {
    let manifest_path = root.join("Cargo.toml");
    if !manifest_path.exists() {
        return Ok(BTreeMap::new());
    }
    let source = fs::read_to_string(&manifest_path)?;
    let document = source
        .parse::<DocumentMut>()
        .map_err(|error| CliError::Usage(format!("invalid Cargo manifest: {error}")))?;
    let mut installed = BTreeMap::new();
    collect_dependency_names(document.get("dependencies"), &mut installed);
    collect_dependency_names(
        document
            .get("workspace")
            .and_then(|workspace| workspace.get("dependencies")),
        &mut installed,
    );
    Ok(installed)
}

fn collect_dependency_names(dependencies: Option<&Item>, installed: &mut BTreeMap<String, ()>) {
    let Some(dependencies) = dependencies.and_then(Item::as_table_like) else {
        return;
    };
    for (name, _dependency) in dependencies.iter() {
        installed.insert(name.to_owned(), ());
    }
}

fn install_to_add_dependency(install: &CargoDependency) -> AddDependencyOptions {
    AddDependencyOptions {
        crate_name: install.crate_name.clone(),
        source: match &install.source {
            Some(CargoDependencySource::Path(path)) => DependencySource::Path(path.clone()),
            Some(CargoDependencySource::Git(git)) => DependencySource::Git(git.clone()),
            None => DependencySource::Registry,
        },
        version: install.version.clone(),
        package: install.package.clone(),
        global: false,
        editable: false,
    }
}

pub(super) fn module_install_json(module: &ModuleInstallPlan) -> serde_json::Value {
    json!({
        "workflow_id": module.workflow_id,
        "required_by": module.required_by,
        "dependency": module.options.crate_name,
        "version": module.options.version,
        "source": match &module.options.source {
            DependencySource::Registry => json!({ "registry": "crates.io" }),
            DependencySource::Path(path) => json!({ "path": path }),
            DependencySource::Git(git) => json!({ "git": git }),
        },
        "package": module.options.package,
        "editable": module.options.editable,
    })
}
