//! A DNS resolver that enforces the SSRF address policy at connect time.

use std::net::SocketAddr;
use std::sync::Arc;

use reqwest::dns::{Addrs, Name, Resolve, Resolving};
use scrape_core::block_reason;

/// Resolves hostnames and drops any address blocked by the SSRF policy.
///
/// This runs immediately before the TCP connection is established, so it also
/// defends against DNS rebinding: even if a hostname previously resolved to a
/// public address, a later private answer is rejected here.
#[derive(Debug, Clone, Copy)]
pub struct GuardedResolver {
    /// Whether loopback/private/link-local destinations are permitted.
    pub allow_private: bool,
}

/// Builds a shared resolver for `reqwest`.
pub fn resolver(allow_private: bool) -> Arc<GuardedResolver> {
    Arc::new(GuardedResolver { allow_private })
}

impl Resolve for GuardedResolver {
    fn resolve(&self, name: Name) -> Resolving {
        let host = name.as_str().to_owned();
        let allow_private = self.allow_private;
        Box::pin(async move {
            let addrs = tokio::net::lookup_host((host.as_str(), 0))
                .await
                .map_err(|err| -> Box<dyn std::error::Error + Send + Sync> { Box::new(err) })?;
            let allowed: Vec<SocketAddr> = addrs
                .filter(|addr| block_reason(addr.ip(), allow_private).is_none())
                .collect();
            if allowed.is_empty() {
                return Err("all resolved addresses are blocked by the SSRF policy".into());
            }
            let iter: Addrs = Box::new(allowed.into_iter());
            Ok(iter)
        })
    }
}
