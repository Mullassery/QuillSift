//! JSON parsing and JSON Schema validation.

use serde_json::Value;

use scrape_core::{Result, ScrapeError};

use crate::text::truncate;

/// A compiled JSON Schema plus the original document (needed in prompts).
pub struct SchemaValidator {
    schema: Value,
    validator: jsonschema::Validator,
}

impl SchemaValidator {
    /// Compiles `schema`, rejecting non-object or invalid schemas.
    ///
    /// # Errors
    /// Returns [`ScrapeError::Config`] when the schema is not an object or does
    /// not compile.
    pub fn new(schema: Value) -> Result<Self> {
        if !schema.is_object() {
            return Err(ScrapeError::Config(
                "extraction schema must be a JSON object".into(),
            ));
        }
        let validator = jsonschema::validator_for(&schema)
            .map_err(|err| ScrapeError::Config(format!("invalid JSON Schema: {err}")))?;
        Ok(Self { schema, validator })
    }

    /// Returns the schema document.
    pub fn schema(&self) -> &Value {
        &self.schema
    }

    /// Validates `value`, returning a bounded, human-readable error if invalid.
    ///
    /// # Errors
    /// Returns [`ScrapeError::Extraction`] when validation fails.
    pub fn validate(&self, value: &Value) -> Result<()> {
        if self.validator.validate(value).is_ok() {
            return Ok(());
        }
        let mut messages = Vec::new();
        for error in self.validator.iter_errors(value).take(8) {
            messages.push(truncate(&error.to_string(), 300));
        }
        Err(ScrapeError::Extraction(format!(
            "schema validation failed: {}",
            messages.join("; ")
        )))
    }
}

/// Parses model output that should be JSON, tolerating a markdown code fence.
///
/// # Errors
/// Returns [`ScrapeError::Extraction`] when no JSON value can be recovered.
pub fn parse_candidate(raw: &str) -> Result<Value> {
    let text = raw.trim();
    let candidate = strip_fence(text).unwrap_or(text);
    if let Ok(value) = serde_json::from_str(candidate) {
        return Ok(value);
    }
    if let (Some(start), Some(end)) = (candidate.find('{'), candidate.rfind('}')) {
        if start < end {
            if let Ok(value) = serde_json::from_str(&candidate[start..=end]) {
                return Ok(value);
            }
        }
    }
    Err(ScrapeError::Extraction(format!(
        "model output was not valid JSON: {}",
        truncate(text, 200)
    )))
}

fn strip_fence(text: &str) -> Option<&str> {
    let rest = text.strip_prefix("```")?;
    let rest = rest.strip_prefix("json").unwrap_or(rest);
    let rest = rest.trim_start_matches(['\r', '\n']);
    let inner = rest.strip_suffix("```")?;
    Some(inner.trim())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_plain_and_fenced_json() {
        assert_eq!(parse_candidate(r#"{"a":1}"#).unwrap(), json!({"a": 1}));
        assert_eq!(
            parse_candidate("```json\n{\"a\":1}\n```").unwrap(),
            json!({"a": 1})
        );
        assert_eq!(
            parse_candidate("here you go: {\"a\":1} thanks").unwrap(),
            json!({"a": 1})
        );
    }

    #[test]
    fn rejects_non_json() {
        let err = parse_candidate("not json").unwrap_err();
        assert!(matches!(err, ScrapeError::Extraction(_)));
    }

    #[test]
    fn schema_validation_reports_errors() {
        let validator =
            SchemaValidator::new(json!({"type": "object", "required": ["title"]})).unwrap();
        assert!(validator.validate(&json!({"title": "x"})).is_ok());
        let err = validator.validate(&json!({})).unwrap_err();
        assert!(matches!(err, ScrapeError::Extraction(_)));
        assert!(err.to_string().contains("title"), "{err}");
    }

    #[test]
    fn non_object_and_invalid_schemas_are_rejected() {
        assert!(matches!(
            SchemaValidator::new(json!("nope")),
            Err(ScrapeError::Config(_))
        ));
        assert!(matches!(
            SchemaValidator::new(json!({"type": "not-a-real-type"})),
            Err(ScrapeError::Config(_))
        ));
    }
}
