//! Small text helpers shared by prompting and diagnostics.

/// Truncates to at most `limit` characters, appending `...` when shortened.
pub(crate) fn truncate(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_owned();
    }
    let mut out: String = text.chars().take(limit).collect();
    out.push_str("...");
    out
}

/// Bounds untrusted input and marks it when truncated.
pub(crate) fn bounded_input(content: &str, limit: usize) -> String {
    if content.chars().count() <= limit {
        return content.to_owned();
    }
    let mut out: String = content.chars().take(limit).collect();
    out.push_str("\n[TRUNCATED]");
    out
}
