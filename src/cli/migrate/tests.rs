use super::*;
use std::cell::RefCell;

struct RenameFailure {
    from: PathBuf,
    to: PathBuf,
    message: &'static str,
}

struct FaultingFileOps {
    rename_failures: RefCell<Vec<RenameFailure>>,
}

impl FaultingFileOps {
    fn new(rename_failures: Vec<RenameFailure>) -> Self {
        Self {
            rename_failures: RefCell::new(rename_failures),
        }
    }
}

impl MigrationFileOps for FaultingFileOps {
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        let failure = self
            .rename_failures
            .borrow()
            .iter()
            .position(|failure| failure.from == from && failure.to == to);
        if let Some(index) = failure {
            let failure = self.rename_failures.borrow_mut().remove(index);
            return Err(io::Error::other(failure.message));
        }
        fs::rename(from, to)
    }

    fn write(&self, path: &Path, contents: &str) -> io::Result<()> {
        fs::write(path, contents)
    }

    fn remove_file(&self, path: &Path) -> io::Result<()> {
        fs::remove_file(path)
    }
}

#[test]
fn execute_migration_rolls_back_first_move_when_second_move_fails() {
    let root = tempfile::tempdir().expect("tempdir");
    let first = move_plan(root.path(), "alpha", "one");
    let second = move_plan(root.path(), "beta", "two");
    let update = manifest_update(root.path(), "Cargo.toml", "before", "after");
    let file_ops = FaultingFileOps::new(vec![RenameFailure {
        from: second.from.clone(),
        to: second.to.clone(),
        message: "injected second move failure",
    }]);

    let error = execute_migration_with(&[first, second], &[update], &file_ops)
        .expect_err("second move should fail");

    assert!(error.to_string().contains("injected second move failure"));
    assert!(root.path().join("workflows/alpha/one").is_dir());
    assert!(root.path().join("workflows/beta/two").is_dir());
    assert!(!root.path().join("workflows/one").exists());
    assert!(!root.path().join("workflows/two").exists());
    assert!(!root.path().join("Cargo.toml.lightflow-migrate").exists());
    assert_eq!(
        fs::read_to_string(root.path().join("Cargo.toml")).expect("manifest"),
        "before"
    );
}

#[test]
fn execute_migration_restores_moves_and_manifests_when_second_apply_fails() {
    let root = tempfile::tempdir().expect("tempdir");
    let plan = move_plan(root.path(), "alpha", "one");
    let first = manifest_update(root.path(), "Cargo.toml", "first-before", "first-after");
    let second = manifest_update(
        root.path(),
        ".lightflow/Cargo.toml",
        "second-before",
        "second-after",
    );
    let file_ops = FaultingFileOps::new(vec![RenameFailure {
        from: second.temporary.clone(),
        to: second.path.clone(),
        message: "injected second manifest apply failure",
    }]);

    let error = execute_migration_with(&[plan], &[first, second], &file_ops)
        .expect_err("second manifest apply should fail");

    assert!(
        error
            .to_string()
            .contains("injected second manifest apply failure")
    );
    assert!(root.path().join("workflows/alpha/one").is_dir());
    assert!(!root.path().join("workflows/one").exists());
    assert_eq!(
        fs::read_to_string(root.path().join("Cargo.toml")).expect("root manifest"),
        "first-before"
    );
    assert_eq!(
        fs::read_to_string(root.path().join(".lightflow/Cargo.toml")).expect("nested manifest"),
        "second-before"
    );
    assert!(!root.path().join("Cargo.toml.lightflow-migrate").exists());
    assert!(
        !root
            .path()
            .join(".lightflow/Cargo.toml.lightflow-migrate")
            .exists()
    );
}

#[test]
fn execute_migration_reports_rollback_failure_with_primary_error() {
    let root = tempfile::tempdir().expect("tempdir");
    let first = move_plan(root.path(), "alpha", "one");
    let second = move_plan(root.path(), "beta", "two");
    let file_ops = FaultingFileOps::new(vec![
        RenameFailure {
            from: second.from.clone(),
            to: second.to.clone(),
            message: "injected forward failure",
        },
        RenameFailure {
            from: first.to.clone(),
            to: first.from.clone(),
            message: "injected rollback failure",
        },
    ]);

    let error = execute_migration_with(&[first, second], &[], &file_ops)
        .expect_err("move and rollback should fail")
        .to_string();

    assert!(error.contains("injected forward failure"));
    assert!(error.contains("migration recovery failed"));
    assert!(error.contains("injected rollback failure"));
}

fn move_plan(root: &Path, category: &str, crate_name: &str) -> MovePlan {
    let category = root.join("workflows").join(category);
    let from = category.join(crate_name);
    fs::create_dir_all(&from).expect("source crate");
    MovePlan {
        from,
        to: root.join("workflows").join(crate_name),
        category,
    }
}

fn manifest_update(root: &Path, relative: &str, before: &str, after: &str) -> ManifestUpdate {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().expect("manifest parent")).expect("manifest parent");
    fs::write(&path, before).expect("manifest before");
    ManifestUpdate {
        temporary: path.with_extension("toml.lightflow-migrate"),
        path,
        before: before.to_owned(),
        after: after.to_owned(),
    }
}
