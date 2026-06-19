use super::add::{AddDependencyOptions, DependencySource, add_dependency};
use super::{CliError, CliResult, required_flag_value};
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use toml_edit::DocumentMut;

#[derive(Debug, Clone, Eq, PartialEq)]
pub(super) struct ImportOptions {
    pub(super) source: ImportSource,
    pub(super) global: bool,
    pub(super) name: Option<String>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(super) enum ImportSource {
    Path(PathBuf),
    Git(String),
}

pub(super) fn parse_import_options(args: &[String]) -> CliResult<ImportOptions> {
    let mut source = None;
    let mut global = false;
    let mut name = None;
    let mut force_git = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--global" | "-g" => {
                global = true;
                index += 1;
            }
            "--git" => {
                force_git = true;
                index += 1;
            }
            "--name" => {
                if name.is_some() {
                    return Err(CliError::Usage("duplicate flag --name".to_owned()));
                }
                name = Some(required_flag_value(args, index, "--name")?.to_owned());
                index += 2;
            }
            value if value.starts_with("--") => {
                return Err(CliError::Usage(format!(
                    "unexpected argument for import: {value}"
                )));
            }
            value => {
                if source.is_some() {
                    return Err(CliError::Usage(format!(
                        "unexpected argument for import: {value}"
                    )));
                }
                source = Some(value.to_owned());
                index += 1;
            }
        }
    }
    let source = source.ok_or_else(|| CliError::Usage("missing import source".to_owned()))?;
    let source = if force_git || looks_like_git_source(&source) {
        ImportSource::Git(source)
    } else {
        ImportSource::Path(PathBuf::from(source))
    };
    Ok(ImportOptions {
        source,
        global,
        name,
    })
}

pub(super) fn import_workflow_repo(
    workspace_root: &Path,
    repo_store_root: &Path,
    options: &ImportOptions,
) -> CliResult<serde_json::Value> {
    let (source_root, source_json) = match &options.source {
        ImportSource::Path(path) => {
            let source_root = path.canonicalize()?;
            (source_root, json!({ "path": path }))
        }
        ImportSource::Git(url) => {
            let clone_dir = repo_store_root.join(repo_slug(options.name.as_deref(), url));
            sync_git_repo(url, &clone_dir)?;
            (clone_dir.canonicalize()?, json!({ "git": url }))
        }
    };
    let crates = discover_workflow_crates(&source_root)?;
    if crates.is_empty() {
        return Err(CliError::Usage(format!(
            "no workflow crates found under {}",
            source_root.display()
        )));
    }

    let mut imported = Vec::new();
    for workflow_crate in &crates {
        let path = workflow_crate.path.display().to_string();
        let dependency = add_dependency(
            workspace_root,
            &AddDependencyOptions {
                crate_name: workflow_crate.package.clone(),
                source: DependencySource::Path(path.clone()),
                version: None,
                package: None,
                global: options.global,
                editable: matches!(options.source, ImportSource::Path(_)),
            },
            options.global,
        )?;
        imported.push(json!({
            "package": workflow_crate.package,
            "category": workflow_crate.category,
            "path": path,
            "dependency": dependency,
        }));
    }

    Ok(json!({
        "source": source_json,
        "source_root": source_root,
        "workspace": workspace_root,
        "global": options.global,
        "imported": imported,
    }))
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct WorkflowCrate {
    category: String,
    package: String,
    path: PathBuf,
}

fn discover_workflow_crates(root: &Path) -> CliResult<Vec<WorkflowCrate>> {
    let collection = if root.join("workflows").is_dir() {
        root.join("workflows")
    } else {
        root.to_path_buf()
    };
    let mut crates = Vec::new();
    let Ok(categories) = fs::read_dir(&collection) else {
        return Ok(crates);
    };
    for category in categories {
        let category = category?;
        let category_path = category.path();
        if !category_path.is_dir() {
            continue;
        }
        let category_name = category.file_name().to_string_lossy().into_owned();
        let Ok(entries) = fs::read_dir(&category_path) else {
            continue;
        };
        for entry in entries {
            let entry = entry?;
            let crate_path = entry.path();
            if !crate_path.join("Cargo.toml").is_file()
                || !crate_path.join("src").join("lib.rs").is_file()
            {
                continue;
            }
            crates.push(WorkflowCrate {
                category: category_name.clone(),
                package: package_name(&crate_path.join("Cargo.toml"))?,
                path: crate_path.canonicalize()?,
            });
        }
    }
    crates.sort_by(|left, right| left.package.cmp(&right.package));
    Ok(crates)
}

fn package_name(manifest: &Path) -> CliResult<String> {
    let source = fs::read_to_string(manifest)?;
    let document = source
        .parse::<DocumentMut>()
        .map_err(|error| CliError::Usage(format!("invalid Cargo manifest: {error}")))?;
    document
        .get("package")
        .and_then(|package| package.get("name"))
        .and_then(toml_edit::Item::as_str)
        .map(str::to_owned)
        .ok_or_else(|| CliError::Usage(format!("missing package.name in {}", manifest.display())))
}

fn sync_git_repo(url: &str, clone_dir: &Path) -> CliResult<()> {
    if clone_dir.exists() {
        run_status(
            Command::new("git")
                .arg("-C")
                .arg(clone_dir)
                .arg("pull")
                .arg("--ff-only"),
        )?;
        return Ok(());
    }
    if let Some(parent) = clone_dir.parent() {
        fs::create_dir_all(parent)?;
    }
    run_status(Command::new("git").arg("clone").arg(url).arg(clone_dir))
}

fn run_status(command: &mut Command) -> CliResult<()> {
    let status = command.status()?;
    if status.success() {
        Ok(())
    } else {
        Err(CliError::Usage(format!(
            "command failed with status {status}"
        )))
    }
}

fn repo_slug(name: Option<&str>, source: &str) -> String {
    let value = name.unwrap_or_else(|| {
        source
            .trim_end_matches('/')
            .rsplit(['/', ':'])
            .next()
            .unwrap_or("repo")
            .trim_end_matches(".git")
    });
    let mut slug = String::new();
    let mut previous_dash = false;
    for character in value.chars() {
        if character.is_ascii_alphanumeric() {
            slug.push(character.to_ascii_lowercase());
            previous_dash = false;
        } else if !previous_dash {
            slug.push('-');
            previous_dash = true;
        }
    }
    let slug = slug.trim_matches('-');
    if slug.is_empty() {
        "repo".to_owned()
    } else {
        slug.to_owned()
    }
}

fn looks_like_git_source(source: &str) -> bool {
    source.starts_with("https://")
        || source.starts_with("http://")
        || source.starts_with("ssh://")
        || source.starts_with("git@")
        || source.ends_with(".git")
}
