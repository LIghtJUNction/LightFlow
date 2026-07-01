use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub(super) struct TestDir {
    path: PathBuf,
}

impl TestDir {
    pub(super) fn new(name: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock after unix epoch")
            .as_nanos();
        Self {
            path: std::env::temp_dir().join(format!(
                "lightflow-publish-checks-{name}-{}-{nanos}",
                std::process::id()
            )),
        }
    }

    pub(super) fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}
