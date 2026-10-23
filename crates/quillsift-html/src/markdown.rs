//! HTML to Markdown conversion.

use quillsift_core::{Result, ScrapeError};

/// Converts an HTML document to Markdown.
///
/// # Errors
/// Returns [`ScrapeError::Parse`] if the converter fails.
pub fn to_markdown(html: &str) -> Result<String> {
    htmd::convert(html)
        .map_err(|err| ScrapeError::Parse(format!("markdown conversion failed: {err}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_headings_and_links() {
        let md =
            to_markdown(r#"<h1>Title</h1><p>Hello <a href="https://example.com">world</a></p>"#)
                .unwrap();
        assert!(md.contains("# Title"));
        assert!(md.contains("[world](https://example.com)"));
    }

    #[test]
    fn converts_lists() {
        let md = to_markdown("<ul><li>one</li><li>two</li></ul>").unwrap();
        assert!(md.contains("one"));
        assert!(md.contains("two"));
    }
}
