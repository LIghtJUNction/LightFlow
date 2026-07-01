use toml_edit::{DocumentMut, Item};

pub(super) fn collect(
    document: &DocumentMut,
    workspace_document: Option<&DocumentMut>,
    issues: &mut Vec<String>,
) {
    collect_publish_dependency_issues(document.get("dependencies"), issues);
    collect_publish_dependency_issues(document.get("build-dependencies"), issues);
    collect_publish_dependency_issues(document.get("dev-dependencies"), issues);
    collect_target_publish_dependency_issues(document, issues);
    collect_publish_dependency_issues(
        document
            .get("workspace")
            .and_then(|workspace| workspace.get("dependencies")),
        issues,
    );
    collect_inherited_publish_dependency_issues(document, workspace_document, issues);
}

fn collect_publish_dependency_issues(dependencies: Option<&Item>, issues: &mut Vec<String>) {
    let Some(dependencies) = dependencies.and_then(Item::as_table_like) else {
        return;
    };
    for (name, dependency) in dependencies.iter() {
        collect_publish_dependency_issue(name, dependency, issues);
    }
}

fn collect_publish_dependency_issue(name: &str, dependency: &Item, issues: &mut Vec<String>) {
    if dependency.get("git").is_some() {
        issues.push(format!(
            "dependency {name} uses git, which cannot be published to crates.io"
        ));
    }
    if dependency.get("path").is_some() && dependency.get("version").is_none() {
        issues.push(format!(
            "dependency {name} uses path without a crates.io version"
        ));
    }
}

fn collect_target_publish_dependency_issues(document: &DocumentMut, issues: &mut Vec<String>) {
    let Some(targets) = document.get("target").and_then(Item::as_table_like) else {
        return;
    };
    for (_target, target) in targets.iter() {
        collect_publish_dependency_issues(target.get("dependencies"), issues);
        collect_publish_dependency_issues(target.get("build-dependencies"), issues);
        collect_publish_dependency_issues(target.get("dev-dependencies"), issues);
    }
}

fn collect_inherited_publish_dependency_issues(
    document: &DocumentMut,
    workspace_document: Option<&DocumentMut>,
    issues: &mut Vec<String>,
) {
    let Some(workspace_dependencies) = workspace_document
        .and_then(|document| document.get("workspace"))
        .and_then(|workspace| workspace.get("dependencies"))
        .and_then(Item::as_table_like)
    else {
        return;
    };
    collect_inherited_publish_dependency_section_issues(
        document.get("dependencies"),
        workspace_dependencies,
        issues,
    );
    collect_inherited_publish_dependency_section_issues(
        document.get("build-dependencies"),
        workspace_dependencies,
        issues,
    );
    collect_inherited_publish_dependency_section_issues(
        document.get("dev-dependencies"),
        workspace_dependencies,
        issues,
    );
    collect_inherited_target_publish_dependency_issues(document, workspace_dependencies, issues);
}

fn collect_inherited_publish_dependency_section_issues(
    dependencies: Option<&Item>,
    workspace_dependencies: &dyn toml_edit::TableLike,
    issues: &mut Vec<String>,
) {
    let Some(dependencies) = dependencies.and_then(Item::as_table_like) else {
        return;
    };
    for (name, dependency) in dependencies.iter() {
        if dependency.get("workspace").and_then(Item::as_bool) != Some(true) {
            continue;
        }
        let Some(workspace_dependency) = workspace_dependencies.get(name) else {
            continue;
        };
        collect_publish_dependency_issue(name, workspace_dependency, issues);
    }
}

fn collect_inherited_target_publish_dependency_issues(
    document: &DocumentMut,
    workspace_dependencies: &dyn toml_edit::TableLike,
    issues: &mut Vec<String>,
) {
    let Some(targets) = document.get("target").and_then(Item::as_table_like) else {
        return;
    };
    for (_target, target) in targets.iter() {
        collect_inherited_publish_dependency_section_issues(
            target.get("dependencies"),
            workspace_dependencies,
            issues,
        );
        collect_inherited_publish_dependency_section_issues(
            target.get("build-dependencies"),
            workspace_dependencies,
            issues,
        );
        collect_inherited_publish_dependency_section_issues(
            target.get("dev-dependencies"),
            workspace_dependencies,
            issues,
        );
    }
}
