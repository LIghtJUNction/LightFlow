use super::{CliError, CliResult};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use toml_edit::{DocumentMut, Formatted, Item, Value};

#[derive(Debug)]
struct MovePlan {
    from: PathBuf,
    to: PathBuf,
    category: PathBuf,
}

pub(super) fn parse_migrate_root(args: &[String]) -> CliResult<PathBuf> {
    match args {
        [] => Ok(std::env::current_dir()?),
        [arg] if matches!(arg.as_str(), "-h" | "--help" | "help") => {
            Err(CliError::Usage(migrate_usage()))
        }
        [arg] if !arg.starts_with('-') => Ok(PathBuf::from(arg)),
        _ => Err(CliError::Usage(migrate_usage())),
    }
}

fn migrate_usage() -> String {
    [
        "usage:",
        "  lfw migrate [path]",
        "",
        "Migrates legacy workflows/<category>/<crate> collections to workflows/<crate>.",
        "The command preflights every source and target before moving any crate.",
    ]
    .join("\n")
}

pub(super) fn migrate_workflow_collections(root: &Path) -> CliResult<serde_json::Value> {
    let collections = [
        root.join(".lightflow/workflows"),
        root.join("workflows"),
        root.join("lightflow/workflows"),
    ];
    let plans = migration_plans(&collections)?;
    let manifest_updates = manifest_updates(root)?;
    execute_migration(&plans, &manifest_updates)?;

    let mut retained = BTreeSet::new();
    for category in plans.iter().map(|plan| &plan.category) {
        match fs::remove_dir(category) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) if error.kind() == io::ErrorKind::DirectoryNotEmpty => {
                retained.insert(category.clone());
            }
            Err(error) => return Err(CliError::Io(error)),
        }
    }

    Ok(json!({
        "migrated": plans.len(),
        "moves": plans.iter().map(|plan| json!({
            "from": plan.from,
            "to": plan.to,
        })).collect::<Vec<_>>(),
        "from": plans.iter().map(|plan| &plan.from).collect::<Vec<_>>(),
        "to": plans.iter().map(|plan| &plan.to).collect::<Vec<_>>(),
        "updated_manifests": manifest_updates.iter().map(|update| &update.path).collect::<Vec<_>>(),
        "retained": retained,
    }))
}

fn migration_plans(collections: &[PathBuf]) -> CliResult<Vec<MovePlan>> {
    let mut plans = Vec::new();
    for collection in collections {
        let entries = match sorted_entries(collection) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(CliError::Io(error)),
        };
        let mut targets = BTreeMap::<PathBuf, PathBuf>::new();
        for entry in entries {
            let category = entry.path();
            if !entry.file_type()?.is_dir() || is_workflow_crate(&category) {
                continue;
            }
            let children = match sorted_entries(&category) {
                Ok(children) => children,
                Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                Err(error) => return Err(CliError::Io(error)),
            };
            for child in children {
                let from = child.path();
                if !child.file_type()?.is_dir() || !is_workflow_crate(&from) {
                    continue;
                }
                let file_name = from.file_name().ok_or_else(|| {
                    CliError::Usage(format!(
                        "workflow crate path has no name: {}",
                        from.display()
                    ))
                })?;
                let to = collection.join(file_name);
                if to.exists() {
                    return Err(CliError::Usage(format!(
                        "workflow migration target already exists: {}",
                        to.display()
                    )));
                }
                if let Some(existing) = targets.insert(to.clone(), from.clone()) {
                    return Err(CliError::Usage(format!(
                        "workflow migration maps both {} and {} to {}",
                        existing.display(),
                        from.display(),
                        to.display()
                    )));
                }
                plans.push(MovePlan {
                    from,
                    to,
                    category: category.clone(),
                });
            }
        }
    }
    plans.sort_by(|left, right| left.from.cmp(&right.from));
    Ok(plans)
}

fn is_workflow_crate(path: &Path) -> bool {
    path.join("Cargo.toml").is_file() && path.join("src/lib.rs").is_file()
}

fn sorted_entries(path: &Path) -> io::Result<Vec<fs::DirEntry>> {
    let mut entries = fs::read_dir(path)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(fs::DirEntry::path);
    Ok(entries)
}

struct ManifestUpdate {
    path: PathBuf,
    temporary: PathBuf,
    before: String,
    after: String,
}

fn manifest_updates(root: &Path) -> CliResult<Vec<ManifestUpdate>> {
    let candidates = [
        root.join("Cargo.toml"),
        root.join(".lightflow/Cargo.toml"),
        root.join("lightflow/Cargo.toml"),
    ];
    let mut updates = Vec::new();
    for path in candidates {
        let before = match fs::read_to_string(&path) {
            Ok(before) => before,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(CliError::Io(error)),
        };
        let mut document = before.parse::<DocumentMut>().map_err(|error| {
            CliError::Usage(format!(
                "invalid Cargo manifest {}; workflow migration did not move any files: {error}",
                path.display()
            ))
        })?;
        let Some(members) = document
            .get_mut("workspace")
            .and_then(|workspace| workspace.get_mut("members"))
            .and_then(Item::as_array_mut)
        else {
            continue;
        };
        let mut changed = false;
        for member in members.iter_mut() {
            let Value::String(member) = member else {
                continue;
            };
            let Some(replacement) = flat_workspace_member(member.value()) else {
                continue;
            };
            let decor = member.decor().clone();
            let mut replacement = Formatted::new(replacement.to_owned());
            *replacement.decor_mut() = decor;
            *member = replacement;
            changed = true;
        }
        if !changed {
            continue;
        }
        let after = document.to_string();
        if after == before {
            continue;
        }
        updates.push(ManifestUpdate {
            temporary: path.with_extension("toml.lightflow-migrate"),
            path,
            before,
            after,
        });
    }
    Ok(updates)
}

fn flat_workspace_member(member: &str) -> Option<&'static str> {
    match member {
        ".lightflow/workflows/*/*" => Some(".lightflow/workflows/*"),
        "workflows/*/*" => Some("workflows/*"),
        "lightflow/workflows/*/*" => Some("lightflow/workflows/*"),
        _ => None,
    }
}

trait MigrationFileOps {
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()>;
    fn write(&self, path: &Path, contents: &str) -> io::Result<()>;
    fn remove_file(&self, path: &Path) -> io::Result<()>;
}

struct RealFileOps;

impl MigrationFileOps for RealFileOps {
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        fs::rename(from, to)
    }

    fn write(&self, path: &Path, contents: &str) -> io::Result<()> {
        fs::write(path, contents)
    }

    fn remove_file(&self, path: &Path) -> io::Result<()> {
        fs::remove_file(path)
    }
}

fn execute_migration(plans: &[MovePlan], updates: &[ManifestUpdate]) -> CliResult<()> {
    execute_migration_with(plans, updates, &RealFileOps)
}

fn execute_migration_with(
    plans: &[MovePlan],
    updates: &[ManifestUpdate],
    file_ops: &impl MigrationFileOps,
) -> CliResult<()> {
    prepare_manifest_updates(updates, file_ops)?;

    let mut completed = Vec::new();
    for plan in plans {
        if let Err(error) = file_ops.rename(&plan.from, &plan.to) {
            let mut recovery_errors = rollback_moves(&completed, file_ops);
            recovery_errors.extend(cleanup_manifest_updates(updates, file_ops));
            return Err(migration_error(error, recovery_errors));
        }
        completed.push(plan);
    }
    if let Err(error) = apply_manifest_updates(updates, file_ops) {
        let mut recovery_errors = rollback_moves(&completed, file_ops);
        recovery_errors.extend(restore_manifests(updates, file_ops));
        return Err(migration_error(error, recovery_errors));
    }
    Ok(())
}

fn prepare_manifest_updates(
    updates: &[ManifestUpdate],
    file_ops: &impl MigrationFileOps,
) -> CliResult<()> {
    for update in updates {
        if let Err(error) = file_ops.write(&update.temporary, &update.after) {
            let recovery_errors = cleanup_manifest_updates(updates, file_ops);
            return Err(migration_error(error, recovery_errors));
        }
    }
    Ok(())
}

fn apply_manifest_updates(
    updates: &[ManifestUpdate],
    file_ops: &impl MigrationFileOps,
) -> io::Result<()> {
    for update in updates {
        file_ops.rename(&update.temporary, &update.path)?;
    }
    Ok(())
}

fn cleanup_manifest_updates(
    updates: &[ManifestUpdate],
    file_ops: &impl MigrationFileOps,
) -> Vec<String> {
    let mut errors = Vec::new();
    for update in updates {
        if let Err(error) = file_ops.remove_file(&update.temporary)
            && error.kind() != io::ErrorKind::NotFound
        {
            errors.push(format!(
                "remove temporary manifest {}: {error}",
                update.temporary.display()
            ));
        }
    }
    errors
}

fn restore_manifests(updates: &[ManifestUpdate], file_ops: &impl MigrationFileOps) -> Vec<String> {
    let mut errors = Vec::new();
    for update in updates {
        if let Err(error) = file_ops.write(&update.path, &update.before) {
            errors.push(format!(
                "restore manifest {}: {error}",
                update.path.display()
            ));
        }
    }
    errors.extend(cleanup_manifest_updates(updates, file_ops));
    errors
}

fn rollback_moves(completed: &[&MovePlan], file_ops: &impl MigrationFileOps) -> Vec<String> {
    let mut errors = Vec::new();
    for plan in completed.iter().rev() {
        if let Err(error) = file_ops.rename(&plan.to, &plan.from) {
            errors.push(format!(
                "roll back move {} to {}: {error}",
                plan.to.display(),
                plan.from.display()
            ));
        }
    }
    errors
}

fn migration_error(primary: io::Error, recovery_errors: Vec<String>) -> CliError {
    if recovery_errors.is_empty() {
        return CliError::Io(primary);
    }
    CliError::Io(io::Error::new(
        primary.kind(),
        format!(
            "{primary}; migration recovery failed: {}",
            recovery_errors.join("; ")
        ),
    ))
}

#[cfg(test)]
mod tests;
