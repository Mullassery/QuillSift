//! # scrape-core
//!
//! Core domain types, configuration policies, errors and traits for `ScrapeOwl`.
//!
//! This crate contains no network, parsing or LLM logic. It is depended on by
//! every other crate so that limits and error contracts have a single source of
//! truth.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod config;
pub mod error;
pub mod net;
pub mod traits;
pub mod types;

pub use config::{default_user_agent, CrawlPolicy, FetchPolicy, RetryPolicy};
pub use error::{ErrorKind, Result, ScrapeError};
pub use net::{block_reason, classify, is_cloud_metadata, normalize, AddressClass};
pub use traits::Extractor;
pub use types::{Document, Extracted, Metadata};
