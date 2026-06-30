use super::{ApiError, ApiResult};
use std::fs;
use std::path::{Path, PathBuf};

pub(super) fn discover_local_workflow_crates(root: &Path) -> ApiResult<Vec<PathBuf>> {
    let mut crates =
        discover_workflow_collection_crates(&root.join(".lightflow").join("workflows"))?;
    crates.extend(discover_workflow_collection_crates(
        &root.join("workflows"),
    )?);
    crates.sort();
    crates.dedup();
    Ok(crates)
}

pub(super) fn discover_workflow_collection_crates(collection: &Path) -> ApiResult<Vec<PathBuf>> {
    let mut crates = Vec::new();
    let Ok(categories) = fs::read_dir(collection) else {
        return Ok(crates);
    };
    for category in categories {
        let category_path = category?.path();
        if !category_path.is_dir() {
            continue;
        }
        if category_path.join("Cargo.toml").is_file()
            && category_path.join("src").join("lib.rs").is_file()
        {
            crates.push(category_path);
            continue;
        }
        let Ok(entries) = fs::read_dir(&category_path) else {
            continue;
        };
        for entry in entries {
            let crate_dir = entry?.path();
            if crate_dir.join("Cargo.toml").is_file()
                && crate_dir.join("src").join("lib.rs").is_file()
            {
                crates.push(crate_dir);
            }
        }
    }
    crates.sort();
    Ok(crates)
}

pub(super) fn workflow_id_from_crate(crate_dir: &Path) -> ApiResult<String> {
    let lib = crate_dir.join("src").join("lib.rs");
    let source = fs::read_to_string(&lib)?;
    extract_workflow_id(&source).ok_or_else(|| {
        ApiError::InvalidRequest(format!(
            "workflow crate {} does not define workflow(\"...\")",
            crate_dir.display()
        ))
    })
}

fn extract_workflow_id(source: &str) -> Option<String> {
    let start = source.find("workflow(\"")? + "workflow(\"".len();
    let rest = &source[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_owned())
}
