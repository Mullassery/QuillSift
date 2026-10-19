//! The HTTP fetcher.

use futures_util::StreamExt;
use reqwest::header::{CONTENT_TYPE, LOCATION};
use reqwest::redirect::Policy;
use reqwest::{Client, Response};
use tracing::{debug, instrument};
use url::Url;

use scrape_core::{block_reason, Document, FetchPolicy, Result, ScrapeError};

use crate::guard::ensure_allowed;

/// An SSRF-guarded HTTP client that enforces timeouts and response limits.
#[derive(Debug, Clone)]
pub struct Fetcher {
    client: Client,
    policy: FetchPolicy,
}

impl Fetcher {
    /// Builds a fetcher from a validated [`FetchPolicy`].
    ///
    /// # Errors
    /// Returns [`ScrapeError::Config`] if the policy is invalid and
    /// [`ScrapeError::Transport`] if the HTTP client cannot be built.
    pub fn new(policy: FetchPolicy) -> Result<Self> {
        policy.validate()?;
        let allow_private = policy.allow_private_networks;
        let client = Client::builder()
            .user_agent(policy.user_agent.clone())
            .default_headers(policy.headers.clone())
            .connect_timeout(policy.connect_timeout)
            .timeout(policy.total_timeout)
            // Redirects are followed manually so each hop can be validated.
            .redirect(Policy::none())
            .dns_resolver(crate::dns::resolver(allow_private))
            .build()
            .map_err(|err| ScrapeError::Transport(err.to_string()))?;
        Ok(Self { client, policy })
    }

    /// Returns the policy this fetcher was built with.
    pub fn policy(&self) -> &FetchPolicy {
        &self.policy
    }

    /// Fetches a URL, following redirects up to the configured limit.
    ///
    /// The body is read with a hard cap on decompressed size, so a gzip bomb
    /// cannot exhaust memory.
    ///
    /// # Errors
    /// Returns a typed [`ScrapeError`] for blocked URLs, timeouts, oversized
    /// responses, too many redirects and non-success HTTP statuses.
    #[instrument(skip(self), fields(url = %seed))]
    pub async fn fetch(&self, seed: &Url) -> Result<Document> {
        let allow_private = self.policy.allow_private_networks;
        let mut url = seed.clone();
        let mut redirects = 0u8;

        loop {
            ensure_allowed(&url, allow_private)?;
            validate_resolution(&url, allow_private).await?;
            debug!(%url, "fetching");

            let response = self
                .client
                .get(url.clone())
                .send()
                .await
                .map_err(|err| map_error(&err))?;
            let status = response.status();

            if status.is_redirection() {
                if redirects >= self.policy.max_redirects {
                    return Err(ScrapeError::TooManyRedirects {
                        limit: self.policy.max_redirects,
                    });
                }
                let location = response
                    .headers()
                    .get(LOCATION)
                    .and_then(|value| value.to_str().ok())
                    .ok_or_else(|| ScrapeError::HttpStatus {
                        status: status.as_u16(),
                        url: url.to_string(),
                    })?;
                let next = url
                    .join(location)
                    .map_err(|err| ScrapeError::UrlNotAllowed(err.to_string()))?;
                redirects += 1;
                url = next;
                continue;
            }

            let final_url = response.url().clone();
            if !status.is_success() {
                return Err(ScrapeError::HttpStatus {
                    status: status.as_u16(),
                    url: final_url.to_string(),
                });
            }
            if let Some(length) = response.content_length() {
                if length > self.policy.max_response_bytes {
                    return Err(ScrapeError::ResponseTooLarge {
                        limit: self.policy.max_response_bytes,
                    });
                }
            }

            let content_type = response
                .headers()
                .get(CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned);
            let body = read_bounded(response, self.policy.max_decompressed_bytes).await?;
            let html = String::from_utf8_lossy(&body).into_owned();
            debug!(%final_url, bytes = body.len(), "fetched");

            return Ok(Document::new(
                final_url,
                status.as_u16(),
                content_type,
                html,
            ));
        }
    }
}

/// Resolves hostnames up front so blocked destinations produce a precise error.
///
/// This is defence in depth, not the only guard: [`crate::dns::GuardedResolver`]
/// re-checks every address at connect time, so a hostname that rebinds between
/// this check and the connection is still rejected.
async fn validate_resolution(url: &Url, allow_private: bool) -> Result<()> {
    let Some(url::Host::Domain(host)) = url.host() else {
        return Ok(());
    };
    let port = url.port_or_known_default().unwrap_or(80);
    let addrs = tokio::net::lookup_host((host, port))
        .await
        .map_err(|err| ScrapeError::DnsFailure(format!("{host}: {err}")))?;

    let mut first_blocked: Option<std::net::IpAddr> = None;
    let mut any_allowed = false;
    for addr in addrs {
        if block_reason(addr.ip(), allow_private).is_some() {
            first_blocked.get_or_insert(addr.ip());
        } else {
            any_allowed = true;
        }
    }
    if any_allowed {
        return Ok(());
    }
    match first_blocked {
        Some(ip) => Err(ScrapeError::AddressBlocked(ip)),
        None => Err(ScrapeError::DnsFailure(format!(
            "{host}: no addresses returned"
        ))),
    }
}

async fn read_bounded(response: Response, limit: u64) -> Result<Vec<u8>> {
    let mut stream = response.bytes_stream();
    let mut buffer: Vec<u8> = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|err| map_error(&err))?;
        if buffer.len() as u64 + chunk.len() as u64 > limit {
            return Err(ScrapeError::ResponseTooLarge { limit });
        }
        buffer.extend_from_slice(&chunk);
    }
    Ok(buffer)
}

fn map_error(err: &reqwest::Error) -> ScrapeError {
    if err.is_timeout() {
        ScrapeError::Timeout
    } else {
        ScrapeError::Transport(err.to_string())
    }
}
