use std::path::{Component, Path};

pub(crate) fn project_filter_matches(
    filter: &str,
    name: &str,
    label: impl AsRef<Path>,
    root: &Path,
) -> bool {
    let label = label.as_ref();
    filter == name
        || filter == label.display().to_string()
        || filter == root.display().to_string()
        || normalized_path_text_matches(filter, label)
        || project_path_matches(filter, root)
        || name
            .strip_prefix("lightflow-")
            .is_some_and(|alias| filter == alias)
}

fn project_path_matches(filter: &str, root: &Path) -> bool {
    let filter = Path::new(filter);
    match (filter.canonicalize(), root.canonicalize()) {
        (Ok(filter), Ok(root)) => filter == root,
        _ => false,
    }
}

fn normalized_path_text_matches(filter: &str, label: &Path) -> bool {
    normalized_path_text(Path::new(filter)) == normalized_path_text(label)
}

fn normalized_path_text(path: &Path) -> String {
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if parts.last().is_some_and(|part| part != "..") {
                    parts.pop();
                } else {
                    parts.push("..".to_owned());
                }
            }
            Component::Normal(part) => parts.push(part.to_string_lossy().into_owned()),
            Component::RootDir | Component::Prefix(_) => {
                parts.push(component.as_os_str().to_string_lossy().into_owned())
            }
        }
    }
    parts.join("/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn project_filter_matches_names_labels_aliases_and_paths() {
        let root = test_dir("project-filter");
        let workspace = root.path.join("projects/lightflow-std");
        fs::create_dir_all(&workspace).expect("workspace dir");

        for filter in [
            "lightflow-std",
            "projects/lightflow-std",
            "./projects/lightflow-std",
            "projects/extra/../lightflow-std/",
            "std",
            workspace.to_str().expect("workspace path"),
        ] {
            assert!(
                project_filter_matches(
                    filter,
                    "lightflow-std",
                    "projects/lightflow-std",
                    &workspace,
                ),
                "filter should match: {filter}"
            );
        }

        assert!(!project_filter_matches(
            "lightflow-flux",
            "lightflow-std",
            "projects/lightflow-std",
            &workspace,
        ));
    }

    struct TestDir {
        path: PathBuf,
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn test_dir(name: &str) -> TestDir {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock after unix epoch")
            .as_nanos();
        TestDir {
            path: std::env::temp_dir().join(format!(
                "lightflow-project-filter-{name}-{}-{nanos}",
                std::process::id()
            )),
        }
    }
}
