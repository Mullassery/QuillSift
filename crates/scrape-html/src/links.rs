//! Link and image extraction with absolute-URL resolution.

use std::collections::HashSet;

use scraper::{Html, Selector};
use url::Url;

use crate::canonical::canonicalize;

fn resolve(base: &Url, raw: &str) -> Option<Url> {
    // `href`/`src` may contain multiple space-separated URLs (e.g. `srcset`);
    // use only the first token.
    let first = raw.split_whitespace().next().unwrap_or("");
    let joined = base.join(first).ok()?;
    match joined.scheme() {
        "http" | "https" => Some(canonicalize(&joined)),
        _ => None,
    }
}

fn collect(doc: &Html, css: &str, attr: &str, base: &Url) -> Vec<Url> {
    let mut urls = Vec::new();
    let mut seen = HashSet::new();
    let Ok(selector) = Selector::parse(css) else {
        return urls;
    };
    for element in doc.select(&selector) {
        let Some(raw) = element.value().attr(attr) else {
            continue;
        };
        if let Some(url) = resolve(base, raw) {
            if seen.insert(url.clone()) {
                urls.push(url);
            }
        }
    }
    urls
}

/// Extracts deduplicated links (`a[href]`) and images (`img[src]`).
///
/// Non-HTTP(S) schemes (`mailto:`, `javascript:`, `data:`, `tel:`, …) are
/// dropped. Relative URLs are resolved against `base`.
pub fn extract(doc: &Html, base: &Url) -> (Vec<Url>, Vec<Url>) {
    (
        collect(doc, "a[href]", "href", base),
        collect(doc, "img[src]", "src", base),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> Url {
        Url::parse("https://example.com/dir/").unwrap()
    }

    #[test]
    fn resolves_and_dedupes_links() {
        let html = r#"
          <a href="/a">a</a>
          <a href="/a#frag">a again</a>
          <a href="page.html">relative</a>
          <a href="mailto:x@example.com">mail</a>
          <a href="javascript:void(0)">js</a>
          <a href="https://other.example.com/x">other</a>"#;
        let doc = Html::parse_document(html);
        let (links, images) = extract(&doc, &base());
        let links: Vec<&str> = links.iter().map(Url::as_str).collect();
        assert!(links.contains(&"https://example.com/a"));
        assert!(links.contains(&"https://example.com/dir/page.html"));
        assert!(links.contains(&"https://other.example.com/x"));
        assert!(!links.iter().any(|u| u.starts_with("mailto:")));
        assert!(!links.iter().any(|u| u.starts_with("javascript:")));
        assert_eq!(
            links
                .iter()
                .filter(|u| **u == "https://example.com/a")
                .count(),
            1
        );
        assert_eq!(images.len(), 0);
    }

    #[test]
    fn extracts_images() {
        let html = r#"<img src="/img/a.png"><img src="data:image/png;base64,AAAA">"#;
        let doc = Html::parse_document(html);
        let (_, images) = extract(&doc, &base());
        assert_eq!(images.len(), 1);
        assert_eq!(images[0].as_str(), "https://example.com/img/a.png");
    }
}
