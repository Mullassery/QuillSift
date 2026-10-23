//! Domain types shared across `QuillSift` crates.

use std::time::SystemTime;

use serde::{Deserialize, Serialize};
use url::Url;

/// A successfully fetched document.
#[derive(Debug, Clone)]
pub struct Document {
    /// The final URL after redirects.
    pub url: Url,
    /// The HTTP status code.
    pub status: u16,
    /// The `Content-Type` header, when present.
    pub content_type: Option<String>,
    /// The document body (HTML or other text).
    pub html: String,
    /// When the document was fetched.
    pub fetched_at: SystemTime,
}

impl Document {
    /// Creates a new document.
    pub fn new(url: Url, status: u16, content_type: Option<String>, html: String) -> Self {
        Self {
            url,
            status,
            content_type,
            html,
            fetched_at: SystemTime::now(),
        }
    }
}

/// Deterministic, LLM-free metadata extracted from a document.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Metadata {
    /// The document `<title>`.
    pub title: Option<String>,
    /// The `meta[name=description]` value.
    pub description: Option<String>,
    /// The declared canonical URL, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub canonical: Option<Url>,
    /// The document language (`<html lang>`), if present.
    pub language: Option<String>,
    /// The declared author, if present.
    pub author: Option<String>,
}

/// The result of deterministically processing a document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Extracted {
    /// The URL the content came from.
    pub url: Url,
    /// Normalized visible text.
    pub text: String,
    /// Markdown rendering of the content.
    pub markdown: String,
    /// Outbound links, absolutized against `url`.
    pub links: Vec<Url>,
    /// Image URLs, absolutized against `url`.
    pub images: Vec<Url>,
    /// Deterministic metadata.
    pub metadata: Metadata,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn document_records_fetch_time() {
        let url = Url::parse("https://example.com/").unwrap();
        let doc = Document::new(url, 200, Some("text/html".into()), "<html></html>".into());
        assert_eq!(doc.status, 200);
        assert!(doc.fetched_at <= SystemTime::now());
    }

    #[test]
    fn metadata_omits_absent_canonical_when_serialized() {
        let meta = Metadata::default();
        let json = serde_json::to_string(&meta).unwrap();
        assert!(!json.contains("canonical"));
        assert!(json.contains("\"title\":null"));
    }

    #[test]
    fn metadata_roundtrips() {
        let meta = Metadata {
            title: Some("Example".into()),
            description: Some("desc".into()),
            canonical: Some(Url::parse("https://example.com/").unwrap()),
            language: Some("en".into()),
            author: None,
        };
        let json = serde_json::to_string(&meta).unwrap();
        let back: Metadata = serde_json::from_str(&json).unwrap();
        assert_eq!(meta, back);
    }
}
