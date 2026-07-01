use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn source_shape_check_covers_first_party_rust_files() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = SourceShapeFixture::new()?;

    let output = Command::new("sh")
        .arg(fixture.root.join("scripts/check-source-shape.sh"))
        .current_dir(&fixture.root)
        .output()?;

    assert!(!output.status.success(), "source-shape check should fail");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("source_shape_over_limit_fixture.rs"),
        "stdout:\n{stdout}"
    );
    assert!(stdout.contains("over 500 lines"), "stdout:\n{stdout}");
    assert!(
        stdout.contains("tests/123_source_shape_fixture.rs"),
        "stdout:\n{stdout}"
    );
    assert!(
        stdout.contains("src/123_source_shape_fixture.rs"),
        "stdout:\n{stdout}"
    );
    assert!(
        stdout.contains("projects/example/workflows/demo/src/123_source_shape_fixture.rs"),
        "stdout:\n{stdout}"
    );
    assert!(
        stdout.contains("numeric-prefix filename"),
        "stdout:\n{stdout}"
    );
    assert!(
        !stdout.contains("generated_over_limit_fixture.rs"),
        "stdout:\n{stdout}"
    );
    assert!(
        !stdout.contains("vendor/123_source_shape_fixture.rs"),
        "stdout:\n{stdout}"
    );

    Ok(())
}

struct SourceShapeFixture {
    root: PathBuf,
}

impl SourceShapeFixture {
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let root = std::env::temp_dir().join(format!(
            "lightflow-source-shape-fixture-{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        ));
        fs::create_dir_all(root.join("scripts"))?;
        fs::create_dir_all(root.join("src"))?;
        fs::create_dir_all(root.join("tests"))?;
        fs::create_dir_all(root.join("projects/example/workflows/demo/src"))?;
        fs::create_dir_all(root.join("vendor"))?;
        fs::copy(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scripts/check-source-shape.sh"),
            root.join("scripts/check-source-shape.sh"),
        )?;
        fs::write(
            root.join("tests/source_shape_over_limit_fixture.rs"),
            (0..=500).map(|_| "fn fixture() {}\n").collect::<String>(),
        )?;
        fs::write(
            root.join("tests/123_source_shape_fixture.rs"),
            "fn fixture() {}\n",
        )?;
        fs::write(
            root.join("src/123_source_shape_fixture.rs"),
            "fn fixture() {}\n",
        )?;
        fs::write(
            root.join("projects/example/workflows/demo/src/123_source_shape_fixture.rs"),
            "fn fixture() {}\n",
        )?;
        fs::write(
            root.join("src/generated_over_limit_fixture.rs"),
            format!(
                "// @generated\n{}",
                (0..=500).map(|_| "fn fixture() {}\n").collect::<String>()
            ),
        )?;
        fs::write(
            root.join("vendor/123_source_shape_fixture.rs"),
            "fn fixture() {}\n",
        )?;
        Ok(Self { root })
    }
}

impl Drop for SourceShapeFixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
