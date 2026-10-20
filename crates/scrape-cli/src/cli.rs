//! Command-line surface.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};
use url::Url;

/// `ScrapeOwl` command-line interface.
#[derive(Debug, Parser)]
#[command(
    name = "scrapeowl",
    version,
    about = "Self-hosted deterministic scraping and schema-validated LLM extraction"
)]
pub struct Cli {
    /// Subcommand to run.
    #[command(subcommand)]
    pub command: Command,
}

/// Available subcommands.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Fetch a URL and print deterministic content.
    Fetch(FetchArgs),
    /// Fetch a URL and extract schema-validated JSON with an LLM.
    Extract(ExtractArgs),
}

/// Output formats for `fetch`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Format {
    /// Markdown rendering of the main content.
    Markdown,
    /// Normalized visible text.
    Text,
    /// Full structured result as JSON.
    Json,
}

/// Arguments for `fetch`.
#[derive(Debug, Args)]
pub struct FetchArgs {
    /// URL to fetch.
    pub url: Url,
    /// Allow loopback/private destinations (trusted deployments only).
    #[arg(long)]
    pub allow_private: bool,
    /// Output format.
    #[arg(long, value_enum, default_value_t = Format::Markdown)]
    pub format: Format,
}

/// Supported LLM providers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Provider {
    /// `OpenAI` `/chat/completions` protocol.
    Openai,
    /// Local Ollama `/api/chat` protocol.
    Ollama,
}

/// Arguments for `extract`.
#[derive(Debug, Args)]
pub struct ExtractArgs {
    /// URL to fetch.
    pub url: Url,
    /// Path to a JSON Schema file (at most 256 KiB).
    #[arg(long)]
    pub schema: PathBuf,
    /// LLM provider protocol.
    #[arg(long, value_enum, default_value_t = Provider::Openai)]
    pub provider: Provider,
    /// Provider base URL (no credentials).
    #[arg(long, default_value = "https://api.openai.com/v1")]
    pub base_url: String,
    /// Model identifier.
    #[arg(long)]
    pub model: String,
    /// Environment variable holding the bearer token (`OpenAI` provider only).
    #[arg(long, default_value = "OPENAI_API_KEY")]
    pub api_key_env: String,
    /// Allow loopback/private destinations (trusted deployments only).
    #[arg(long)]
    pub allow_private: bool,
    /// Total model calls, including the first attempt.
    #[arg(long, default_value_t = 3)]
    pub max_attempts: u32,
    /// Sampling temperature (defaults to 0.0).
    #[arg(long)]
    pub temperature: Option<f32>,
    /// Maximum document characters sent to the model.
    #[arg(long)]
    pub max_input_chars: Option<usize>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn parses_fetch_with_defaults() {
        let cli = Cli::try_parse_from(["scrapeowl", "fetch", "https://example.com"]).unwrap();
        match cli.command {
            Command::Fetch(args) => {
                assert_eq!(args.format, Format::Markdown);
                assert!(!args.allow_private);
                assert_eq!(args.url.as_str(), "https://example.com/");
            }
            Command::Extract(_) => panic!("expected fetch"),
        }
    }

    #[test]
    fn parses_extract_options() {
        let cli = Cli::try_parse_from([
            "scrapeowl",
            "extract",
            "https://example.com",
            "--schema",
            "schema.json",
            "--provider",
            "ollama",
            "--base-url",
            "http://localhost:11434",
            "--model",
            "llama3",
            "--allow-private",
            "--max-attempts",
            "5",
            "--temperature",
            "0.2",
        ])
        .unwrap();
        match cli.command {
            Command::Extract(args) => {
                assert_eq!(args.provider, Provider::Ollama);
                assert_eq!(args.model, "llama3");
                assert_eq!(args.max_attempts, 5);
                assert!(args.allow_private);
                assert_eq!(args.temperature, Some(0.2));
            }
            Command::Fetch(_) => panic!("expected extract"),
        }
    }

    #[test]
    fn extract_requires_schema_and_model() {
        assert!(Cli::try_parse_from(["scrapeowl", "extract", "https://example.com"]).is_err());
        assert!(Cli::try_parse_from([
            "scrapeowl",
            "extract",
            "https://example.com",
            "--schema",
            "schema.json"
        ])
        .is_err());
    }

    #[test]
    fn rejects_unknown_provider_and_format() {
        assert!(Cli::try_parse_from([
            "scrapeowl",
            "extract",
            "https://example.com",
            "--schema",
            "schema.json",
            "--model",
            "m",
            "--provider",
            "unknown"
        ])
        .is_err());
        assert!(Cli::try_parse_from([
            "scrapeowl",
            "fetch",
            "https://example.com",
            "--format",
            "xml"
        ])
        .is_err());
    }
}
