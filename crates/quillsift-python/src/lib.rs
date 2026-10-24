//! Python bindings for `QuillSift`.
//!
//! The module exposes synchronous, blocking functions that release the GIL
//! while doing network I/O. All fetching goes through the same SSRF-guarded
//! client and schema-validated extractor as the CLI.

use std::sync::OnceLock;
use std::time::Duration;

use http::HeaderValue;
use pyo3::exceptions::{PyPermissionError, PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use serde_json::Value;
use url::Url;

use quillsift_core::{CrawlPolicy, ErrorKind, Extracted, Extractor, FetchPolicy, ScrapeError};
use quillsift_crawl::Crawler;
use quillsift_extract::{ExtractionConfig, LlmExtractor};
use quillsift_fetch::Fetcher;
use quillsift_html::StructuralExtractor;
use quillsift_llm::{build_provider, ProviderConfig, ProviderKind};

const MAX_SCHEMA_BYTES: usize = 256 * 1024;

fn runtime() -> &'static tokio::runtime::Runtime {
    static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("failed to create Tokio runtime")
    })
}

fn policy(allow_private: bool) -> FetchPolicy {
    FetchPolicy {
        allow_private_networks: allow_private,
        ..FetchPolicy::default()
    }
}

fn to_pyerr(err: ScrapeError) -> PyErr {
    let message = err.to_string();
    match err.kind() {
        ErrorKind::Configuration | ErrorKind::Parse => PyValueError::new_err(message),
        ErrorKind::Blocked => PyPermissionError::new_err(message),
        _ => PyRuntimeError::new_err(message),
    }
}

fn json_to_py(py: Python<'_>, value: &Value) -> PyResult<Py<PyAny>> {
    let text = serde_json::to_string(value)
        .map_err(|err| PyRuntimeError::new_err(format!("failed to encode JSON: {err}")))?;
    Ok(py.import("json")?.call_method1("loads", (text,))?.unbind())
}

fn parse_schema(py: Python<'_>, schema: &Bound<'_, PyAny>) -> PyResult<Value> {
    let text = if let Ok(text) = schema.extract::<String>() {
        text
    } else {
        py.import("json")?
            .call_method1("dumps", (schema,))?
            .extract::<String>()?
    };
    if text.len() > MAX_SCHEMA_BYTES {
        return Err(PyValueError::new_err(format!(
            "schema exceeds {MAX_SCHEMA_BYTES} bytes"
        )));
    }
    serde_json::from_str(&text)
        .map_err(|err| PyValueError::new_err(format!("schema is not valid JSON: {err}")))
}

async fn run_fetch(url: Url, allow_private: bool) -> Result<Extracted, ScrapeError> {
    let document = Fetcher::new(policy(allow_private))?.fetch(&url).await?;
    StructuralExtractor::new().extract(&document.html, &document.url)
}

/// Fetches a URL and returns deterministic content.
///
/// `format` is `"markdown"`, `"text"` or `"json"`. The JSON form is a dict with
/// `url`, `text`, `markdown`, `links`, `images` and `metadata` keys. Private
/// destinations are refused unless `allow_private=True`.
///
/// # Errors
/// Raises `ValueError` for invalid URLs/options, `PermissionError` for blocked
/// destinations and `RuntimeError` for transport, status or parse failures.
/// Parses caller-supplied HTML with no network access.
///
/// Useful for local files, tests, and content already fetched elsewhere.
/// `format` is `"markdown"`, `"text"` or `"json"`; `base_url` is used only to
/// resolve relative links and fill the `url` field.
#[pyfunction]
#[pyo3(signature = (html, *, base_url = "https://quillsift.local/", format = "markdown"))]
fn parse_html(py: Python<'_>, html: &str, base_url: &str, format: &str) -> PyResult<Py<PyAny>> {
    let base = Url::parse(base_url).map_err(|err| to_pyerr(err.into()))?;
    let extracted = StructuralExtractor::new()
        .extract(html, &base)
        .map_err(to_pyerr)?;
    match format {
        "markdown" => Ok(extracted
            .markdown
            .trim_end()
            .to_owned()
            .into_pyobject(py)?
            .into_any()
            .unbind()),
        "text" => Ok(extracted
            .text
            .trim_end()
            .to_owned()
            .into_pyobject(py)?
            .into_any()
            .unbind()),
        "json" => {
            let value = serde_json::to_value(&extracted).map_err(|err| {
                PyRuntimeError::new_err(format!("failed to encode result: {err}"))
            })?;
            json_to_py(py, &value)
        }
        other => Err(PyValueError::new_err(format!(
            "unknown format `{other}`; expected `markdown`, `text` or `json`"
        ))),
    }
}

#[pyfunction]
#[pyo3(signature = (url, *, format = "markdown", allow_private = false))]
fn fetch(py: Python<'_>, url: &str, format: &str, allow_private: bool) -> PyResult<Py<PyAny>> {
    let parsed = Url::parse(url).map_err(|err| to_pyerr(err.into()))?;
    let extracted = py
        .detach(|| runtime().block_on(run_fetch(parsed, allow_private)))
        .map_err(to_pyerr)?;
    match format {
        "markdown" => Ok(extracted
            .markdown
            .trim_end()
            .to_owned()
            .into_pyobject(py)?
            .into_any()
            .unbind()),
        "text" => Ok(extracted
            .text
            .trim_end()
            .to_owned()
            .into_pyobject(py)?
            .into_any()
            .unbind()),
        "json" => {
            let value = serde_json::to_value(&extracted).map_err(|err| {
                PyRuntimeError::new_err(format!("failed to encode result: {err}"))
            })?;
            json_to_py(py, &value)
        }
        other => Err(PyValueError::new_err(format!(
            "unknown format `{other}`; expected `markdown`, `text` or `json`"
        ))),
    }
}

struct ExtractSpec {
    url: Url,
    schema: Value,
    provider: String,
    base_url: Option<String>,
    model: String,
    api_key_env: String,
    allow_private: bool,
    max_attempts: u32,
    temperature: f32,
    max_input_chars: usize,
}

async fn run_extract(spec: ExtractSpec) -> Result<Value, ScrapeError> {
    let kind = match spec.provider.as_str() {
        "openai" => ProviderKind::OpenAiCompatible,
        "ollama" => ProviderKind::Ollama,
        other => {
            return Err(ScrapeError::Config(format!(
                "unknown provider `{other}`; expected `openai` or `ollama`"
            )))
        }
    };

    let mut llm_policy = policy(spec.allow_private);
    if kind == ProviderKind::OpenAiCompatible {
        let key = std::env::var(&spec.api_key_env).map_err(|_| {
            ScrapeError::Config(format!(
                "environment variable `{}` is not set",
                spec.api_key_env
            ))
        })?;
        if key.trim().is_empty() {
            return Err(ScrapeError::Config(format!(
                "environment variable `{}` is empty",
                spec.api_key_env
            )));
        }
        let header: HeaderValue = format!("Bearer {}", key.trim())
            .parse()
            .map_err(|err| ScrapeError::Config(format!("invalid API key header: {err}")))?;
        llm_policy.headers.insert("authorization", header);
    }

    let base_url = spec.base_url.unwrap_or_else(|| match kind {
        ProviderKind::OpenAiCompatible => "https://api.openai.com/v1".to_owned(),
        ProviderKind::Ollama => "http://localhost:11434".to_owned(),
    });

    let page_http = Fetcher::new(policy(spec.allow_private))?;
    let llm_http = Fetcher::new(llm_policy)?;
    let model = build_provider(
        llm_http,
        &ProviderConfig {
            kind,
            base_url,
            model: spec.model,
        },
    )?;
    let extractor = LlmExtractor::new(
        model,
        spec.schema,
        ExtractionConfig {
            max_attempts: spec.max_attempts,
            temperature: spec.temperature,
            max_input_chars: spec.max_input_chars,
            max_tokens: None,
        },
    )?;

    let document = page_http.fetch(&spec.url).await?;
    let content = StructuralExtractor::new().extract(&document.html, &document.url)?;
    extractor.extract(&content.text).await
}

/// Fetches a URL and extracts schema-validated JSON with an LLM.
///
/// `schema` is a JSON Schema as a dict or a JSON string. `provider` is
/// `"openai"` (API key from `api_key_env`) or `"ollama"`. Output is repaired
/// for at most `max_attempts` total model calls. Private destinations,
/// including a local Ollama endpoint, require `allow_private=True`.
///
/// # Errors
/// Raises `ValueError` for invalid schemas/configuration, `PermissionError`
/// for blocked destinations and `RuntimeError` for fetch, provider or
/// validation failures.
#[pyfunction]
#[pyo3(signature = (
    url,
    schema,
    *,
    provider = "openai",
    base_url = None,
    model,
    api_key_env = "OPENAI_API_KEY",
    allow_private = false,
    max_attempts = 3,
    temperature = 0.0,
    max_input_chars = 100_000
))]
#[allow(clippy::too_many_arguments)]
fn extract(
    py: Python<'_>,
    url: &str,
    schema: &Bound<'_, PyAny>,
    provider: &str,
    base_url: Option<&str>,
    model: &str,
    api_key_env: &str,
    allow_private: bool,
    max_attempts: u32,
    temperature: f32,
    max_input_chars: usize,
) -> PyResult<Py<PyAny>> {
    let spec = ExtractSpec {
        url: Url::parse(url).map_err(|err| to_pyerr(err.into()))?,
        schema: parse_schema(py, schema)?,
        provider: provider.to_owned(),
        base_url: base_url.map(str::to_owned),
        model: model.to_owned(),
        api_key_env: api_key_env.to_owned(),
        allow_private,
        max_attempts,
        temperature,
        max_input_chars,
    };
    let value = py
        .detach(|| runtime().block_on(run_extract(spec)))
        .map_err(to_pyerr)?;
    json_to_py(py, &value)
}

/// Legacy bounded crawl; prefer `MudFish` for new crawling and JS rendering.
///
/// Returns a dict with `seeds`, `pages`, `failures` and `truncated` keys. Pages
/// include their final URL, depth, status, content type and deterministic
/// content for HTML. Per-page failures are returned in the report instead of
/// raising. Private destinations are refused unless `allow_private=True`.
///
/// # Errors
/// Raises `ValueError` for invalid URLs or policies, `PermissionError` for
/// blocked destinations and `RuntimeError` for transport failures.
#[pyfunction]
#[pyo3(signature = (
    url,
    *,
    max_depth = 1,
    max_pages = 50,
    same_origin = true,
    respect_robots = true,
    crawl_delay_ms = None,
    concurrency = 4,
    allow_private = false
))]
#[allow(clippy::too_many_arguments)]
fn crawl(
    py: Python<'_>,
    url: &str,
    max_depth: u32,
    max_pages: usize,
    same_origin: bool,
    respect_robots: bool,
    crawl_delay_ms: Option<u64>,
    concurrency: usize,
    allow_private: bool,
) -> PyResult<Py<PyAny>> {
    let seed = Url::parse(url).map_err(|err| to_pyerr(err.into()))?;
    let crawl_policy = CrawlPolicy {
        max_depth,
        max_pages,
        same_origin_only: same_origin,
        respect_robots,
        crawl_delay: crawl_delay_ms.map(Duration::from_millis),
        max_concurrency: concurrency,
    };
    let crawler = Crawler::new(policy(allow_private), crawl_policy).map_err(to_pyerr)?;
    let report = py
        .detach(|| runtime().block_on(crawler.crawl(&[seed])))
        .map_err(to_pyerr)?;
    let value = serde_json::to_value(&report)
        .map_err(|err| PyRuntimeError::new_err(format!("failed to encode result: {err}")))?;
    json_to_py(py, &value)
}

#[pymodule]
fn _quillsift(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    m.add_function(wrap_pyfunction!(fetch, m)?)?;
    m.add_function(wrap_pyfunction!(parse_html, m)?)?;
    m.add_function(wrap_pyfunction!(extract, m)?)?;
    m.add_function(wrap_pyfunction!(crawl, m)?)?;
    Ok(())
}
