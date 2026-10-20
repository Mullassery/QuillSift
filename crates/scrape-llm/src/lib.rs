//! # scrape-llm
//!
//! Small, provider-agnostic chat adapters used by `ScrapeOwl` extraction.
//!
//! Supported backends:
//! - `OpenAI`-compatible endpoints (`/chat/completions`).
//! - `Ollama` (`/api/chat`).
//!
//! HTTP goes through [`scrape_fetch::Fetcher`], so SSRF, timeout and response
//! size policies apply to model endpoints too. Credentials are supplied as
//! request headers (redacted from `Debug` output), never embedded in URLs.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod chat;
mod ollama;
mod openai;
mod util;

pub use chat::{ChatModel, ChatRequest};
pub use ollama::Ollama;
pub use openai::OpenAiCompat;

use scrape_core::Result;
use scrape_fetch::Fetcher;

/// Supported provider families.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderKind {
    /// `OpenAI` `/chat/completions` protocol.
    OpenAiCompatible,
    /// `Ollama` `/api/chat` protocol.
    Ollama,
}

/// Declarative provider selection.
#[derive(Debug, Clone)]
pub struct ProviderConfig {
    /// Which protocol to speak.
    pub kind: ProviderKind,
    /// Protocol base URL (no credentials).
    pub base_url: String,
    /// Model identifier.
    pub model: String,
}

impl ProviderConfig {
    /// Creates an `OpenAI`-compatible configuration.
    pub fn openai_compatible(base_url: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            kind: ProviderKind::OpenAiCompatible,
            base_url: base_url.into(),
            model: model.into(),
        }
    }

    /// Creates an `Ollama` configuration.
    pub fn ollama(base_url: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            kind: ProviderKind::Ollama,
            base_url: base_url.into(),
            model: model.into(),
        }
    }
}

/// Builds the provider selected by `config`.
///
/// # Errors
/// Propagates provider construction errors (invalid URL or empty model).
pub fn build_provider(http: Fetcher, config: &ProviderConfig) -> Result<Box<dyn ChatModel>> {
    match config.kind {
        ProviderKind::OpenAiCompatible => Ok(Box::new(OpenAiCompat::new(
            http,
            &config.base_url,
            config.model.clone(),
        )?)),
        ProviderKind::Ollama => Ok(Box::new(Ollama::new(
            http,
            &config.base_url,
            config.model.clone(),
        )?)),
    }
}
