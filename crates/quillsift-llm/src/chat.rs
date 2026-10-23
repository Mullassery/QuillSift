//! Provider-neutral chat types.

use async_trait::async_trait;

use quillsift_core::Result;

/// A single, stateless chat completion request.
///
/// `system` is trusted policy supplied by the application; `user` may contain
/// untrusted scraped content and must be treated as data.
#[derive(Clone)]
pub struct ChatRequest {
    /// Fixed system instructions.
    pub system: String,
    /// User content (possibly untrusted).
    pub user: String,
    /// Sampling temperature; `0.0` for deterministic extraction.
    pub temperature: f32,
    /// Optional output token cap.
    pub max_tokens: Option<u32>,
}

impl ChatRequest {
    /// Creates a request with temperature `0.0`.
    pub fn new(system: impl Into<String>, user: impl Into<String>) -> Self {
        Self {
            system: system.into(),
            user: user.into(),
            temperature: 0.0,
            max_tokens: None,
        }
    }

    /// Sets the sampling temperature.
    #[must_use]
    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.temperature = temperature;
        self
    }

    /// Sets the output token cap.
    #[must_use]
    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = Some(max_tokens);
        self
    }
}

/// A chat completion backend.
#[async_trait]
pub trait ChatModel: Send + Sync {
    /// Provider family, e.g. `openai-compatible` or `ollama`.
    fn provider(&self) -> &'static str;

    /// Model identifier sent to the provider.
    fn model(&self) -> &str;

    /// Runs one completion and returns the assistant text.
    ///
    /// # Errors
    /// Returns [`quillsift_core::ScrapeError::Llm`] for provider-side failures and
    /// transport errors from the underlying HTTP client.
    async fn complete(&self, request: &ChatRequest) -> Result<String>;
}

#[async_trait]
impl ChatModel for Box<dyn ChatModel> {
    fn provider(&self) -> &'static str {
        (**self).provider()
    }

    fn model(&self) -> &str {
        (**self).model()
    }

    async fn complete(&self, request: &ChatRequest) -> Result<String> {
        (**self).complete(request).await
    }
}
