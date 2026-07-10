#[cfg(unix)]
use std::ffi::CString;
use std::fs::File;
use std::io;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

#[cfg(unix)]
use std::os::fd::FromRawFd;

use super::paths::OutputDirectory;
use crate::api::{ApiError, ApiResult};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

pub(super) fn create_unique_temporary(
    output_dir: &OutputDirectory,
    target: &str,
) -> ApiResult<(String, File)> {
    validate_basename(target)?;
    for _ in 0..128 {
        let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let temporary = format!(".{target}.lightflow-{}-{counter}.part", std::process::id());
        match create_new_file(output_dir, &temporary) {
            Ok(file) => return Ok((temporary, file)),
            Err(ApiError::Io(error)) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    invalid("failed to allocate a unique artifact temporary file")
}

#[cfg(unix)]
fn create_new_file(output_dir: &OutputDirectory, name: &str) -> ApiResult<File> {
    let name = c_name(name)?;
    // SAFETY: The directory fd is owned by OutputDirectory, and name is a NUL-free basename.
    let fd = unsafe {
        libc::openat(
            output_dir.raw_fd(),
            name.as_ptr(),
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            0o600,
        )
    };
    if fd < 0 {
        return Err(ApiError::Io(io::Error::last_os_error()));
    }
    // SAFETY: openat returned a new owned file descriptor.
    Ok(unsafe { File::from_raw_fd(fd) })
}

#[cfg(not(unix))]
fn create_new_file(output_dir: &OutputDirectory, name: &str) -> ApiResult<File> {
    let _ = (output_dir, name);
    invalid("safe ComfyUI output persistence is unsupported on this platform")
}

pub(super) fn persist_no_clobber(
    output_dir: &OutputDirectory,
    temporary: &str,
    target: &str,
) -> ApiResult<()> {
    validate_basename(temporary)?;
    validate_basename(target)?;
    persist(output_dir, temporary, target)
}

#[cfg(unix)]
fn persist(output_dir: &OutputDirectory, temporary: &str, target: &str) -> ApiResult<()> {
    let temporary_name = c_name(temporary)?;
    let target_name = c_name(target)?;
    // SAFETY: Both names are NUL-free basenames resolved against the same owned directory fd.
    let result = unsafe {
        libc::linkat(
            output_dir.raw_fd(),
            temporary_name.as_ptr(),
            output_dir.raw_fd(),
            target_name.as_ptr(),
            0,
        )
    };
    if result != 0 {
        let error = io::Error::last_os_error();
        if error.kind() == io::ErrorKind::AlreadyExists {
            return invalid(format!(
                "refusing to overwrite existing artifact: {}",
                output_dir.path().join(target).display()
            ));
        }
        return Err(ApiError::Io(error));
    }
    Ok(())
}

#[cfg(not(unix))]
fn persist(output_dir: &OutputDirectory, temporary: &str, target: &str) -> ApiResult<()> {
    let _ = (output_dir, temporary, target);
    invalid("safe ComfyUI output persistence is unsupported on this platform")
}

pub(super) fn verify_and_finalize(
    output_dir: &OutputDirectory,
    temporary: &str,
    target: &str,
) -> ApiResult<()> {
    validate_basename(temporary)?;
    validate_basename(target)?;
    finalize(output_dir, temporary, target)
}

#[cfg(unix)]
fn finalize(output_dir: &OutputDirectory, temporary: &str, target: &str) -> ApiResult<()> {
    if let Err(error) = output_dir.verify_identity() {
        let _ = unlink(output_dir, temporary);
        let _ = unlink(output_dir, target);
        return Err(error);
    }
    if let Err(error) = unlink(output_dir, temporary) {
        let _ = unlink(output_dir, target);
        return Err(error);
    }
    Ok(())
}

#[cfg(not(unix))]
fn finalize(output_dir: &OutputDirectory, temporary: &str, target: &str) -> ApiResult<()> {
    let _ = (output_dir, temporary, target);
    invalid("safe ComfyUI output persistence is unsupported on this platform")
}

pub(super) fn cleanup_temporary(output_dir: &OutputDirectory, temporary: &str) {
    #[cfg(unix)]
    let _ = unlink(output_dir, temporary);
    #[cfg(not(unix))]
    let _ = (output_dir, temporary);
}

#[cfg(unix)]
fn unlink(output_dir: &OutputDirectory, name: &str) -> ApiResult<()> {
    let name = c_name(name)?;
    // SAFETY: name is a NUL-free basename resolved against the owned directory fd.
    if unsafe { libc::unlinkat(output_dir.raw_fd(), name.as_ptr(), 0) } == 0 {
        Ok(())
    } else {
        Err(ApiError::Io(io::Error::last_os_error()))
    }
}

fn validate_basename(name: &str) -> ApiResult<()> {
    if name.is_empty()
        || Path::new(name).file_name().and_then(|value| value.to_str()) != Some(name)
        || name.contains(['/', '\\', '\0'])
        || matches!(name, "." | "..")
    {
        return invalid("output filename must be a safe basename");
    }
    Ok(())
}

#[cfg(unix)]
fn c_name(name: &str) -> ApiResult<CString> {
    CString::new(name)
        .map_err(|_| ApiError::InvalidRequest("output filename contains NUL".to_owned()))
}

fn invalid<T>(message: impl Into<String>) -> ApiResult<T> {
    Err(ApiError::InvalidRequest(message.into()))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::Write;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;
    use crate::api::comfyui::paths;

    #[cfg(unix)]
    #[test]
    fn pinned_directory_controls_persistence_after_path_swap() {
        let root = test_root("output-directory-swap");
        fs::create_dir_all(&root).expect("test root");
        let output_dir = paths::prepare_output_dir(&root, Path::new("output"), "test output")
            .expect("prepare output");
        let pinned_path = root.join("pinned-output");
        let outside_path = root.join("outside");
        fs::create_dir(&outside_path).expect("outside directory");
        fs::rename(root.join("output"), &pinned_path).expect("move validated directory");
        std::os::unix::fs::symlink(&outside_path, root.join("output"))
            .expect("replace output path");

        let (temporary, mut file) =
            create_unique_temporary(&output_dir, "artifact.bin").expect("temporary output");
        file.write_all(b"pinned bytes").expect("write output");
        file.flush().expect("flush output");
        persist_no_clobber(&output_dir, &temporary, "artifact.bin").expect("link output");
        let error = verify_and_finalize(&output_dir, &temporary, "artifact.bin")
            .expect_err("changed directory identity must fail")
            .to_string();

        assert!(
            error.contains("output directory identity changed after validation"),
            "{error}"
        );
        assert!(!outside_path.join("artifact.bin").exists());
        assert!(!pinned_path.join("artifact.bin").exists());
        assert_eq!(
            fs::read_dir(&pinned_path)
                .expect("pinned directory")
                .count(),
            0
        );

        fs::remove_dir_all(root).expect("remove test root");
    }

    fn test_root(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "lightflow-comfyui-{name}-{}-{nanos}",
            std::process::id()
        ))
    }
}
