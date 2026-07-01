use crate::cli::{CliError, CliResult};
use std::fs;
use std::path::Path;

pub(super) fn write_new_text(path: &Path, body: &str, created: &mut Vec<String>) -> CliResult<()> {
    if path.exists() {
        return Err(CliError::Usage(format!(
            "{} already exists; refusing to overwrite",
            path.display()
        )));
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, body)?;
    created.push(path.to_string_lossy().into_owned());
    Ok(())
}

pub(super) fn write_init_text(path: &Path, body: &str, created: &mut Vec<String>) -> CliResult<()> {
    if path.exists() {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, body)?;
    created.push(path.to_string_lossy().into_owned());
    Ok(())
}
