mod abc_workflow_support;
mod support;

use abc_workflow_support::*;
use std::fs;
use std::process::Command;
use support::*;

#[test]
fn abc_workflow_projects_resolve_import_run_and_add_modes() -> Result<(), Box<dyn std::error::Error>>
{
    let base = unique_temp_root();
    let project_a = base.join("lightflow-a");
    let project_b = base.join("lightflow-b");
    let project_c = base.join("lightflow-c");
    fs::create_dir_all(&base)?;

    write_leaf_project(&project_b, "b", "lightflow.b", "B")?;
    write_leaf_project(&project_c, "c", "lightflow.c", "C")?;
    write_a_project(&project_a, &project_b, &project_c)?;

    for project in [&project_a, &project_b, &project_c] {
        let workspace = fs::read_to_string(project.join("Cargo.toml"))?;
        assert!(workspace.contains("[workspace.dependencies]"));
        assert!(workspace.contains("lightflow = { path = "));
        let crate_manifest = workflow_manifest(project)?;
        assert!(fs::read_to_string(crate_manifest)?.contains("lightflow = { workspace = true }"));
    }

    let incomplete_deps = lfw(&project_a, ["deps", "lightflow.a"])?;
    assert_eq!(incomplete_deps["complete"], false);
    assert_eq!(
        incomplete_deps["missing_workflows"],
        serde_json::json!(["lightflow.b", "lightflow.c"])
    );

    let b_path = project_b
        .join(".lightflow/workflows/abc/b")
        .display()
        .to_string();
    let c_path = project_c
        .join(".lightflow/workflows/abc/c")
        .display()
        .to_string();
    let c_relative_path = "../lightflow-c/.lightflow/workflows/abc/c";
    let editable_b = lfw(
        &project_a,
        [
            "add",
            "lightflow-b",
            "--path",
            b_path.as_str(),
            "--editable",
        ],
    )?;
    assert_eq!(editable_b["dependency"], "lightflow-b");
    assert_eq!(editable_b["source"]["path"], b_path);
    assert_eq!(editable_b["editable"], true);

    let path_c = lfw(
        &project_a,
        ["add", "lightflow-c", "--path", c_relative_path],
    )?;
    assert_eq!(path_c["dependency"], "lightflow-c");
    assert_eq!(path_c["source"]["path"], c_relative_path);
    assert_eq!(path_c["editable"], false);

    let manifest = fs::read_to_string(project_a.join("Cargo.toml"))?;
    assert!(manifest.contains(&format!("lightflow-b = {{ path = \"{b_path}\" }}")));
    assert!(
        manifest.contains("lightflow-c = { path = \"../lightflow-c/.lightflow/workflows/abc/c\" }")
    );
    assert!(!manifest.contains("editable"));

    let listed = lfw(&project_a, ["list"])?;
    let ids = workflow_ids(&listed);
    assert_eq!(ids, vec!["lightflow.a", "lightflow.b", "lightflow.c"]);

    let deps = lfw(&project_a, ["deps", "lightflow.a"])?;
    assert_eq!(deps["complete"], true);
    assert_eq!(
        deps["workflow_order"],
        serde_json::json!(["lightflow.b", "lightflow.c", "lightflow.a"])
    );

    let true_run = lfw(
        &project_a,
        [
            "run",
            "lightflow.a",
            "-i",
            "use_b=true",
            "-i",
            "value=from-b",
        ],
    )?;
    assert_eq!(true_run["outputs"]["value"], "from-b");
    assert_eq!(true_run["nodes"][0]["selected_workflow_id"], "lightflow.b");

    let false_run = lfw(
        &project_a,
        [
            "run",
            "lightflow.a",
            "-i",
            "use_b=false",
            "-i",
            "value=from-c",
        ],
    )?;
    assert_eq!(false_run["outputs"]["value"], "from-c");
    assert_eq!(false_run["nodes"][0]["selected_workflow_id"], "lightflow.c");

    let global_project = base.join("global-consumer");
    write_a_project(&global_project, &project_b, &project_c)?;
    lfw(&global_project, ["init"])?;
    let global_b = lfw(
        &global_project,
        [
            "add",
            "-g",
            "lightflow-b",
            "--path",
            b_path.as_str(),
            "--editable",
        ],
    )?;
    assert_eq!(global_b["global"], true);
    assert_eq!(global_b["editable"], true);
    let global_c = lfw(
        &global_project,
        ["add", "-g", "lightflow-c", "--path", c_path.as_str()],
    )?;
    assert_eq!(global_c["global"], true);

    let global_manifest = fs::read_to_string(global_project.join(".lightflow/Cargo.toml"))?;
    assert!(global_manifest.contains("lightflow-b"));
    assert!(global_manifest.contains("lightflow-c"));
    let global_deps = lfw(&global_project, ["deps", "lightflow.a"])?;
    assert_eq!(global_deps["complete"], true);

    let import_collection = base.join("import-collection");
    write_empty_workspace(&import_collection)?;
    write_leaf_project_in_workspace(&import_collection, "b", "lightflow.b", "B")?;
    write_leaf_project_in_workspace(&import_collection, "c", "lightflow.c", "C")?;
    let import_project = base.join("import-consumer");
    write_a_project(&import_project, &project_b, &project_c)?;
    let imported = lfw(
        &import_project,
        ["import", import_collection.to_str().unwrap()],
    )?;
    let imported_packages = imported["imported"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["package"].as_str().unwrap_or_default())
        .collect::<Vec<_>>();
    assert_eq!(imported_packages, vec!["lightflow-b", "lightflow-c"]);
    let import_deps = lfw(&import_project, ["deps", "lightflow.a"])?;
    assert_eq!(import_deps["complete"], true);

    let git_import_collection = base.join("git-import-collection");
    write_empty_workspace(&git_import_collection)?;
    write_leaf_project_in_workspace(&git_import_collection, "b", "lightflow.b", "B")?;
    write_leaf_project_in_workspace(&git_import_collection, "c", "lightflow.c", "C")?;
    init_git_repo(&git_import_collection)?;
    let git_import_project = base.join("git-import-consumer");
    write_a_project(&git_import_project, &project_b, &project_c)?;
    let git_import_url = format!("file://{}", git_import_collection.display());
    let git_imported = lfw(
        &git_import_project,
        [
            "import",
            "--git",
            "--name",
            "abc-import",
            git_import_url.as_str(),
        ],
    )?;
    assert_eq!(git_imported["source"]["git"], git_import_url);
    let git_imported_packages = git_imported["imported"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["package"].as_str().unwrap_or_default())
        .collect::<Vec<_>>();
    assert_eq!(git_imported_packages, vec!["lightflow-b", "lightflow-c"]);
    let git_imported_paths = git_imported["imported"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["path"].as_str().unwrap_or_default())
        .collect::<Vec<_>>();
    assert!(
        git_imported_paths
            .iter()
            .all(|path| path.contains(".lightflow/repos/abc-import/.lightflow/workflows/abc/")),
        "git import paths should use the local repo cache: {git_imported_paths:?}"
    );
    assert!(
        git_imported["imported"]
            .as_array()
            .unwrap()
            .iter()
            .all(|item| item["dependency"]["editable"] == false)
    );
    let git_import_deps = lfw(&git_import_project, ["deps", "lightflow.a"])?;
    assert_eq!(git_import_deps["complete"], true);

    let registry_project = base.join("registry-install");
    write_empty_workspace(&registry_project)?;
    let registry = lfw(
        &registry_project,
        ["add", "lightflow-b", "--version", "0.1.0"],
    )?;
    assert_eq!(registry["source"]["registry"], "crates.io");
    assert_eq!(registry["version"], "0.1.0");
    assert!(
        fs::read_to_string(registry_project.join("Cargo.toml"))?
            .contains("lightflow-b = { version = \"0.1.0\" }")
    );

    let github_project = base.join("github-install");
    write_empty_workspace(&github_project)?;
    let github_url = "https://github.com/lightjunction/lightflow-b";
    let github = lfw(
        &github_project,
        [
            "add",
            "lightflow-b",
            "--git",
            github_url,
            "--package",
            "lightflow-b",
        ],
    )?;
    assert_eq!(github["source"]["git"], github_url);
    assert_eq!(github["package"], "lightflow-b");
    assert!(fs::read_to_string(github_project.join("Cargo.toml"))?.contains(
        "lightflow-b = { git = \"https://github.com/lightjunction/lightflow-b\", package = \"lightflow-b\" }"
    ));

    let git_repo = base.join("lightflow-b-git");
    write_standalone_workflow_crate(&git_repo, "lightflow-b", "lightflow.git_b")?;
    init_git_repo(&git_repo)?;
    let git_project = base.join("git-install");
    write_fetch_workspace(&git_project)?;
    let git_url = format!("file://{}", git_repo.display());
    let git = lfw(
        &git_project,
        [
            "add",
            "lightflow-b-git",
            "--git",
            git_url.as_str(),
            "--package",
            "lightflow-b",
        ],
    )?;
    assert_eq!(git["source"]["git"], git_url);
    assert_eq!(git["package"], "lightflow-b");
    run_ok(Command::new("cargo").arg("fetch").current_dir(&git_project))?;
    let git_lock = fs::read_to_string(git_project.join("Cargo.lock"))?;
    assert!(git_lock.contains("name = \"lightflow-b\""));

    let _ = fs::remove_dir_all(base);
    Ok(())
}
