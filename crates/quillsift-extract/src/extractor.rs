//! Schema-validated extraction with bounded repair.

use serde_json::Value;

use quillsift_core::{Result, ScrapeError};
use quillsift_llm::{ChatModel, ChatRequest};

use crate::prompt;
use crate::text::{bounded_input, truncate};
use crate::validate::{parse_candidate, SchemaValidator};

/// Bounds applied to an extraction run.
#[derive(Debug, Clone)]
pub struct ExtractionConfig {
    /// Total model calls allowed, including the first attempt (minimum 1).
    pub max_attempts: u32,
    /// Sampling temperature; `0.0` for deterministic output.
    pub temperature: f32,
    /// Maximum document characters sent to the model.
    pub max_input_chars: usize,
    /// Optional output token cap.
    pub max_tokens: Option<u32>,
}

impl Default for ExtractionConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            temperature: 0.0,
            max_input_chars: 100_000,
            max_tokens: None,
        }
    }
}

impl ExtractionConfig {
    /// Validates the configuration.
    ///
    /// # Errors
    /// Returns [`ScrapeError::Config`] for zero attempts/input or an invalid
    /// temperature.
    pub fn validate(&self) -> Result<()> {
        if self.max_attempts == 0 {
            return Err(ScrapeError::Config(
                "max_attempts must be greater than zero".into(),
            ));
        }
        if self.max_input_chars == 0 {
            return Err(ScrapeError::Config(
                "max_input_chars must be greater than zero".into(),
            ));
        }
        if !self.temperature.is_finite() || self.temperature < 0.0 {
            return Err(ScrapeError::Config(
                "temperature must be a finite, non-negative number".into(),
            ));
        }
        Ok(())
    }
}

/// Extracts schema-shaped JSON from untrusted content using a [`ChatModel`].
///
/// The system policy is fixed and the document is fenced as data. If the first
/// response is not valid JSON or fails the schema, a bounded repair prompt is
/// sent that includes only the validation error and the previous output.
pub struct LlmExtractor<M: ChatModel> {
    model: M,
    schema: SchemaValidator,
    config: ExtractionConfig,
}

impl<M: ChatModel> LlmExtractor<M> {
    /// Builds an extractor for a JSON Schema document.
    ///
    /// # Errors
    /// Returns [`ScrapeError::Config`] for an invalid config or schema.
    pub fn new(model: M, schema: Value, config: ExtractionConfig) -> Result<Self> {
        config.validate()?;
        Ok(Self {
            model,
            schema: SchemaValidator::new(schema)?,
            config,
        })
    }

    /// Builds an extractor from a Rust type implementing [`schemars::JsonSchema`].
    ///
    /// # Errors
    /// Returns [`ScrapeError::Config`] if the schema cannot be produced or
    /// compiled.
    pub fn from_schemars<T: schemars::JsonSchema>(
        model: M,
        config: ExtractionConfig,
    ) -> Result<Self> {
        let schema = serde_json::to_value(schemars::schema_for!(T))
            .map_err(|err| ScrapeError::Config(format!("failed to serialize schema: {err}")))?;
        Self::new(model, schema, config)
    }

    /// Returns the compiled schema document.
    pub fn schema(&self) -> &Value {
        self.schema.schema()
    }

    /// Returns the extraction configuration.
    pub fn config(&self) -> &ExtractionConfig {
        &self.config
    }

    /// Returns the underlying model.
    pub fn model(&self) -> &M {
        &self.model
    }

    /// Builds the initial prompt for `content` without calling the model.
    ///
    /// Exposed so callers and tests can inspect the trust boundary.
    pub fn prompt(&self, content: &str) -> ChatRequest {
        let input = bounded_input(content, self.config.max_input_chars);
        let request = prompt::build(self.schema.schema(), &input);
        self.apply(request)
    }

    /// Runs extraction, repairing invalid output up to the configured limit.
    ///
    /// # Errors
    /// Returns the model's error immediately, or
    /// [`ScrapeError::Extraction`] when all attempts fail validation.
    pub async fn extract(&self, content: &str) -> Result<Value> {
        let mut request = self.prompt(content);
        let mut last_error = String::new();

        for attempt in 1..=self.config.max_attempts {
            let raw = self.model.complete(&request).await?;
            match parse_candidate(&raw).and_then(|value| {
                self.schema.validate(&value)?;
                Ok(value)
            }) {
                Ok(value) => return Ok(value),
                Err(err) => {
                    last_error = err.to_string();
                    if attempt < self.config.max_attempts {
                        request = self.repair_request(&raw, &last_error);
                    }
                }
            }
        }

        Err(ScrapeError::Extraction(format!(
            "extraction failed after {} attempt(s): {last_error}",
            self.config.max_attempts
        )))
    }

    fn apply(&self, mut request: ChatRequest) -> ChatRequest {
        request = request.with_temperature(self.config.temperature);
        if let Some(max_tokens) = self.config.max_tokens {
            request = request.with_max_tokens(max_tokens);
        }
        request
    }

    fn repair_request(&self, previous: &str, error: &str) -> ChatRequest {
        let user = format!(
            "The previous response was rejected.\nReason: {}\nPrevious response:\n{}\nReturn ONLY corrected JSON that validates against the schema.",
            truncate(error, 600),
            truncate(previous, 2000)
        );
        let request = ChatRequest::new(prompt::system_policy(self.schema.schema()), user);
        self.apply(request)
    }
}
