//! Traits implemented by the concrete crates.
//!
//! These are intentionally small and object-safe so pipelines can be assembled
//! and tested with mock implementations.

use url::Url;

use crate::error::Result;
use crate::types::Extracted;

/// Deterministically extracts content from an HTML document.
///
/// Implementations must be pure with respect to the network: no fetching is
/// performed here.
pub trait Extractor {
    /// Extracts text, markdown, links, images and metadata from `html`,
    /// resolving relative references against `base`.
    ///
    /// # Errors
    /// Returns [`crate::error::ScrapeError::Parse`] when the input cannot be
    /// processed.
    fn extract(&self, html: &str, base: &Url) -> Result<Extracted>;
}
