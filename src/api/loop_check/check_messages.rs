pub(super) fn summarize_messages(messages: &[String], limit: usize) -> String {
    let sample = messages.iter().take(limit).cloned().collect::<Vec<_>>();
    if messages.len() > sample.len() {
        format!(
            "{} and {} more",
            sample.join("; "),
            messages.len() - sample.len()
        )
    } else {
        sample.join("; ")
    }
}

pub(super) fn patch_validation_summary(name: &str, issues: &[String]) -> String {
    if issues.is_empty() {
        return name.to_owned();
    }
    format!("{name}: {}", summarize_messages(issues, 2))
}
