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

pub(super) fn output_tail(output: &[u8]) -> Option<String> {
    if output.is_empty() {
        return None;
    }
    let text = String::from_utf8_lossy(output);
    let mut lines = text.lines().rev().take(20).collect::<Vec<_>>();
    lines.reverse();
    Some(lines.join("\n"))
}
