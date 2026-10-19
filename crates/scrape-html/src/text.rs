//! Deterministic main-content text extraction.

use ego_tree::NodeRef;
use scraper::node::Node;
use scraper::{ElementRef, Html, Selector};

/// Elements whose text is never part of the main content.
const NOISE_TAGS: &[&str] = &[
    "script", "style", "noscript", "template", "svg", "canvas", "iframe", "nav", "footer",
    "header", "aside", "form",
];

/// Candidate containers, most specific first. The one with the most text wins.
const CANDIDATES: &[&str] = &[
    "article",
    "main",
    "[role=\"main\"]",
    "[role=\"article\"]",
    "#content",
    ".content",
    "body",
];

fn is_noise(node: &Node) -> bool {
    node.as_element()
        .is_some_and(|element| NOISE_TAGS.contains(&element.name()))
}

fn collect(node: NodeRef<'_, Node>, out: &mut String) {
    match node.value() {
        Node::Text(text) => {
            out.push_str(&text.text);
            out.push(' ');
        }
        Node::Element(_) => {
            if is_noise(node.value()) {
                return;
            }
            for child in node.children() {
                collect(child, out);
            }
        }
        _ => {}
    }
}

fn element_text(element: &ElementRef<'_>) -> String {
    let mut out = String::new();
    collect(**element, &mut out);
    normalize_whitespace(&out)
}

/// Collapses runs of whitespace into single spaces and trims the result.
pub fn normalize_whitespace(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut last_space = true;
    for ch in input.chars() {
        if ch.is_whitespace() {
            if !last_space {
                out.push(' ');
                last_space = true;
            }
        } else {
            out.push(ch);
            last_space = false;
        }
    }
    out.trim_end().to_string()
}

/// Extracts the main text of a document using a deterministic container
/// heuristic. No JavaScript is executed and no network is used.
pub fn main_text(doc: &Html) -> String {
    let mut best = String::new();
    for candidate in CANDIDATES {
        let Ok(selector) = Selector::parse(candidate) else {
            continue;
        };
        for element in doc.select(&selector) {
            let text = element_text(&element);
            if text.len() > best.len() {
                best = text;
            }
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_article_over_body_and_drops_noise() {
        let html = r"
            <html><body>
              <nav>navigation links</nav>
              <article><h1>Real title</h1><p>The actual article body text.</p></article>
              <footer>footer text</footer>
              <script>var secret = 1;</script>
            </body></html>";
        let doc = Html::parse_document(html);
        let text = main_text(&doc);
        assert!(text.contains("Real title"));
        assert!(text.contains("actual article body"));
        assert!(!text.contains("navigation"));
        assert!(!text.contains("footer text"));
        assert!(!text.contains("secret"));
    }

    #[test]
    fn falls_back_to_body_when_no_main_container() {
        let html = "<html><body><div>plain content here</div></body></html>";
        let doc = Html::parse_document(html);
        assert_eq!(main_text(&doc), "plain content here");
    }

    #[test]
    fn whitespace_is_collapsed() {
        assert_eq!(normalize_whitespace("  a\n\n   b\tc  "), "a b c");
    }
}
