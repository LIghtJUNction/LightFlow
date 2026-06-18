use super::project::workspace_manifest;
use super::{CliError, CliResult, required_flag_value};
use serde_json::json;
use std::fs;
use std::path::Path;
use toml_edit::{DocumentMut, InlineTable, Item, value};

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
            "--global" | "-g" => {
                global = true;
                index += 1;
            }
            "--editable" => {
                if editable {
                    return Err(CliError::Usage("duplicate flag --editable".to_owned()));
                }
                editable = true;
                index += 1;
            }
            "--version" => {
                if version.is_some() {
                    return Err(CliError::Usage("duplicate flag --version".to_owned()));
                }
                version = Some(required_flag_value(args, index, "--version")?.to_owned());
                index += 2;
            }
            "--path" => {
                ensure_single_dependency_source(&source)?;
                source = Some(DependencySource::Path(
                    required_flag_value(args, index, "--path")?.to_owned(),
                ));
                index += 2;
            }
            "--git" => {
                ensure_single_dependency_source(&source)?;
                source = Some(DependencySource::Git(
                    required_flag_value(args, index, "--git")?.to_owned(),
                ));
                index += 2;
            }
            "--package" => {
                if package.is_some() {
                    return Err(CliError::Usage("duplicate flag --package".to_owned()));
                }
                package = Some(required_flag_value(args, index, "--package")?.to_owned());
                index += 2;
            }
            value => {
                if crate_name.is_some() {
                    return Err(CliError::Usage(format!(
                        "unexpected argument for add: {value}"
                    )));
                }
                crate_name = Some(value.to_owned());
                index += 1;
            }
        }
    }
    let crate_name = crate_name.ok_or_else(|| CliError::Usage("missing crate name".to_owned()))?;
    let source = source.unwrap_or(DependencySource::Registry);
    if source == DependencySource::Registry && version.is_none() {
        return Err(CliError::Usage(
            "registry add requires --version <version>".to_owned(),
        ));
    }
    if editable && !matches!(source, DependencySource::Path(_)) {
        return Err(CliError::Usage(
            "editable add requires --path <path>".to_owned(),
        ));
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

fn ensure_single_dependency_source(source: &Option<DependencySource>) -> CliResult<()> {
    if source.is_some() {
        Err(CliError::Usage(
            "add accepts only one dependency source".to_owned(),
        ))
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
            super::project::workflow_collection_manifest()
        } else {
            workspace_manifest()
        };
        fs::write(&manifest_path, manifest)?;
    }
    let source = fs::read_to_string(&manifest_path)?;
    let mut document = source
        .parse::<DocumentMut>()
        .map_err(|error| CliError::Usage(format!("invalid Cargo manifest: {error}")))?;
    ensure_workspace_dependencies_table(&mut document);
    let dependency = dependency_item(options);
    document["workspace"]["dependencies"][&options.crate_name] = dependency;
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

fn ensure_workspace_dependencies_table(document: &mut DocumentMut) {
    if !document["workspace"].is_table() {
        document["workspace"] = Item::Table(toml_edit::Table::new());
    }
    if !document["workspace"]["dependencies"].is_table() {
        document["workspace"]["dependencies"] = Item::Table(toml_edit::Table::new());
    }
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
