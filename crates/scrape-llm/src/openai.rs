//! `OpenAI`-compatible chat completions provider.

use async_trait::async_trait;
use serde_json::{json, Value};
use url::Url;

use scrape_core::{Result, ScrapeError};
use scrape_fetch::Fetcher;

use crate::chat::{ChatModel, ChatRequest};
use crate::util;

/// A provider speaking the `OpenAI` `/chat/completions` protocol.
///
/// This covers `OpenAI` itself and compatible gateways (`vLLM`, `LiteLLM`,
/// `OpenRouter`, `LM Studio`, etc.). Authentication is supplied through the
/// [`Fetcher`]'s headers so the key never enters this crate.
pub struct OpenAiCompat {
    http: Fetcher,
    endpoint: Url,
    model: String,
    json_mode: bool,
}

impl OpenAiCompat {
    /// Builds a provider from `base_url` (e.g. `https://api.openai.com/v1`).
    ///
    /// # Errors
    /// Returns [`ScrapeError::Config`] for an empty model and
    /// [`ScrapeError::UrlNotAllowed`] for an invalid or credentialed base URL.
    pub fn new(http: Fetcher, base_url: &str, model: impl Into<String>) -> Result<Self> {
        let model = model.into();
        if model.trim().is_empty() {
            return Err(ScrapeError::Config("LLM model must not be empty".into()));
        }
        Ok(Self {
            http,
            endpoint: util::endpoint(base_url, "chat/completions")?,
            model,
            json_mode: true,
        })
    }

    /// Enables or disables the `response_format: json_object` hint.
    #[must_use]
    pub fn with_json_mode(mut self, enabled: bool) -> Self {
        self.json_mode = enabled;
        self
    }

    fn request_body(&self, request: &ChatRequest) -> Value {
        let mut body = json!({
            "model": self.model,
            "messages": [
                { "role": "system", "content": request.system },
                { "role": "user", "content": request.user },
            ],
            "temperature": request.temperature,
        });
        if let Some(max_tokens) = request.max_tokens {
            body["max_tokens"] = json!(max_tokens);
        }
        if self.json_mode {
            body["response_format"] = json!({ "type": "json_object" });
        }
        body
    }
}

#[async_trait]
impl ChatModel for OpenAiCompat {
    fn provider(&self) -> &'static str {
        "openai-compatible"
    }

    fn model(&self) -> &str {
        &self.model
    }

    async fn complete(&self, request: &ChatRequest) -> Result<String> {
        let body = self.request_body(request);
        let response = self.http.post_json(&self.endpoint, &body).await?;
        if !(200..300).contains(&response.status) {
            return Err(ScrapeError::Llm(format!(
                "{} returned HTTP {}: {}",
                self.provider(),
                response.status,
                util::truncate(&response.body, 512)
            )));
        }
        let parsed: Value = serde_json::from_str(&response.body).map_err(|err| {
            ScrapeError::Llm(format!("{} returned invalid JSON: {err}", self.provider()))
        })?;
        let content = parsed
            .pointer("/choices/0/message/content")
            .ok_or_else(|| {
                ScrapeError::Llm(format!(
                    "{} response is missing choices[0].message.content",
                    self.provider()
                ))
            })?;
        util::content_text(content).ok_or_else(|| {
            ScrapeError::Llm(format!(
                "{} response contained no text content",
                self.provider()
            ))
        })
    }
}
