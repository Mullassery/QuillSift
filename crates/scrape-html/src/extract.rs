//! The deterministic [`Extractor`] implementation.

use scraper::Html;
use url::Url;

use scrape_core::{Extracted, Extractor, Result};

use crate::{canonical, links, markdown, metadata, text};

/// Extracts content from HTML without any LLM or network access.
#[derive(Debug, Default, Clone, Copy)]
pub struct StructuralExtractor;

impl StructuralExtractor {
    /// Creates a new extractor.
    pub fn new() -> Self {
        Self
    }
}

impl Extractor for StructuralExtractor {
    fn extract(&self, html: &str, base: &Url) -> Result<Extracted> {
        let doc = Html::parse_document(html);
        let metadata = metadata::extract(&doc, base);
        let (links, images) = links::extract(&doc, base);
        let text = text::main_text(&doc);
        let markdown = markdown::to_markdown(html)?;

        Ok(Extracted {
            url: canonical::canonicalize(base),
            text,
            markdown,
            links,
            images,
            metadata,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_everything_in_one_pass() {
        let html = r#"<!doctype html>
          <html lang="en">
            <head>
              <title>Sample</title>
              <meta name="description" content="Sample page">
              <link rel="canonical" href="https://example.com/canonical">
            </head>
            <body>
              <main>
                <h1>Sample heading</h1>
                <p>Body text with a <a href="/link">link</a>.</p>
                <img src="/pic.png">
              </main>
              <script>ignore()</script>
            </body>
          </html>"#;
        let base = Url::parse("https://example.com/page").unwrap();
        let extracted = StructuralExtractor::new().extract(html, &base).unwrap();

        assert_eq!(extracted.metadata.title.as_deref(), Some("Sample"));
        assert_eq!(
            extracted.metadata.canonical.as_ref().map(Url::as_str),
            Some("https://example.com/canonical")
        );
        assert!(extracted.text.contains("Sample heading"));
        assert!(!extracted.text.contains("ignore"));
        assert!(extracted.markdown.contains("# Sample heading"));
        assert_eq!(extracted.links.len(), 1);
        assert_eq!(extracted.links[0].as_str(), "https://example.com/link");
        assert_eq!(extracted.images.len(), 1);
        assert_eq!(extracted.url.as_str(), "https://example.com/page");
    }

    #[test]
    fn malformed_html_does_not_panic_and_drops_unsafe_links() {
        let html = "<html><body><p>unclosed <div><a href='javascript:alert(1)'>x</a>\
                    <a href='/ok'>ok</a> & <img src='data:image/png;base64,AA'>";
        let base = Url::parse("https://example.com/").unwrap();
        let extracted = StructuralExtractor::new().extract(html, &base).unwrap();

        assert_eq!(extracted.links.len(), 1);
        assert_eq!(extracted.links[0].as_str(), "https://example.com/ok");
        assert_eq!(extracted.images.len(), 0);
    }
}
