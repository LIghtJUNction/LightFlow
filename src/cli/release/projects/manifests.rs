use crate::api::resolve_workspace_member_manifests;
use crate::cli::{CliError, CliResult};
use serde::Serialize;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use toml_edit::{DocumentMut, Item, Table, Value, value};

#[derive(Debug, Serialize)]
pub(super) struct ManifestChange {
    pub(super) manifest: PathBuf,
    pub(super) package: Option<String>,
    pub(super) old_version: Option<String>,
    pub(super) new_version: Option<String>,
    pub(super) changed: bool,
}

#[derive(Debug, Clone, Serialize)]
pub(super) struct DependencyUpdate {
    pub(super) manifest: PathBuf,
    pub(super) section: String,
    pub(super) dependency: String,
    pub(super) package: String,
    pub(super) old_version: Option<String>,
    pub(super) new_version: String,
}

pub(super) struct PlannedManifest {
    pub(super) path: PathBuf,
    pub(super) original: String,
    pub(super) rendered: String,
    pub(super) change: ManifestChange,
    pub(super) dependency_updates: Vec<DependencyUpdate>,
}

pub(super) fn required_package_name(path: &Path) -> CliResult<String> {
    let document = read_manifest(path)?;
    package_string(&document, path, "name")
}

pub(super) fn validate_workspace_manifest(path: &Path) -> CliResult<Option<String>> {
    let document = read_manifest(path)?;
    if document.get("package").is_some() {
        let package = package_string(&document, path, "name")?;
        package_string(&document, path, "version")?;
        return Ok(Some(package));
    }
    Ok(None)
}

pub(super) fn publishable_workspace_member_manifests(
    workspace_manifest: &Path,
) -> CliResult<Vec<PathBuf>> {
    workspace_member_manifests(workspace_manifest).and_then(|manifests| {
        manifests
            .into_iter()
            .filter_map(|manifest| match read_manifest(&manifest) {
                Ok(document) if package_is_publishable(&document) => Some(Ok(manifest)),
                Ok(_) => None,
                Err(error) => Some(Err(error)),
            })
            .collect()
    })
}

pub(super) fn workspace_member_manifests(workspace_manifest: &Path) -> CliResult<Vec<PathBuf>> {
    let document = read_manifest(workspace_manifest)?;
    resolve_workspace_member_manifests(workspace_manifest, &document).map_err(CliError::Usage)
}

fn package_is_publishable(document: &DocumentMut) -> bool {
    let Some(package) = document.get("package").and_then(Item::as_table_like) else {
        return false;
    };
    match package.get("publish") {
        Some(item) if item.as_bool() == Some(false) => false,
        Some(item)
            if item
                .as_array()
                .is_some_and(|registries| registries.is_empty()) =>
        {
            false
        }
        _ => true,
    }
}

pub(super) fn read_manifest(path: &Path) -> CliResult<DocumentMut> {
    let source = fs::read_to_string(path)?;
    source.parse::<DocumentMut>().map_err(|error| {
        CliError::Usage(format!(
            "invalid Cargo manifest {}: {error}",
            path.display()
        ))
    })
}

fn package_string(document: &DocumentMut, path: &Path, field: &str) -> CliResult<String> {
    document
        .get("package")
        .and_then(Item::as_table_like)
        .and_then(|package| package.get(field))
        .and_then(Item::as_str)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| {
            CliError::Usage(format!(
                "{} has no non-empty package.{field}",
                path.display()
            ))
        })
}

pub(super) fn plan_manifest(
    root: &Path,
    path: &Path,
    train_packages: &BTreeSet<String>,
    version: &str,
) -> CliResult<PlannedManifest> {
    let original = fs::read_to_string(path)?;
    let mut document = original.parse::<DocumentMut>().map_err(|error| {
        CliError::Usage(format!(
            "invalid Cargo manifest {}: {error}",
            path.display()
        ))
    })?;
    let package = package_value(&document, "name");
    let old_version = package_value(&document, "version");
    if package.is_some() && old_version.is_none() {
        return Err(CliError::Usage(format!(
            "{} has a package but no package.version",
            path.display()
        )));
    }
    if package.is_some() {
        document["package"]["version"] = value(version);
    }
    let relative = path.strip_prefix(root).unwrap_or(path).to_path_buf();
    let mut dependency_updates = Vec::new();
    update_dependency_tables(
        &mut document,
        &relative,
        train_packages,
        version,
        &mut dependency_updates,
    );
    let rendered = document.to_string();
    Ok(PlannedManifest {
        path: path.to_path_buf(),
        original: original.clone(),
        rendered: rendered.clone(),
        change: ManifestChange {
            manifest: relative,
            package,
            old_version,
            new_version: package_value(&document, "version"),
            changed: original != rendered,
        },
        dependency_updates,
    })
}

fn package_value(document: &DocumentMut, field: &str) -> Option<String> {
    document
        .get("package")
        .and_then(Item::as_table_like)
        .and_then(|package| package.get(field))
        .and_then(Item::as_str)
        .map(ToOwned::to_owned)
}

fn update_dependency_tables(
    document: &mut DocumentMut,
    manifest: &Path,
    train_packages: &BTreeSet<String>,
    version: &str,
    updates: &mut Vec<DependencyUpdate>,
) {
    update_named_dependency_sections(
        document.as_table_mut(),
        "",
        manifest,
        train_packages,
        version,
        updates,
    );
    if let Some(workspace_dependencies) = document
        .get_mut("workspace")
        .and_then(Item::as_table_mut)
        .and_then(|workspace| workspace.get_mut("dependencies"))
        .and_then(Item::as_table_mut)
    {
        update_dependencies(
            workspace_dependencies,
            "workspace.dependencies",
            manifest,
            train_packages,
            version,
            updates,
        );
    }
    if let Some(targets) = document.get_mut("target").and_then(Item::as_table_mut) {
        for (target, item) in targets.iter_mut() {
            let Some(target_table) = item.as_table_mut() else {
                continue;
            };
            update_named_dependency_sections(
                target_table,
                &format!("target.{target}"),
                manifest,
                train_packages,
                version,
                updates,
            );
        }
    }
}

fn update_named_dependency_sections(
    table: &mut Table,
    prefix: &str,
    manifest: &Path,
    train_packages: &BTreeSet<String>,
    version: &str,
    updates: &mut Vec<DependencyUpdate>,
) {
    for name in ["dependencies", "dev-dependencies", "build-dependencies"] {
        let Some(dependencies) = table.get_mut(name).and_then(Item::as_table_mut) else {
            continue;
        };
        let section = if prefix.is_empty() {
            name.to_owned()
        } else {
            format!("{prefix}.{name}")
        };
        update_dependencies(
            dependencies,
            &section,
            manifest,
            train_packages,
            version,
            updates,
        );
    }
}

fn update_dependencies(
    dependencies: &mut Table,
    section: &str,
    manifest: &Path,
    train_packages: &BTreeSet<String>,
    version: &str,
    updates: &mut Vec<DependencyUpdate>,
) {
    for (dependency, item) in dependencies.iter_mut() {
        let dependency = dependency.to_string();
        let package = dependency_package(&dependency, item);
        if !train_packages.contains(&package) || dependency_uses_workspace(item) {
            continue;
        }
        let old_version = dependency_version(item);
        set_dependency_version(item, version);
        if old_version.as_deref() != Some(version) {
            updates.push(DependencyUpdate {
                manifest: manifest.to_path_buf(),
                section: section.to_owned(),
                dependency,
                package,
                old_version,
                new_version: version.to_owned(),
            });
        }
    }
}

fn dependency_package(dependency: &str, item: &Item) -> String {
    item.as_inline_table()
        .and_then(|table| table.get("package"))
        .and_then(Value::as_str)
        .or_else(|| {
            item.as_table()
                .and_then(|table| table.get("package"))
                .and_then(Item::as_str)
        })
        .unwrap_or(dependency)
        .to_owned()
}

fn dependency_uses_workspace(item: &Item) -> bool {
    item.as_inline_table()
        .and_then(|table| table.get("workspace"))
        .and_then(Value::as_bool)
        .or_else(|| {
            item.as_table()
                .and_then(|table| table.get("workspace"))
                .and_then(Item::as_bool)
        })
        == Some(true)
}

fn dependency_version(item: &Item) -> Option<String> {
    item.as_str()
        .or_else(|| {
            item.as_inline_table()
                .and_then(|table| table.get("version"))
                .and_then(Value::as_str)
        })
        .or_else(|| {
            item.as_table()
                .and_then(|table| table.get("version"))
                .and_then(Item::as_str)
        })
        .map(ToOwned::to_owned)
}

fn set_dependency_version(item: &mut Item, version: &str) {
    if item.is_str() {
        *item = value(version);
    } else if let Some(table) = item.as_inline_table_mut() {
        table.insert("version", Value::from(version));
    } else if let Some(table) = item.as_table_mut() {
        table.insert("version", value(version));
    }
}

pub(super) fn apply_manifests(manifests: &[PlannedManifest]) -> CliResult<()> {
    let mut written: Vec<&PlannedManifest> = Vec::new();
    for manifest in manifests
        .iter()
        .filter(|manifest| manifest.original != manifest.rendered)
    {
        if let Err(error) = fs::write(&manifest.path, &manifest.rendered) {
            for previous in written.into_iter().rev() {
                let _ = fs::write(&previous.path, &previous.original);
            }
            return Err(CliError::Io(error));
        }
        written.push(manifest);
    }
    Ok(())
}

pub(super) fn restore_manifests(manifests: &[PlannedManifest]) {
    for manifest in manifests {
        if manifest.original != manifest.rendered {
            let _ = fs::write(&manifest.path, &manifest.original);
        }
    }
}
