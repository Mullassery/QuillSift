//! The bounded breadth-first crawler.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use futures_util::stream::{self, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tokio::time::Instant;
use tracing::{debug, instrument, warn};
use url::Url;

use scrape_core::{CrawlPolicy, Extracted, Extractor, FetchPolicy, Result, ScrapeError};
use scrape_fetch::Fetcher;
use scrape_html::StructuralExtractor;

use crate::robots::{parse_robots, RobotsRules};

/// Why a URL could not be included in a crawl report.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CrawlFailureKind {
    /// The HTTP request failed.
    Fetch,
    /// `robots.txt` disallowed the URL.
    Robots,
    /// The page was outside the configured crawl scope.
    Scope,
    /// The document was fetched but could not be parsed.
    Parse,
}

/// A non-fatal crawl failure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrawlFailure {
    /// The URL that failed.
    pub url: Url,
    /// Link depth of the URL, where seeds are depth `0`.
    pub depth: u32,
    /// Failure category.
    pub kind: CrawlFailureKind,
    /// Human-readable detail.
    pub message: String,
}

/// One fetched page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrawlPage {
    /// The URL that was requested.
    pub requested_url: Url,
    /// The final URL after redirects.
    pub url: Url,
    /// Link depth, where seeds are depth `0`.
    pub depth: u32,
    /// HTTP status code.
    pub status: u16,
    /// Response `Content-Type`, when present.
    pub content_type: Option<String>,
    /// Deterministic extraction, present only for HTML documents.
    pub content: Option<Extracted>,
}

/// The result of a crawl job.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrawlReport {
    /// Seed URLs supplied by the caller.
    pub seeds: Vec<Url>,
    /// Successfully fetched pages in breadth-first order.
    pub pages: Vec<CrawlPage>,
    /// Per-URL failures. A failure does not abort the job.
    pub failures: Vec<CrawlFailure>,
    /// True when the page-count bound stopped the crawl early.
    pub truncated: bool,
}

/// A bounded crawler sharing one SSRF-guarded fetcher and extraction policy.
#[derive(Debug, Clone)]
pub struct Crawler {
    fetcher: Fetcher,
    policy: CrawlPolicy,
    extractor: StructuralExtractor,
}

#[derive(Debug)]
struct Ready {
    url: Url,
    depth: u32,
    delay: Option<Duration>,
}

#[derive(Debug)]
struct PageOutcome {
    page: CrawlPage,
    links: Vec<Url>,
    failures: Vec<CrawlFailure>,
}

#[derive(Debug)]
enum Visit {
    Page(Box<PageOutcome>),
    Failed(CrawlFailure),
}

#[derive(Debug, Default)]
struct DelayGate {
    last: Mutex<HashMap<String, Instant>>,
}

impl DelayGate {
    async fn wait(&self, origin: &str, delay: Option<Duration>) {
        let Some(delay) = delay.filter(|delay| !delay.is_zero()) else {
            return;
        };
        let mut last = self.last.lock().await;
        if let Some(previous) = last.get(origin) {
            if let Some(remaining) = delay.checked_sub(previous.elapsed()) {
                tokio::time::sleep(remaining).await;
            }
        }
        last.insert(origin.to_owned(), Instant::now());
    }
}

impl Crawler {
    /// Builds a crawler from validated fetch and crawl policies.
    ///
    /// # Errors
    /// Returns [`ScrapeError::Config`] when either policy is invalid.
    pub fn new(fetch_policy: FetchPolicy, crawl_policy: CrawlPolicy) -> Result<Self> {
        crawl_policy.validate()?;
        Ok(Self {
            fetcher: Fetcher::new(fetch_policy)?,
            policy: crawl_policy,
            extractor: StructuralExtractor::new(),
        })
    }

    /// Returns the crawl policy.
    pub fn policy(&self) -> &CrawlPolicy {
        &self.policy
    }

    /// Crawls from one or more seeds using bounded breadth-first traversal.
    ///
    /// Per-page fetch/parse failures are reported in [`CrawlReport::failures`]
    /// and do not abort the job.
    ///
    /// # Errors
    /// Returns [`ScrapeError::Config`] when no seeds are supplied.
    #[instrument(skip(self, seeds), fields(seeds = seeds.len()))]
    pub async fn crawl(&self, seeds: &[Url]) -> Result<CrawlReport> {
        if seeds.is_empty() {
            return Err(ScrapeError::Config(
                "crawl requires at least one seed URL".into(),
            ));
        }

        let allowed_origins: HashSet<String> = seeds.iter().filter_map(origin_key).collect();
        let mut visited: HashSet<String> = HashSet::new();
        let mut frontier = Vec::new();
        for seed in seeds {
            let url = normalize_url(seed);
            if visited.insert(queue_key(&url)) {
                frontier.push((url, 0));
            }
        }

        let mut report = CrawlReport {
            seeds: seeds.to_vec(),
            pages: Vec::new(),
            failures: Vec::new(),
            truncated: false,
        };
        let mut robots_cache: HashMap<String, RobotsRules> = HashMap::new();
        let gate = DelayGate::default();
        let mut attempted = 0usize;

        for depth in 0..=self.policy.max_depth {
            if frontier.is_empty() {
                break;
            }
            if attempted >= self.policy.max_pages {
                report.truncated = true;
                break;
            }

            let (mut ready, failures) = self.prepare_frontier(frontier, &mut robots_cache).await;
            report.failures.extend(failures);
            if ready.is_empty() {
                break;
            }
            let remaining = self.policy.max_pages - attempted;
            if ready.len() > remaining {
                ready.truncate(remaining);
                report.truncated = true;
            }
            attempted += ready.len();

            let outcomes = self.fetch_all(ready, &allowed_origins, &gate).await;
            let (mut next, pages, failures) =
                self.consume_outcomes(outcomes, depth, &allowed_origins, &mut visited);
            report.pages.extend(pages);
            report.failures.extend(failures);

            let remaining = self.policy.max_pages - attempted;
            if next.len() > remaining {
                next.truncate(remaining);
                report.truncated = true;
            }
            frontier = next;
        }

        Ok(report)
    }

    async fn prepare_frontier(
        &self,
        frontier: Vec<(Url, u32)>,
        robots_cache: &mut HashMap<String, RobotsRules>,
    ) -> (Vec<Ready>, Vec<CrawlFailure>) {
        let mut ready = Vec::new();
        let mut failures = Vec::new();
        for (url, depth) in frontier {
            let Some(origin) = origin_key(&url) else {
                failures.push(CrawlFailure {
                    url,
                    depth,
                    kind: CrawlFailureKind::Scope,
                    message: "only http and https URLs can be crawled".into(),
                });
                continue;
            };

            let delay = if self.policy.respect_robots {
                if !robots_cache.contains_key(&origin) {
                    let rules = self.robots_for(&url).await;
                    robots_cache.insert(origin.clone(), rules);
                }
                let rules = &robots_cache[&origin];
                let path = path_and_query(&url);
                if !rules.is_allowed(&path) {
                    failures.push(CrawlFailure {
                        url,
                        depth,
                        kind: CrawlFailureKind::Robots,
                        message: format!("disallowed by robots.txt: {path}"),
                    });
                    continue;
                }
                max_delay(self.policy.crawl_delay, rules.crawl_delay())
            } else {
                self.policy.crawl_delay
            };
            ready.push(Ready { url, depth, delay });
        }
        (ready, failures)
    }

    async fn fetch_all(
        &self,
        ready: Vec<Ready>,
        allowed_origins: &HashSet<String>,
        gate: &DelayGate,
    ) -> Vec<Visit> {
        let concurrency = self.policy.max_concurrency.max(1);
        stream::iter(ready)
            .map(|item| async move { self.visit(item, allowed_origins, gate).await })
            .buffered(concurrency)
            .collect()
            .await
    }

    fn consume_outcomes(
        &self,
        outcomes: Vec<Visit>,
        depth: u32,
        allowed_origins: &HashSet<String>,
        visited: &mut HashSet<String>,
    ) -> (Vec<(Url, u32)>, Vec<CrawlPage>, Vec<CrawlFailure>) {
        let mut next = Vec::new();
        let mut pages = Vec::new();
        let mut failures = Vec::new();
        for outcome in outcomes {
            match outcome {
                Visit::Failed(failure) => failures.push(failure),
                Visit::Page(outcome) => {
                    visited.insert(queue_key(&outcome.page.url));
                    if depth < self.policy.max_depth {
                        for link in outcome.links {
                            let link = normalize_url(&link);
                            if self.in_scope(&link, allowed_origins)
                                && visited.insert(queue_key(&link))
                            {
                                next.push((link, depth + 1));
                            }
                        }
                    }
                    pages.push(outcome.page);
                    failures.extend(outcome.failures);
                }
            }
        }
        (next, pages, failures)
    }

    async fn visit(
        &self,
        item: Ready,
        allowed_origins: &HashSet<String>,
        gate: &DelayGate,
    ) -> Visit {
        let Ready { url, depth, delay } = item;
        if let Some(origin) = origin_key(&url) {
            gate.wait(&origin, delay).await;
        }

        let document = match self.fetcher.fetch(&url).await {
            Ok(document) => document,
            Err(error) => {
                return Visit::Failed(CrawlFailure {
                    url,
                    depth,
                    kind: CrawlFailureKind::Fetch,
                    message: error.to_string(),
                });
            }
        };

        if self.policy.same_origin_only {
            let in_scope =
                origin_key(&document.url).is_some_and(|origin| allowed_origins.contains(&origin));
            if !in_scope {
                return Visit::Failed(CrawlFailure {
                    url: document.url,
                    depth,
                    kind: CrawlFailureKind::Scope,
                    message: "redirect left the allowed origins".into(),
                });
            }
        }

        let mut failures = Vec::new();
        let mut links = Vec::new();
        let content = if is_html(document.content_type.as_deref()) {
            match self.extractor.extract(&document.html, &document.url) {
                Ok(extracted) => {
                    links.clone_from(&extracted.links);
                    Some(extracted)
                }
                Err(error) => {
                    failures.push(CrawlFailure {
                        url: document.url.clone(),
                        depth,
                        kind: CrawlFailureKind::Parse,
                        message: error.to_string(),
                    });
                    None
                }
            }
        } else {
            None
        };

        Visit::Page(Box::new(PageOutcome {
            page: CrawlPage {
                requested_url: url,
                url: document.url,
                depth,
                status: document.status,
                content_type: document.content_type,
                content,
            },
            links,
            failures,
        }))
    }

    async fn robots_for(&self, url: &Url) -> RobotsRules {
        let mut robots_url = url.clone();
        robots_url.set_path("/robots.txt");
        robots_url.set_query(None);
        robots_url.set_fragment(None);

        match self.fetcher.fetch(&robots_url).await {
            Ok(document) => parse_robots(&document.html, "ScrapeOwl"),
            Err(ScrapeError::HttpStatus { status, .. }) if matches!(status, 401 | 403) => {
                warn!(%robots_url, status, "robots.txt denied access; disallowing crawl");
                RobotsRules::deny_all()
            }
            Err(ScrapeError::HttpStatus { status, .. }) if matches!(status, 404 | 410) => {
                debug!(%robots_url, status, "robots.txt absent; allowing crawl");
                RobotsRules::allow_all()
            }
            Err(error) => {
                warn!(%robots_url, %error, "robots.txt unavailable; disallowing crawl");
                RobotsRules::deny_all()
            }
        }
    }

    fn in_scope(&self, url: &Url, allowed_origins: &HashSet<String>) -> bool {
        if !matches!(url.scheme(), "http" | "https") {
            return false;
        }
        if !self.policy.same_origin_only {
            return true;
        }
        origin_key(url).is_some_and(|origin| allowed_origins.contains(&origin))
    }
}

fn is_html(content_type: Option<&str>) -> bool {
    match content_type {
        Some(value) => {
            let mime = value
                .split(';')
                .next()
                .unwrap_or("")
                .trim()
                .to_ascii_lowercase();
            matches!(mime.as_str(), "text/html" | "application/xhtml+xml")
        }
        None => true,
    }
}

fn origin_key(url: &Url) -> Option<String> {
    matches!(url.scheme(), "http" | "https").then(|| url.origin().ascii_serialization())
}

fn normalize_url(url: &Url) -> Url {
    let mut normalized = url.clone();
    normalized.set_fragment(None);
    normalized
}

fn queue_key(url: &Url) -> String {
    normalize_url(url).to_string()
}

fn path_and_query(url: &Url) -> String {
    match url.query() {
        Some(query) => format!("{}?{query}", url.path()),
        None => url.path().to_owned(),
    }
}

fn max_delay(left: Option<Duration>, right: Option<Duration>) -> Option<Duration> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.max(right)),
        (left, right) => left.or(right),
    }
}
