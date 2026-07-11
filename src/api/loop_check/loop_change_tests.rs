use super::loop_changes::{
    classify_workflow_change, loop_changes_across_project_set, workflow_crate_removed,
};
use super::test_support::{git_ok, temp_root};
use super::{LoopChangeStatus, WorkflowChangeKind};
use std::fs;
use std::path::{Path, PathBuf};

#[test]
fn classify_workflow_change_tracks_direct_workflow_root_files()
-> Result<(), Box<dyn std::error::Error>> {
    let root = temp_root();
    fs::create_dir_all(root.join("workflows/lightflow.direct/src"))?;
    fs::write(
        root.join("workflows/lightflow.direct/Cargo.toml"),
        "[package]\n",
    )?;
    fs::write(
        root.join("workflows/lightflow.direct/src/lib.rs"),
        "pub fn define() {}\n",
    )?;

    let (workflow_key, kind) =
        classify_workflow_change(&root, Path::new("workflows/lightflow.direct/build.rs"))
            .expect("direct workflow build script should be classified");
    assert_eq!(workflow_key, "lightflow.direct");
    assert_eq!(kind, WorkflowChangeKind::Workflow);

    let (workflow_key, kind) =
        classify_workflow_change(&root, Path::new("workflows/lightflow.direct/README.md"))
            .expect("direct workflow readme should be classified");
    assert_eq!(workflow_key, "lightflow.direct");
    assert_eq!(kind, WorkflowChangeKind::Workflow);

    let (workflow_key, kind) = classify_workflow_change(
        &root,
        Path::new("workflows/lightflow.direct/examples/demo.rs"),
    )
    .expect("direct workflow example should be classified");
    assert_eq!(workflow_key, "lightflow.direct");
    assert_eq!(kind, WorkflowChangeKind::Workflow);

    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn classify_workflow_change_ignores_retained_non_crate_directories()
-> Result<(), Box<dyn std::error::Error>> {
    let root = temp_root();
    fs::create_dir_all(root.join("workflows/alpha"))?;
    fs::write(root.join("workflows/alpha/notes.md"), "retained\n")?;

    let change = classify_workflow_change(&root, Path::new("workflows/alpha/notes.md"));

    assert!(change.is_none());
    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn loop_changes_ignores_changes_in_retained_non_crate_directories()
-> Result<(), Box<dyn std::error::Error>> {
    let root = temp_root();
    fs::create_dir_all(root.join("workflows/alpha"))?;
    fs::write(root.join("workflows/alpha/notes.md"), "before\n")?;
    git_ok(&root, ["init"])?;
    git_ok(&root, ["add", "."])?;
    git_ok(
        &root,
        [
            "-c",
            "user.email=lightflow@example.invalid",
            "-c",
            "user.name=LightFlow Test",
            "commit",
            "-m",
            "fixture",
        ],
    )?;
    fs::write(root.join("workflows/alpha/notes.md"), "after\n")?;

    let report = loop_changes_across_project_set(&root)?;

    assert!(report.changed_workflows.is_empty());
    assert!(report.blockers.is_empty());
    assert_eq!(report.failed, 0);
    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn workflow_crate_removed_requires_missing_manifest_and_missing_changed_paths()
-> Result<(), Box<dyn std::error::Error>> {
    let root = temp_root();
    fs::create_dir_all(root.join("workflows/example/src"))?;
    fs::write(root.join("workflows/example/Cargo.toml"), "[package]\n")?;

    let removed_paths = vec![
        PathBuf::from("workflows/example/Cargo.toml"),
        PathBuf::from("workflows/example/src/lib.rs"),
    ];
    assert!(!workflow_crate_removed(&root, &removed_paths));

    fs::remove_file(root.join("workflows/example/Cargo.toml"))?;
    assert!(workflow_crate_removed(&root, &removed_paths));

    let partial_delete = vec![PathBuf::from("workflows/example/src/lib.rs")];
    assert!(!workflow_crate_removed(&root, &partial_delete));

    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn loop_changes_treats_complete_workflow_crate_removal_as_safe()
-> Result<(), Box<dyn std::error::Error>> {
    let root = temp_root();
    let crate_dir = root.join("workflows/removed");
    fs::create_dir_all(crate_dir.join("src"))?;
    fs::write(crate_dir.join("Cargo.toml"), "[package]\n")?;
    fs::write(crate_dir.join("src/lib.rs"), "pub fn define() {}\n")?;
    git_ok(&root, ["init"])?;
    git_ok(&root, ["add", "."])?;
    git_ok(
        &root,
        [
            "-c",
            "user.email=lightflow@example.invalid",
            "-c",
            "user.name=LightFlow Test",
            "commit",
            "-m",
            "fixture",
        ],
    )?;

    fs::remove_dir_all(&crate_dir)?;

    let report = loop_changes_across_project_set(&root)?;
    assert_eq!(report.failed, 0);
    assert_eq!(report.blockers, Vec::<String>::new());
    let removed = report
        .changed_workflows
        .iter()
        .find(|change| change.workflow_key == "removed")
        .expect("removed workflow change");
    assert_eq!(removed.status, LoopChangeStatus::Passed);
    assert!(removed.message.contains("workflow crate removed"));
    assert!(!removed.skill_changed);

    let _ = fs::remove_dir_all(root);
    Ok(())
}
