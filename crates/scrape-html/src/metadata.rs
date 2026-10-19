//! Deterministic document metadata extraction.

use scraper::{Html, Selector};
use url::Url;

use scrape_core::Metadata;

use crate::canonical::canonicalize;
use crate::text::normalize_whitespace;

fn first_text(doc: &Html, css: &str) -> Option<String> {
    let selector = Selector::parse(css).ok()?;
    let element = doc.select(&selector).next()?;
    let text = normalize_whitespace(&element.text().collect::<Vec<_>>().join(" "));
    (!text.is_empty()).then_some(text)
}

fn content_attr(doc: &Html, css: &str) -> Option<String> {
    let selector = Selector::parse(css).ok()?;
    let element = doc.select(&selector).next()?;
    let value = element.value().attr("content")?;
    let value = normalize_whitespace(value);
    (!value.is_empty()).then_some(value)
}

fn attr(doc: &Html, css: &str, name: &str) -> Option<String> {
    let selector = Selector::parse(css).ok()?;
    let element = doc.select(&selector).next()?;
    let value = element.value().attr(name)?;
    let value = normalize_whitespace(value);
    (!value.is_empty()).then_some(value)
}

/// Extracts title, description, canonical URL, language and author.
pub fn extract(doc: &Html, base: &Url) -> Metadata {
    let description = content_attr(doc, "meta[name=\"description\"]")
        .or_else(|| content_attr(doc, "meta[property=\"og:description\"]"));
    let author = content_attr(doc, "meta[name=\"author\"]")
        .or_else(|| content_attr(doc, "meta[property=\"article:author\"]"));
    let canonical = attr(doc, "link[rel=\"canonical\"]", "href")
        .and_then(|href| base.join(href.trim()).ok())
        .map(|url| canonicalize(&url));

    Metadata {
        title: first_text(doc, "title"),
        description,
        canonical,
        language: attr(doc, "html", "lang"),
        author,
    }
}

/// Convenience helper exposing the raw `title` element text.
pub fn title(doc: &Html) -> Option<String> {
    first_text(doc, "title")
}

/// Convenience helper exposing the `description` meta value.
pub fn description(doc: &Html) -> Option<String> {
    content_attr(doc, "meta[name=\"description\"]")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> Url {
        Url::parse("https://example.com/dir/page.html").unwrap()
    }

    #[test]
    fn extracts_common_metadata() {
        let html = r#"
          <html lang="en">
            <head>
              <title>  Example   Page </title>
              <meta name="description" content="A description.">
              <meta name="author" content="Ada Lovelace">
              <link rel="canonical" href="/canonical#frag">
            </head>
            <body></body>
          </html>"#;
        let doc = Html::parse_document(html);
        let meta = extract(&doc, &base());
        assert_eq!(meta.title.as_deref(), Some("Example Page"));
        assert_eq!(meta.description.as_deref(), Some("A description."));
        assert_eq!(meta.author.as_deref(), Some("Ada Lovelace"));
        assert_eq!(meta.language.as_deref(), Some("en"));
        assert_eq!(
            meta.canonical.as_ref().map(Url::as_str),
            Some("https://example.com/canonical")
        );
    }

    #[test]
    fn falls_back_to_og_description() {
        let html =
            r#"<html><head><meta property="og:description" content="og desc"></head></html>"#;
        let doc = Html::parse_document(html);
        let meta = extract(&doc, &base());
        assert_eq!(meta.description.as_deref(), Some("og desc"));
    }

    #[test]
    fn missing_metadata_is_none() {
        let doc = Html::parse_document("<html><body></body></html>");
        let meta = extract(&doc, &base());
        assert!(meta.title.is_none());
        assert!(meta.description.is_none());
        assert!(meta.canonical.is_none());
    }
}
