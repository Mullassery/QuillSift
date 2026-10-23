//! # quillsift-extract
//!
//! Schema-validated LLM extraction for `QuillSift`.
//!
//! The document is treated as untrusted data: it is truncated to a bound and
//! wrapped in fences inside the user message, while the system message holds
//! only fixed policy and the caller-supplied JSON Schema. Model output must
//! parse as JSON and validate against the schema; invalid output is repaired a
//! bounded number of times.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod extractor;
mod prompt;
mod text;
mod validate;

pub use extractor::{ExtractionConfig, LlmExtractor};
pub use prompt::DOCUMENT_FENCE;
