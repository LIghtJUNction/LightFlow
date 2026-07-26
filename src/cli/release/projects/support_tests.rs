use super::tests::Fixture;
use super::*;
use std::fs;

#[test]
fn support_packages_follow_path_dependency_order_before_workflows() {
    let fixture = Fixture::new("support-order");
    fixture.write_release_train(false);
    configure_support_chain(&fixture);

    let report = release_projects(
        &ApiService::new(&fixture.path),
        &strings(["3.0.0", "--publish"]),
    )
    .expect("release plan");

    assert_eq!(
        report["publish_order"],
        serde_json::json!(["lightflow", "support-b", "support-a", "lightflow-example"])
    );
    assert_eq!(report["executed"], serde_json::json!([]));
}

#[test]
fn invalid_publishable_support_stops_before_writes_or_commands() {
    let fixture = Fixture::new("blocked-support");
    fixture.write_release_train(false);
    let support = fixture
        .path
        .join("projects/lightflow-std/runtime/Cargo.toml");
    let source = fs::read_to_string(&support)
        .unwrap()
        .replace("description = \"Test support crate.\"\n", "");
    fs::write(&support, source).unwrap();
    let root_manifest = fixture.path.join("Cargo.toml");
    let root_before = fs::read_to_string(&root_manifest).unwrap();

    let error = release_projects(
        &ApiService::new(&fixture.path),
        &strings(["2.0.0", "--apply", "--publish"]),
    )
    .expect_err("invalid support must stop release");
    let message = error.to_string();

    assert!(
        message.contains("package.description is missing"),
        "{message}"
    );
    assert!(message.contains("\"executed\":[]"), "{message}");
    assert_eq!(fs::read_to_string(root_manifest).unwrap(), root_before);
}

#[test]
fn incidental_non_publishable_workspace_members_are_skipped() {
    let fixture = Fixture::new("incidental-members");
    fixture.write_release_train(false);
    let project = fixture.path.join("projects/lightflow-std");
    let workspace = project.join("Cargo.toml");
    let source = fs::read_to_string(&workspace).unwrap().replace(
        "members = [\"runtime\", \"workflows/*\"]",
        "members = [\"runtime\", \"tools/*\", \"workflows/*\"]",
    );
    fs::write(&workspace, source).unwrap();
    write_incidental(&project.join("tools/false"), "tool-false", "false");
    write_incidental(&project.join("tools/empty"), "tool-empty", "[]");

    let report = release_projects(
        &ApiService::new(&fixture.path),
        &strings(["2.0.0", "--publish"]),
    )
    .expect("incidental members do not block release");
    let order = report["publish_order"].as_array().unwrap();

    assert!(!order.iter().any(|package| package == "tool-false"));
    assert!(!order.iter().any(|package| package == "tool-empty"));
}

#[test]
fn referenced_non_publishable_incidental_member_blocks_support() {
    let fixture = Fixture::new("referenced-incidental");
    fixture.write_release_train(false);
    let project = fixture.path.join("projects/lightflow-std");
    let workspace = project.join("Cargo.toml");
    let source = fs::read_to_string(&workspace).unwrap().replace(
        "members = [\"runtime\", \"workflows/*\"]",
        "members = [\"runtime\", \"tools/*\", \"workflows/*\"]",
    );
    fs::write(&workspace, source).unwrap();
    write_incidental(&project.join("tools/private"), "private-tool", "false");
    add_support_path_dependency(
        &project.join("runtime/Cargo.toml"),
        "private-tool = { path = \"../tools/private\", version = \"0.1.0\" }",
    );

    let error = release_projects(
        &ApiService::new(&fixture.path),
        &strings(["2.0.0", "--apply", "--publish"]),
    )
    .expect_err("referenced private member must block release");
    let message = error.to_string();

    assert!(message.contains("package.publish = false"), "{message}");
    assert!(message.contains("\"executed\":[]"), "{message}");
}

#[test]
fn support_dependency_excluded_from_members_fails_closed() {
    let fixture = Fixture::new("excluded-support-dependency");
    fixture.write_release_train(false);
    let project = fixture.path.join("projects/lightflow-std");
    let workspace = project.join("Cargo.toml");
    let source = fs::read_to_string(&workspace).unwrap().replace(
        "members = [\"runtime\", \"workflows/*\"]",
        "members = [\"runtime\", \"excluded\", \"workflows/*\"]\nexclude = [\"excluded\"]",
    );
    fs::write(&workspace, source).unwrap();
    let excluded = project.join("excluded");
    fs::create_dir_all(excluded.join("src")).unwrap();
    fs::write(
        excluded.join("Cargo.toml"),
        "[package]\nname = \"excluded-support\"\nversion = \"0.1.0\"\nedition = \"2024\"\ndescription = \"Excluded.\"\nlicense = \"MIT\"\n",
    )
    .unwrap();
    fs::write(excluded.join("src/lib.rs"), "pub fn excluded() {}\n").unwrap();
    add_support_path_dependency(
        &project.join("runtime/Cargo.toml"),
        "excluded-support = { path = \"../excluded\", version = \"0.1.0\" }",
    );

    let error = release_projects(
        &ApiService::new(&fixture.path),
        &strings(["2.0.0", "--apply", "--publish"]),
    )
    .expect_err("excluded support dependency must block release");
    let message = error.to_string();

    assert!(message.contains("excluded from the workspace release catalog"));
    assert!(message.contains("\"executed\":[]"), "{message}");
}

#[test]
fn support_dependency_cycle_fails_closed() {
    let dependencies = std::collections::BTreeMap::from([
        (
            "support-a".to_owned(),
            std::collections::BTreeSet::from(["support-b".to_owned()]),
        ),
        (
            "support-b".to_owned(),
            std::collections::BTreeSet::from(["support-a".to_owned()]),
        ),
    ]);

    let error =
        support::topological_support_order(&dependencies).expect_err("cycle must be rejected");
    assert!(error.to_string().contains("cycle"));
    assert!(error.to_string().contains("\"executed\":[]"));
}

fn configure_support_chain(fixture: &Fixture) {
    let project = fixture.path.join("projects/lightflow-std");
    let workspace = project.join("Cargo.toml");
    let source = fs::read_to_string(&workspace)
        .unwrap()
        .replace(
            "members = [\"runtime\", \"workflows/*\"]",
            "members = [\"runtime\", \"support-a\", \"workflows/*\"]",
        )
        .replace(
            "lightflow-support = { path = \"runtime\", version = \"0.1.0\" }",
            "support-b = { path = \"runtime\", version = \"0.1.0\" }\nsupport-a = { path = \"support-a\", version = \"0.1.0\" }",
        );
    fs::write(&workspace, source).unwrap();

    let support_b = project.join("runtime/Cargo.toml");
    let source = fs::read_to_string(&support_b)
        .unwrap()
        .replace("lightflow-support", "support-b");
    fs::write(&support_b, source).unwrap();

    let support_a = project.join("support-a");
    fs::create_dir_all(support_a.join("src")).unwrap();
    fs::write(
        support_a.join("Cargo.toml"),
        r#"[package]
name = "support-a"
version = "0.1.0"
edition = "2024"
description = "Support A."
license = "MIT"

[dependencies]
support-b = { workspace = true }
"#,
    )
    .unwrap();
    fs::write(support_a.join("src/lib.rs"), "pub fn support_a() {}\n").unwrap();

    let workflow = project.join("workflows/example/Cargo.toml");
    let source = fs::read_to_string(&workflow).unwrap().replace(
        "lightflow-support = { workspace = true }",
        "support-a = { workspace = true }",
    );
    fs::write(workflow, source).unwrap();
}

fn write_incidental(root: &std::path::Path, package: &str, publish: &str) {
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("Cargo.toml"),
        format!(
            "[package]\nname = {package:?}\nversion = \"0.1.0\"\nedition = \"2024\"\npublish = {publish}\n"
        ),
    )
    .unwrap();
    fs::write(root.join("src/lib.rs"), "pub fn tool() {}\n").unwrap();
}

fn add_support_path_dependency(manifest: &std::path::Path, dependency: &str) {
    let source = fs::read_to_string(manifest).unwrap();
    fs::write(manifest, format!("{source}\n{dependency}\n")).unwrap();
}

fn strings<const N: usize>(values: [&str; N]) -> Vec<String> {
    values.into_iter().map(ToOwned::to_owned).collect()
}
