//! # scrape-html
//!
//! Deterministic HTML processing for `ScrapeOwl`: metadata, main-content text,
//! Markdown, links and images. No network access and no JavaScript execution.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod canonical;
pub mod extract;
pub mod links;
pub mod markdown;
pub mod metadata;
pub mod text;

pub use canonical::canonicalize;
pub use extract::StructuralExtractor;
pub use text::normalize_whitespace;
