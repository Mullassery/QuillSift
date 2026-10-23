//! Shared provider helpers.

use serde_json::Value;
use url::Url;

use quillsift_core::{Result, ScrapeError};

/// Joins `path` onto a base URL, ensuring the last base segment is preserved.
pub(crate) fn endpoint(base_url: &str, path: &str) -> Result<Url> {
    let trimmed = base_url.trim();
    if trimmed.is_empty() {
        return Err(ScrapeError::Config("LLM base URL is empty".into()));
    }
    let mut normalized = trimmed.to_owned();
    if !normalized.ends_with('/') {
        normalized.push('/');
    }
    let base = Url::parse(&normalized)?;
    match base.scheme() {
        "http" | "https" => {}
        other => {
            return Err(ScrapeError::UrlNotAllowed(format!(
                "unsupported LLM URL scheme: {other}"
            )));
        }
    }
    if !base.username().is_empty() || base.password().is_some() {
        return Err(ScrapeError::UrlNotAllowed(
            "LLM base URL must not contain credentials; use headers".into(),
        ));
    }
    Ok(base.join(path)?)
}

/// Truncates `text` to at most `limit` characters (not bytes), appending `...`.
pub(crate) fn truncate(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_owned();
    }
    let mut out: String = text.chars().take(limit).collect();
    out.push_str("...");
    out
}

/// Extracts assistant text from `OpenAI`-style `content`, which may be a string
/// or an array of `{ "type": "text", "text": "..." }` parts.
pub(crate) fn content_text(content: &Value) -> Option<String> {
    if let Some(text) = content.as_str() {
        return Some(text.to_owned());
    }
    let parts = content.as_array()?;
    let mut out = String::new();
    for part in parts {
        if let Some(text) = part.get("text").and_then(Value::as_str) {
            out.push_str(text);
        }
    }
    (!out.is_empty()).then_some(out)
}
