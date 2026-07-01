use super::package_name_from_id;
use std::path::Path;

pub(in crate::cli) fn workspace_manifest() -> String {
    format!(
        "[workspace]\nresolver = \"3\"\nmembers = [\".lightflow/workflows/*/*\"]\n\n[workspace.dependencies]\nlightflow = {:?}\n",
        env!("CARGO_PKG_VERSION")
    )
}

pub(in crate::cli) fn workflow_collection_manifest() -> String {
    format!(
        "[workspace]\nresolver = \"3\"\nmembers = [\"workflows/*/*\"]\n\n[workspace.dependencies]\nlightflow = {:?}\n",
        env!("CARGO_PKG_VERSION")
    )
}

pub(super) fn project_gitignore() -> String {
    [
        "/target/",
        "/.cache/",
        "/.test-xdg/",
        "/lfw.lock",
        "",
        "# Local editor and OS files",
        ".DS_Store",
        "*.swp",
        "*.swo",
        "",
    ]
    .join("\n")
}

pub(super) fn plugin_manifest(root: &Path) -> String {
    let package = root
        .file_name()
        .and_then(|name| name.to_str())
        .map(package_name_from_id)
        .unwrap_or_else(|| "lightflow-plugin".to_owned());
    format!(
        "[package]\nname = {:?}\nversion = \"0.1.0\"\nedition = \"2024\"\ndescription = \"LightFlow workflow plugin.\"\nlicense = \"MIT OR Apache-2.0\"\n\n[dependencies]\nlightflow = {:?}\n",
        package,
        env!("CARGO_PKG_VERSION")
    )
}

pub(super) fn workflow_manifest(workflow_id: &str) -> String {
    format!(
        "[package]\nname = {:?}\nversion = \"0.1.0\"\nedition = \"2024\"\ndescription = {:?}\nlicense = \"MIT OR Apache-2.0\"\nrepository = {:?}\n\n[dependencies]\nlightflow = {{ workspace = true }}\n",
        package_name_from_id(workflow_id),
        format!("LightFlow workflow {}", workflow_id),
        env!("CARGO_PKG_REPOSITORY")
    )
}
