//! Typed errors for `ScrapeOwl`.
//!
//! Every fallible operation returns [`ScrapeError`]. Errors carry a coarse
//! [`ErrorKind`] so callers (retry policies, the API layer) can make decisions
//! without matching on message strings.

use std::net::IpAddr;

use thiserror::Error;

/// Convenience result alias used throughout the workspace.
pub type Result<T, E = ScrapeError> = std::result::Result<T, E>;

/// Coarse classification used by retry and reporting logic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrorKind {
    /// Caller supplied invalid configuration and nothing was attempted.
    Configuration,
    /// A URL or destination address was rejected by policy.
    Blocked,
    /// A transient transport/network failure; safe to retry.
    Transient,
    /// The operation timed out; retry depends on policy.
    Timeout,
    /// The operation was cancelled or a deadline elapsed.
    Cancelled,
    /// The remote returned a non-success HTTP status.
    HttpStatus,
    /// Content could not be parsed or validated.
    Parse,
    /// Extraction failed.
    Extraction,
    /// An LLM provider failed.
    Llm,
    /// The operation is not supported.
    Unsupported,
    /// A local I/O failure.
    Io,
}

/// The workspace error type.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ScrapeError {
    /// Invalid configuration detected before any work was performed.
    #[error("invalid configuration: {0}")]
    Config(String),

    /// The URL is syntactically valid but rejected by policy.
    #[error("URL not allowed: {0}")]
    UrlNotAllowed(String),

    /// DNS resolution failed.
    #[error("DNS resolution failed for host: {0}")]
    DnsFailure(String),

    /// The resolved or connected address is in a blocked range.
    #[error("blocked destination address: {0}")]
    AddressBlocked(IpAddr),

    /// More redirects were followed than allowed.
    #[error("too many redirects (limit {limit})")]
    TooManyRedirects {
        /// The configured redirect limit.
        limit: u8,
    },

    /// The response body exceeded the configured limit.
    #[error("response exceeded the size limit of {limit} bytes")]
    ResponseTooLarge {
        /// The configured byte limit.
        limit: u64,
    },

    /// A configured timeout elapsed.
    #[error("operation timed out")]
    Timeout,

    /// The overall job deadline elapsed.
    #[error("deadline exceeded")]
    DeadlineExceeded,

    /// The operation was cancelled by the caller.
    #[error("operation cancelled")]
    Cancelled,

    /// The remote returned a non-success HTTP status.
    #[error("HTTP status {status} for {url}")]
    HttpStatus {
        /// The HTTP status code.
        status: u16,
        /// The URL that produced the status.
        url: String,
    },

    /// A transport-level failure.
    #[error("transport error: {0}")]
    Transport(String),

    /// The document could not be parsed.
    #[error("failed to parse document: {0}")]
    Parse(String),

    /// Extraction failed.
    #[error("extraction failed: {0}")]
    Extraction(String),

    /// An LLM provider failed.
    #[error("LLM provider error: {0}")]
    Llm(String),

    /// The requested operation is not supported.
    #[error("unsupported operation: {0}")]
    Unsupported(String),

    /// A local I/O error.
    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// A URL parsing error.
    #[error(transparent)]
    Url(#[from] url::ParseError),
}

impl ScrapeError {
    /// Returns the coarse category for this error.
    pub fn kind(&self) -> ErrorKind {
        match self {
            Self::Config(_) => ErrorKind::Configuration,
            Self::UrlNotAllowed(_)
            | Self::DnsFailure(_)
            | Self::AddressBlocked(_)
            | Self::TooManyRedirects { .. }
            | Self::ResponseTooLarge { .. } => ErrorKind::Blocked,
            Self::Transport(_) => ErrorKind::Transient,
            Self::Timeout => ErrorKind::Timeout,
            Self::DeadlineExceeded | Self::Cancelled => ErrorKind::Cancelled,
            Self::HttpStatus { .. } => ErrorKind::HttpStatus,
            Self::Parse(_) => ErrorKind::Parse,
            Self::Extraction(_) => ErrorKind::Extraction,
            Self::Llm(_) => ErrorKind::Llm,
            Self::Unsupported(_) => ErrorKind::Unsupported,
            Self::Io(_) | Self::Url(_) => ErrorKind::Io,
        }
    }

    /// Returns `true` when retrying the same operation may succeed.
    ///
    /// Only transport failures and timeouts are considered retryable; policy,
    /// parsing and configuration errors are not.
    pub fn is_retryable(&self) -> bool {
        matches!(self.kind(), ErrorKind::Transient | ErrorKind::Timeout)
    }
}

#[cfg(test)]
mod tests {
    use std::net::Ipv4Addr;

    use super::*;

    #[test]
    fn transport_errors_are_retryable() {
        assert!(ScrapeError::Transport("connection reset".into()).is_retryable());
        assert!(ScrapeError::Timeout.is_retryable());
    }

    #[test]
    fn policy_and_parse_errors_are_not_retryable() {
        assert!(!ScrapeError::UrlNotAllowed("scheme".into()).is_retryable());
        assert!(!ScrapeError::Parse("bad html".into()).is_retryable());
        assert!(!ScrapeError::Config("bad".into()).is_retryable());
        assert!(!ScrapeError::Cancelled.is_retryable());
    }

    #[test]
    fn kinds_are_classified() {
        assert_eq!(
            ScrapeError::Config("x".into()).kind(),
            ErrorKind::Configuration
        );
        let blocked = ScrapeError::AddressBlocked(IpAddr::V4(Ipv4Addr::LOCALHOST));
        assert_eq!(blocked.kind(), ErrorKind::Blocked);
    }

    #[test]
    fn url_errors_convert() {
        let err: ScrapeError = "not a url".parse::<url::Url>().unwrap_err().into();
        assert_eq!(err.kind(), ErrorKind::Io);
    }
}
