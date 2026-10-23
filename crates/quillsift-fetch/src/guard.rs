//! URL-level SSRF guards.

use std::net::IpAddr;

use url::{Host, Url};

use quillsift_core::{block_reason, Result, ScrapeError};

fn check_ip(ip: IpAddr, allow_private: bool) -> Result<()> {
    if block_reason(ip, allow_private).is_some() {
        Err(ScrapeError::AddressBlocked(ip))
    } else {
        Ok(())
    }
}

/// Validates a URL before fetching: only `http`/`https` are allowed, and
/// literal host addresses are checked against the SSRF policy.
///
/// Hostnames are not resolved here; the connection-time DNS guard performs that
/// check so it cannot be bypassed by rebinding.
///
/// # Errors
/// Returns [`ScrapeError::UrlNotAllowed`] for unsupported schemes or hosts, and
/// [`ScrapeError::AddressBlocked`] for blocked literal addresses.
pub fn ensure_allowed(url: &Url, allow_private: bool) -> Result<()> {
    match url.scheme() {
        "http" | "https" => {}
        other => {
            return Err(ScrapeError::UrlNotAllowed(format!(
                "unsupported scheme: {other}"
            )));
        }
    }
    match url.host() {
        Some(Host::Ipv4(v4)) => check_ip(IpAddr::V4(v4), allow_private),
        Some(Host::Ipv6(v6)) => check_ip(IpAddr::V6(v6), allow_private),
        Some(Host::Domain(_)) => Ok(()),
        None => Err(ScrapeError::UrlNotAllowed("URL has no host".into())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u(s: &str) -> Url {
        Url::parse(s).unwrap()
    }

    #[test]
    fn rejects_non_http_schemes() {
        assert!(matches!(
            ensure_allowed(&u("file:///etc/passwd"), false),
            Err(ScrapeError::UrlNotAllowed(_))
        ));
        assert!(matches!(
            ensure_allowed(&u("ftp://example.com/x"), false),
            Err(ScrapeError::UrlNotAllowed(_))
        ));
    }

    #[test]
    fn blocks_loopback_and_private_literals_by_default() {
        assert!(matches!(
            ensure_allowed(&u("http://127.0.0.1/"), false),
            Err(ScrapeError::AddressBlocked(_))
        ));
        assert!(matches!(
            ensure_allowed(&u("http://[::1]/"), false),
            Err(ScrapeError::AddressBlocked(_))
        ));
        assert!(matches!(
            ensure_allowed(&u("http://169.254.169.254/latest/meta-data"), false),
            Err(ScrapeError::AddressBlocked(_))
        ));
    }

    #[test]
    fn allows_private_literals_when_explicitly_enabled() {
        assert!(ensure_allowed(&u("http://127.0.0.1:8080/"), true).is_ok());
    }

    #[test]
    fn allows_public_hosts() {
        assert!(ensure_allowed(&u("https://example.com/"), false).is_ok());
    }

    #[test]
    fn metadata_blocked_even_with_private_enabled() {
        assert!(matches!(
            ensure_allowed(&u("http://169.254.169.254/"), true),
            Err(ScrapeError::AddressBlocked(_))
        ));
    }

    #[test]
    fn rejects_data_scheme() {
        assert!(matches!(
            ensure_allowed(&u("data:text/plain,hello"), false),
            Err(ScrapeError::UrlNotAllowed(_))
        ));
    }
}
