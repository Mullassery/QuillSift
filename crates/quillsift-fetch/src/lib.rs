//! # quillsift-fetch
//!
//! SSRF-guarded HTTP fetching for `QuillSift`.
//!
//! Every request is validated against [`quillsift_core::FetchPolicy`]: only
//! `http`/`https` are allowed, loopback/private/link-local/cloud-metadata
//! destinations are rejected (unless explicitly enabled for trusted local
//! deployments), redirects are followed manually with per-hop validation, and
//! response size and time are bounded.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod dns;
mod fetch;
mod guard;

pub use fetch::{Fetcher, HttpResponse};
pub use guard::ensure_allowed;
