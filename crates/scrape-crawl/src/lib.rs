//! # scrape-crawl
//!
//! Legacy bounded, breadth-first crawler for `ScrapeOwl`.
//!
//! Kept as a compatibility fallback with bug fixes only. Use `MudFish` for new
//! crawling and browser-rendering work.
//!
//! The crawler reuses the SSRF-guarded [`scrape_fetch`] client and deterministic
//! [`scrape_html`] extractor. Limits come from [`scrape_core::CrawlPolicy`]:
//! page count, link depth, same-origin scope, concurrency, crawl delay and
//! optional `robots.txt` enforcement. Individual page failures are reported,
//! not fatal.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod crawl;
mod robots;

pub use crawl::{CrawlFailure, CrawlFailureKind, CrawlPage, CrawlReport, Crawler};
