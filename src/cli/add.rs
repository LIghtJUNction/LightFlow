use super::project::{workflow_host_package_name, workflow_host_source, workspace_manifest};
use super::{CliError, CliResult};
use serde_json::json;
use std::fs;
use std::path::Path;
use toml_edit::{DocumentMut, InlineTable, Item, Table, value};

#[derive(Debug, Clone, Eq, PartialEq)]
pub(super) struct AddDependencyOptions {
    pub(super) crate_name: String,
    pub(super) source: DependencySource,
    pub(super) version: Option<String>,
    pub(super) package: Option<String>,
    pub(super) global: bool,
    pub(super) editable: bool,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(super) enum DependencySource {
    Registry,
    Path(String),
    Git(String),
}

pub(super) fn parse_add_dependency_options(args: &[String]) -> CliResult<AddDependencyOptions> {
    let mut crate_name = None;
    let mut source = None;
    let mut version = None;
    let mut package = None;
    let mut global = false;
    let mut editable = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "-h" | "--help" | "help" => return Err(CliError::Usage(add_usage())),
            "--global" | "-g" => {
                global = true;
                index += 1;
            }
            "--editable" => {
                if editable {
                    return Err(CliError::Usage(add_usage()));
                }
                editable = true;
                index += 1;
            }
            "--version" => {
                if version.is_some() {
                    return Err(CliError::Usage(add_usage()));
                }
                version = Some(required_add_flag_value(args, index)?.to_owned());
                index += 2;
            }
            "--path" => {
                ensure_single_dependency_source(&source)?;
                source = Some(DependencySource::Path(
                    required_add_flag_value(args, index)?.to_owned(),
                ));
                index += 2;
            }
            "--git" => {
                ensure_single_dependency_source(&source)?;
                source = Some(DependencySource::Git(
                    required_add_flag_value(args, index)?.to_owned(),
                ));
                index += 2;
            }
            "--package" => {
                if package.is_some() {
                    return Err(CliError::Usage(add_usage()));
                }
                package = Some(required_add_flag_value(args, index)?.to_owned());
                index += 2;
            }
            value if value.starts_with('-') => {
                return Err(CliError::Usage(add_usage()));
            }
            value => {
                if crate_name.is_some() {
                    return Err(CliError::Usage(add_usage()));
                }
                crate_name = Some(value.to_owned());
                index += 1;
            }
        }
    }
    let Some(crate_name) = crate_name else {
        return Err(CliError::Usage(add_usage()));
    };
    let source = source.unwrap_or(DependencySource::Registry);
    if editable && !matches!(source, DependencySource::Path(_)) {
        return Err(CliError::Usage(add_usage()));
    }
    if source == DependencySource::Registry && version.is_none() {
        return Err(CliError::Usage(add_usage()));
    }
    Ok(AddDependencyOptions {
        crate_name,
        source,
        version,
        package,
        global,
        editable,
    })
}

fn required_add_flag_value(args: &[String], index: usize) -> CliResult<&str> {
    let Some(value) = args.get(index + 1).map(String::as_str) else {
        return Err(CliError::Usage(add_usage()));
    };
    if value.starts_with("--") {
        return Err(CliError::Usage(add_usage()));
    }
    Ok(value)
}

fn add_usage() -> String {
    [
        "usage:",
        "  lfw add <crate_name> [--version <version>] [--path <path>|--git <url>] [--package <package>] [--editable] [--global|-g]",
        "",
        "Adds one workflow crate dependency to the local or global LightFlow host package.",
        "Registry dependencies require --version <version>.",
        "Use --path for local workflow crates and --editable when the path should remain editable.",
        "Use --git for remote workflow crates; --package selects a package when the repository name differs.",
        "Use lfw import when a repository contains multiple workflow crates.",
    ]
    .join("\n")
}

fn ensure_single_dependency_source(source: &Option<DependencySource>) -> CliResult<()> {
    if source.is_some() {
        Err(CliError::Usage(add_usage()))
    } else {
        Ok(())
    }
}

pub(super) fn add_dependency(
    root: &Path,
    options: &AddDependencyOptions,
    global: bool,
) -> CliResult<serde_json::Value> {
    let manifest_path = root.join("Cargo.toml");
    if !manifest_path.exists() {
        let manifest = if global {
            super::project::workflow_collection_manifest(root)
        } else {
            workspace_manifest(root)
        };
        fs::write(&manifest_path, manifest)?;
        write_workflow_host_source(root)?;
    }
    let source = fs::read_to_string(&manifest_path)?;
    let mut document = source
        .parse::<DocumentMut>()
        .map_err(|error| CliError::Usage(format!("invalid Cargo manifest: {error}")))?;
    if document.get("package").is_none() && document.get("workspace").is_some() {
        ensure_workflow_host_package(&mut document, root);
        write_workflow_host_source(root)?;
    }
    ensure_dependencies_table(&mut document);
    let dependency = dependency_item(options);
    document["dependencies"][&options.crate_name] = dependency;
    fs::write(&manifest_path, document.to_string())?;
    Ok(json!({
        "manifest": manifest_path,
        "dependency": options.crate_name,
        "source": match &options.source {
            DependencySource::Registry => json!({ "registry": "crates.io" }),
            DependencySource::Path(path) => json!({ "path": path }),
            DependencySource::Git(git) => json!({ "git": git }),
        },
        "version": options.version,
        "package": options.package,
        "global": global,
        "editable": options.editable,
    }))
}

fn ensure_dependencies_table(document: &mut DocumentMut) {
    let root = document.as_table_mut();
    let needs_dependencies = root
        .get("dependencies")
        .is_none_or(|dependencies| !dependencies.is_table());
    if needs_dependencies {
        root["dependencies"] = Item::Table(Table::new());
    }
}

fn ensure_workflow_host_package(document: &mut DocumentMut, root: &Path) {
    let inherited_dependencies = document
        .get("workspace")
        .and_then(|workspace| workspace.get("dependencies"))
        .and_then(Item::as_table_like)
        .map(|dependencies| {
            dependencies
                .iter()
                .filter(|(name, dependency)| !is_core_lightflow_dependency(name, dependency))
                .map(|(name, dependency)| (name.to_owned(), dependency.clone()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    document["package"] = Item::Table(Table::new());
    document["package"]["name"] = value(workflow_host_package_name(root));
    document["package"]["version"] = value("0.0.0");
    document["package"]["edition"] = value("2024");
    document["package"]["publish"] = value(false);
    document["lib"] = Item::Table(Table::new());
    document["lib"]["path"] = value(".lightflow/workspace.rs");
    ensure_dependencies_table(document);
    for (name, dependency) in &inherited_dependencies {
        document["dependencies"][name] = dependency.clone();
    }
}

fn is_core_lightflow_dependency(name: &str, dependency: &Item) -> bool {
    name == "lightflow" || dependency.get("package").and_then(Item::as_str) == Some("lightflow")
}

fn write_workflow_host_source(root: &Path) -> CliResult<()> {
    let path = root.join(".lightflow/workspace.rs");
    if path.exists() {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, workflow_host_source())?;
    Ok(())
}

fn dependency_item(options: &AddDependencyOptions) -> Item {
    let mut table = InlineTable::new();
    if let Some(version) = &options.version {
        table.insert("version", value(version).into_value().unwrap());
    }
    match &options.source {
        DependencySource::Registry => {}
        DependencySource::Path(path) => {
            table.insert("path", value(path).into_value().unwrap());
        }
        DependencySource::Git(git) => {
            table.insert("git", value(git).into_value().unwrap());
        }
    }
    if let Some(package) = &options.package {
        table.insert("package", value(package).into_value().unwrap());
    }
    Item::Value(toml_edit::Value::InlineTable(table))
}
