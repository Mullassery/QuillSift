//! # quillsift-crawl
//!
//! Legacy bounded, breadth-first crawler for `QuillSift`.
//!
//! Kept as a compatibility fallback with bug fixes only. Use `MudFish` for new
//! crawling and browser-rendering work.
//!
//! The crawler reuses the SSRF-guarded [`quillsift_fetch`] client and deterministic
//! [`quillsift_html`] extractor. Limits come from [`quillsift_core::CrawlPolicy`]:
//! page count, link depth, same-origin scope, concurrency, crawl delay and
//! optional `robots.txt` enforcement. Individual page failures are reported,
//! not fatal.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod crawl;
mod robots;

pub use crawl::{CrawlFailure, CrawlFailureKind, CrawlPage, CrawlReport, Crawler};
