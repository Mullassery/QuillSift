//! Implementations of the CLI subcommands.

use std::fs;
use std::path::Path;
use std::time::Duration;

use http::HeaderValue;
use serde_json::Value;

use scrape_core::{CrawlPolicy, Extractor, FetchPolicy, ScrapeError};
use scrape_crawl::Crawler;
use scrape_extract::{ExtractionConfig, LlmExtractor};
use scrape_fetch::Fetcher;
use scrape_html::StructuralExtractor;
use scrape_llm::{build_provider, ProviderConfig, ProviderKind};

use crate::cli::{CrawlArgs, ExtractArgs, FetchArgs, Format, Provider};

/// CLI result type: any error type that is thread-safe and printable.
pub type CliResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;

const MAX_SCHEMA_BYTES: u64 = 256 * 1024;

fn policy(allow_private: bool) -> FetchPolicy {
    FetchPolicy {
        allow_private_networks: allow_private,
        ..FetchPolicy::default()
    }
}

/// Runs `fetch`.
///
/// # Errors
/// Returns fetch, parse or serialization errors.
pub async fn fetch(args: FetchArgs) -> CliResult {
    let document = Fetcher::new(policy(args.allow_private))?
        .fetch(&args.url)
        .await?;
    let extracted = StructuralExtractor::new().extract(&document.html, &document.url)?;
    match args.format {
        Format::Markdown => println!("{}", extracted.markdown.trim_end()),
        Format::Text => println!("{}", extracted.text.trim_end()),
        Format::Json => println!("{}", serde_json::to_string_pretty(&extracted)?),
    }
    Ok(())
}

/// Runs `crawl`.
///
/// # Errors
/// Returns configuration, fetch or serialization errors. Per-page failures are
/// included in the report instead of aborting the job.
pub async fn crawl(args: CrawlArgs) -> CliResult {
    let crawl_policy = CrawlPolicy {
        max_depth: args.max_depth,
        max_pages: args.max_pages,
        same_origin_only: !args.cross_origin,
        respect_robots: !args.ignore_robots,
        crawl_delay: args.crawl_delay_ms.map(Duration::from_millis),
        max_concurrency: args.concurrency,
    };
    let report = Crawler::new(policy(args.allow_private), crawl_policy)?
        .crawl(&[args.url])
        .await?;
    match args.format {
        Format::Json => println!("{}", serde_json::to_string_pretty(&report)?),
        Format::Markdown => {
            let pages: Vec<&str> = report
                .pages
                .iter()
                .filter_map(|page| page.content.as_ref())
                .map(|content| content.markdown.trim_end())
                .collect();
            println!("{}", pages.join("\n\n---\n\n"));
        }
        Format::Text => {
            let pages: Vec<&str> = report
                .pages
                .iter()
                .filter_map(|page| page.content.as_ref())
                .map(|content| content.text.trim_end())
                .collect();
            println!("{}", pages.join("\n\n---\n\n"));
        }
    }
    Ok(())
}

/// Runs `extract`.
///
/// # Errors
/// Returns configuration, fetch, extraction or serialization errors. The API
/// key is read from an environment variable, never a command-line argument.
pub async fn extract(args: ExtractArgs) -> CliResult {
    let schema = read_schema(&args.schema)?;

    let mut llm_policy = policy(args.allow_private);
    if args.provider == Provider::Openai {
        let key = std::env::var(&args.api_key_env).map_err(|_| {
            ScrapeError::Config(format!(
                "environment variable `{}` is not set",
                args.api_key_env
            ))
        })?;
        if key.trim().is_empty() {
            return Err(ScrapeError::Config(format!(
                "environment variable `{}` is empty",
                args.api_key_env
            ))
            .into());
        }
        let header: HeaderValue = format!("Bearer {}", key.trim()).parse()?;
        llm_policy.headers.insert("authorization", header);
    }

    let page_http = Fetcher::new(policy(args.allow_private))?;
    let llm_http = Fetcher::new(llm_policy)?;
    let kind = match args.provider {
        Provider::Openai => ProviderKind::OpenAiCompatible,
        Provider::Ollama => ProviderKind::Ollama,
    };
    let provider_config = ProviderConfig {
        kind,
        base_url: args.base_url,
        model: args.model,
    };
    let model = build_provider(llm_http, &provider_config)?;
    let extraction_config = ExtractionConfig {
        max_attempts: args.max_attempts,
        temperature: args.temperature.unwrap_or(0.0),
        max_input_chars: args.max_input_chars.unwrap_or(100_000),
        max_tokens: None,
    };
    let extractor = LlmExtractor::new(model, schema, extraction_config)?;

    let document = page_http.fetch(&args.url).await?;
    let content = StructuralExtractor::new().extract(&document.html, &document.url)?;
    let value = extractor.extract(&content.text).await?;
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}

fn read_schema(path: &Path) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let metadata = fs::metadata(path)?;
    if metadata.len() > MAX_SCHEMA_BYTES {
        return Err(
            ScrapeError::Config(format!("schema file exceeds {MAX_SCHEMA_BYTES} bytes")).into(),
        );
    }
    let text = fs::read_to_string(path)?;
    Ok(serde_json::from_str(&text)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_policy_is_private_network_free() {
        assert!(!policy(false).allow_private_networks);
        assert!(policy(true).allow_private_networks);
    }

    #[test]
    fn reads_and_parses_a_schema_file() {
        let path = std::env::temp_dir().join(format!(
            "scrapeowl-schema-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::write(&path, r#"{"type":"object"}"#).unwrap();
        let schema = read_schema(&path).unwrap();
        assert_eq!(schema["type"], "object");
        let _ = fs::remove_file(path);
    }

    #[test]
    fn missing_schema_file_is_an_error() {
        assert!(read_schema(Path::new("/definitely/not/here.json")).is_err());
    }
}
