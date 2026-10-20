//! `Ollama` chat provider.

use async_trait::async_trait;
use serde_json::{json, Value};
use url::Url;

use scrape_core::{Result, ScrapeError};
use scrape_fetch::Fetcher;

use crate::chat::{ChatModel, ChatRequest};
use crate::util;

/// A local [`Ollama`](https://ollama.com) chat provider.
///
/// `Ollama` usually listens on loopback, so the [`Fetcher`] passed here must be
/// built with private-network access explicitly enabled.
pub struct Ollama {
    http: Fetcher,
    endpoint: Url,
    model: String,
    json_mode: bool,
}

impl Ollama {
    /// Builds a provider from `base_url` (e.g. `http://localhost:11434`).
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
            endpoint: util::endpoint(base_url, "api/chat")?,
            model,
            json_mode: true,
        })
    }

    /// Enables or disables `Ollama`'s JSON output format.
    #[must_use]
    pub fn with_json_mode(mut self, enabled: bool) -> Self {
        self.json_mode = enabled;
        self
    }

    fn request_body(&self, request: &ChatRequest) -> Value {
        let mut options = json!({ "temperature": request.temperature });
        if let Some(max_tokens) = request.max_tokens {
            options["num_predict"] = json!(max_tokens);
        }
        let mut body = json!({
            "model": self.model,
            "messages": [
                { "role": "system", "content": request.system },
                { "role": "user", "content": request.user },
            ],
            "stream": false,
            "options": options,
        });
        if self.json_mode {
            body["format"] = json!("json");
        }
        body
    }
}

#[async_trait]
impl ChatModel for Ollama {
    fn provider(&self) -> &'static str {
        "ollama"
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
        parsed
            .pointer("/message/content")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| {
                ScrapeError::Llm(format!(
                    "{} response is missing message.content",
                    self.provider()
                ))
            })
    }
}
