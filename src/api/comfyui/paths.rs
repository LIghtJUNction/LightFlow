use std::fs;
use std::path::{Component, Path, PathBuf};

#[cfg(unix)]
use std::fs::{File, OpenOptions};
#[cfg(unix)]
use std::os::fd::{AsRawFd, RawFd};
#[cfg(unix)]
use std::os::unix::fs::MetadataExt;
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;

use crate::api::{ApiError, ApiResult};

#[derive(Debug)]
pub(super) struct OutputDirectory {
    display_path: PathBuf,
    #[cfg(unix)]
    handle: File,
    #[cfg(unix)]
    device: u64,
    #[cfg(unix)]
    inode: u64,
}

impl OutputDirectory {
    pub(super) fn path(&self) -> &Path {
        &self.display_path
    }

    #[cfg(unix)]
    pub(super) fn raw_fd(&self) -> RawFd {
        self.handle.as_raw_fd()
    }

    #[cfg(unix)]
    pub(super) fn verify_identity(&self) -> ApiResult<()> {
        let metadata = fs::metadata(&self.display_path).map_err(|error| {
            ApiError::InvalidRequest(format!(
                "output directory identity changed after validation: {}: {error}",
                self.display_path.display()
            ))
        })?;
        if !metadata.is_dir() || metadata.dev() != self.device || metadata.ino() != self.inode {
            return invalid(format!(
                "output directory identity changed after validation: {}",
                self.display_path.display()
            ));
        }
        Ok(())
    }
}

pub(super) fn canonical_project_root(root: &Path) -> ApiResult<PathBuf> {
    fs::canonicalize(root).map_err(|error| {
        ApiError::InvalidRequest(format!(
            "failed to canonicalize project root {}: {error}",
            root.display()
        ))
    })
}

pub(super) fn canonical_existing_project_file(
    root: &Path,
    value: &str,
    context: &str,
) -> ApiResult<PathBuf> {
    let relative = safe_relative_path(value, context)?;
    let canonical_root = canonical_project_root(root)?;
    let candidate = canonical_root.join(relative);
    let canonical = fs::canonicalize(&candidate).map_err(|error| {
        ApiError::InvalidRequest(format!(
            "{context} does not name an existing project file {}: {error}",
            candidate.display()
        ))
    })?;
    if !canonical.starts_with(&canonical_root) {
        return invalid(format!(
            "{context} escapes project root through a symlink: {}",
            candidate.display()
        ));
    }
    if !canonical.is_file() {
        return invalid(format!(
            "{context} does not name a regular file: {}",
            candidate.display()
        ));
    }
    Ok(canonical)
}

pub(super) fn prepare_output_dir(
    root: &Path,
    relative: &Path,
    context: &str,
) -> ApiResult<OutputDirectory> {
    let relative = safe_relative_path_value(relative, context)?;
    let canonical_root = canonical_project_root(root)?;
    verify_existing_ancestors(&canonical_root, &relative, context)?;
    let candidate = canonical_root.join(&relative);
    fs::create_dir_all(&candidate).map_err(|error| {
        ApiError::InvalidRequest(format!(
            "failed to create {context} {}: {error}",
            candidate.display()
        ))
    })?;
    verify_existing_ancestors(&canonical_root, &relative, context)?;
    let canonical = fs::canonicalize(&candidate).map_err(|error| {
        ApiError::InvalidRequest(format!(
            "failed to verify {context} {}: {error}",
            candidate.display()
        ))
    })?;
    if !canonical.starts_with(&canonical_root) {
        return invalid(format!(
            "{context} escapes project root: {}",
            candidate.display()
        ));
    }
    #[cfg(unix)]
    let handle = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(&canonical)
        .map_err(|error| {
            ApiError::InvalidRequest(format!(
                "failed to pin {context} {}: {error}",
                canonical.display()
            ))
        })?;
    #[cfg(unix)]
    let identity = handle.metadata()?;
    Ok(OutputDirectory {
        display_path: canonical,
        #[cfg(unix)]
        handle,
        #[cfg(unix)]
        device: identity.dev(),
        #[cfg(unix)]
        inode: identity.ino(),
    })
}

pub(super) fn safe_relative_path(value: &str, context: &str) -> ApiResult<PathBuf> {
    if value.is_empty() {
        return invalid(format!(
            "{context} must be a non-empty safe project-relative path"
        ));
    }
    safe_relative_path_value(Path::new(value), context)
}

fn safe_relative_path_value(path: &Path, context: &str) -> ApiResult<PathBuf> {
    if path.as_os_str().is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return invalid(format!("{context} must be a safe project-relative path"));
    }
    Ok(path.to_owned())
}

fn verify_existing_ancestors(root: &Path, relative: &Path, context: &str) -> ApiResult<()> {
    let mut current = root.to_owned();
    for component in relative.components() {
        let Component::Normal(component) = component else {
            return invalid(format!("{context} must be a safe project-relative path"));
        };
        current.push(component);
        let metadata = match fs::symlink_metadata(&current) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(ApiError::Io(error)),
        };
        if metadata.file_type().is_symlink() {
            return invalid(format!(
                "{context} must not traverse a symlink: {}",
                current.display()
            ));
        }
        if !metadata.is_dir() {
            return invalid(format!(
                "{context} ancestor is not a directory: {}",
                current.display()
            ));
        }
        let canonical = fs::canonicalize(&current)?;
        if !canonical.starts_with(root) {
            return invalid(format!(
                "{context} escapes project root: {}",
                current.display()
            ));
        }
    }
    Ok(())
}

fn invalid<T>(message: impl Into<String>) -> ApiResult<T> {
    Err(ApiError::InvalidRequest(message.into()))
}
