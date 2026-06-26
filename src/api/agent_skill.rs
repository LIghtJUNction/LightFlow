pub(crate) fn agent_skill_issues(source: &str, workflow_id: &str) -> Vec<String> {
    let mut issues = Vec::new();
    if !agent_skill_has_frontmatter(source) {
        issues.push("frontmatter with name, description, and version".to_owned());
    }
    if !source.contains(workflow_id) {
        issues.push(format!("workflow id `{workflow_id}`"));
    }
    if !source.contains("lfw run") {
        issues.push("CLI `lfw run` example".to_owned());
    }
    let http_run_path = format!("/workflows/{workflow_id}/run");
    if !source.contains(&http_run_path) {
        issues.push(format!("HTTP `{http_run_path}` example"));
    }
    issues
}

fn agent_skill_has_frontmatter(source: &str) -> bool {
    let mut lines = source.lines();
    if lines.next() != Some("---") {
        return false;
    }
    let mut has_name = false;
    let mut has_description = false;
    let mut has_version = false;
    for line in lines {
        if line == "---" {
            return has_name && has_description && has_version;
        }
        has_name |= line.starts_with("name:");
        has_description |= line.starts_with("description:");
        has_version |= line.starts_with("version:");
    }
    false
}
