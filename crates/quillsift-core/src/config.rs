//! Typed configuration policies.
//!
//! These are the single source of truth for limits that other crates enforce.
//! Constructors provide conservative, secure defaults; [`FetchPolicy::validate`]
//! and [`CrawlPolicy::validate`] reject nonsensical or unsafe combinations
//! before any network access happens.

use std::time::Duration;

use crate::error::{Result, ScrapeError};

/// Default `User-Agent` sent when the caller does not override it.
pub fn default_user_agent() -> String {
    format!("QuillSift/{}", env!("CARGO_PKG_VERSION"))
}

/// Policy controlling a single HTTP fetch.
///
/// The defaults are conservative: private network access is disabled, redirects
/// and response sizes are bounded, and both connect and total timeouts apply.
#[derive(Clone)]
pub struct FetchPolicy {
    /// `User-Agent` header value.
    pub user_agent: String,
    /// Additional request headers.
    pub headers: http::HeaderMap,
    /// Maximum accepted response body size, in bytes.
    pub max_response_bytes: u64,
    /// Maximum accepted decompressed body size, in bytes (guards gzip bombs).
    pub max_decompressed_bytes: u64,
    /// Maximum number of redirects to follow.
    pub max_redirects: u8,
    /// TCP/TLS connect timeout.
    pub connect_timeout: Duration,
    /// Total time budget for the request, including body download.
    pub total_timeout: Duration,
    /// Whether private/loopback/link-local destinations are permitted.
    ///
    /// This must only be enabled in explicitly trusted deployments; it is
    /// `false` by default and is the primary SSRF guard.
    pub allow_private_networks: bool,
}

struct RedactedHeaders<'a>(&'a http::HeaderMap);

impl std::fmt::Debug for RedactedHeaders<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "<{} header(s) redacted>", self.0.len())
    }
}

impl std::fmt::Debug for FetchPolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FetchPolicy")
            .field("user_agent", &self.user_agent)
            .field("headers", &RedactedHeaders(&self.headers))
            .field("max_response_bytes", &self.max_response_bytes)
            .field("max_decompressed_bytes", &self.max_decompressed_bytes)
            .field("max_redirects", &self.max_redirects)
            .field("connect_timeout", &self.connect_timeout)
            .field("total_timeout", &self.total_timeout)
            .field("allow_private_networks", &self.allow_private_networks)
            .finish()
    }
}

impl Default for FetchPolicy {
    fn default() -> Self {
        Self {
            user_agent: default_user_agent(),
            headers: http::HeaderMap::new(),
            max_response_bytes: 10 * 1024 * 1024,
            max_decompressed_bytes: 50 * 1024 * 1024,
            max_redirects: 5,
            connect_timeout: Duration::from_secs(10),
            total_timeout: Duration::from_secs(30),
            allow_private_networks: false,
        }
    }
}

impl FetchPolicy {
    /// Validates the policy, returning a [`ScrapeError::Config`] on problems.
    ///
    /// # Errors
    /// Returns an error when limits are zero or when a timeout is zero.
    pub fn validate(&self) -> Result<()> {
        if self.max_response_bytes == 0 {
            return Err(ScrapeError::Config(
                "max_response_bytes must be greater than zero".into(),
            ));
        }
        if self.max_decompressed_bytes < self.max_response_bytes {
            return Err(ScrapeError::Config(
                "max_decompressed_bytes must be >= max_response_bytes".into(),
            ));
        }
        if self.connect_timeout.is_zero() {
            return Err(ScrapeError::Config(
                "connect_timeout must be non-zero".into(),
            ));
        }
        if self.total_timeout.is_zero() {
            return Err(ScrapeError::Config("total_timeout must be non-zero".into()));
        }
        if self.total_timeout < self.connect_timeout {
            return Err(ScrapeError::Config(
                "total_timeout must be >= connect_timeout".into(),
            ));
        }
        Ok(())
    }
}

/// Bounded retry policy with exponential backoff.
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    /// Maximum number of attempts, including the first.
    pub max_attempts: u32,
    /// Initial backoff applied before the first retry.
    pub initial_backoff: Duration,
    /// Upper bound for any single backoff.
    pub max_backoff: Duration,
    /// Whether to apply random jitter to each backoff.
    pub jitter: bool,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_backoff: Duration::from_millis(200),
            max_backoff: Duration::from_secs(5),
            jitter: true,
        }
    }
}

impl RetryPolicy {
    /// Validates the policy.
    ///
    /// # Errors
    /// Returns an error for zero attempts, zero initial backoff, or a maximum
    /// backoff smaller than the initial backoff.
    pub fn validate(&self) -> Result<()> {
        if self.max_attempts == 0 {
            return Err(ScrapeError::Config(
                "retry max_attempts must be at least 1".into(),
            ));
        }
        if self.initial_backoff.is_zero() {
            return Err(ScrapeError::Config(
                "retry initial_backoff must be non-zero".into(),
            ));
        }
        if self.max_backoff < self.initial_backoff {
            return Err(ScrapeError::Config(
                "retry max_backoff must be >= initial_backoff".into(),
            ));
        }
        Ok(())
    }
}

/// Bounds and compliance settings for a crawl job.
#[derive(Debug, Clone)]
pub struct CrawlPolicy {
    /// Maximum link-following depth from the seed URL(s).
    pub max_depth: u32,
    /// Maximum number of pages fetched in a single job.
    pub max_pages: usize,
    /// Whether to restrict crawling to the seed origin.
    pub same_origin_only: bool,
    /// Whether to honor `robots.txt` before fetching a URL.
    pub respect_robots: bool,
    /// Optional delay applied between requests to the same host.
    pub crawl_delay: Option<Duration>,
    /// Maximum number of in-flight requests.
    pub max_concurrency: usize,
}

impl Default for CrawlPolicy {
    fn default() -> Self {
        Self {
            max_depth: 1,
            max_pages: 50,
            same_origin_only: true,
            respect_robots: true,
            crawl_delay: None,
            max_concurrency: 4,
        }
    }
}

impl CrawlPolicy {
    /// Validates the crawl policy.
    ///
    /// # Errors
    /// Returns an error when page limits or concurrency are zero.
    pub fn validate(&self) -> Result<()> {
        if self.max_pages == 0 {
            return Err(ScrapeError::Config(
                "crawl max_pages must be greater than zero".into(),
            ));
        }
        if self.max_concurrency == 0 {
            return Err(ScrapeError::Config(
                "crawl max_concurrency must be greater than zero".into(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fetch_defaults_are_secure_and_valid() {
        let policy = FetchPolicy::default();
        assert!(!policy.allow_private_networks);
        assert!(policy.max_redirects > 0);
        policy.validate().expect("defaults must validate");
    }

    #[test]
    fn zero_response_limit_is_rejected() {
        let policy = FetchPolicy {
            max_response_bytes: 0,
            ..FetchPolicy::default()
        };
        assert!(matches!(policy.validate(), Err(ScrapeError::Config(_))));
    }

    #[test]
    fn decompressed_bound_must_cover_response_bound() {
        let policy = FetchPolicy {
            max_response_bytes: 100,
            max_decompressed_bytes: 50,
            ..FetchPolicy::default()
        };
        assert!(policy.validate().is_err());
    }

    #[test]
    fn total_timeout_must_cover_connect_timeout() {
        let policy = FetchPolicy {
            connect_timeout: Duration::from_secs(30),
            total_timeout: Duration::from_secs(5),
            ..FetchPolicy::default()
        };
        assert!(policy.validate().is_err());
    }

    #[test]
    fn retry_defaults_validate_and_zero_attempts_fail() {
        RetryPolicy::default().validate().unwrap();
        let bad = RetryPolicy {
            max_attempts: 0,
            ..RetryPolicy::default()
        };
        assert!(bad.validate().is_err());
    }

    #[test]
    fn crawl_defaults_validate_and_zero_pages_fail() {
        CrawlPolicy::default().validate().unwrap();
        let bad = CrawlPolicy {
            max_pages: 0,
            ..CrawlPolicy::default()
        };
        assert!(bad.validate().is_err());
    }

    #[test]
    fn user_agent_is_versioned() {
        assert!(default_user_agent().starts_with("QuillSift/"));
    }

    #[test]
    fn debug_redacts_header_values() {
        let mut policy = FetchPolicy::default();
        policy.headers.insert(
            http::header::AUTHORIZATION,
            "Bearer super-secret-token".parse().unwrap(),
        );
        let rendered = format!("{policy:?}");
        assert!(!rendered.contains("super-secret-token"), "{rendered}");
        assert!(rendered.contains("redacted"), "{rendered}");
    }
}
