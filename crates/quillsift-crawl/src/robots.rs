//! Minimal `robots.txt` parsing.
//!
//! The parser supports user-agent groups, `Allow`, `Disallow`, `Crawl-delay`,
//! `*` wildcards and `$` end anchors. Unknown directives are ignored. `Sitemap`
//! and other extensions do not affect crawling.

use std::time::Duration;

#[derive(Debug, Clone)]
struct Rule {
    allow: bool,
    pattern: String,
}

#[derive(Debug, Clone, Default)]
struct Group {
    agents: Vec<String>,
    rules: Vec<Rule>,
    crawl_delay: Option<Duration>,
}

/// The effective rules for one crawler identity and origin.
#[derive(Debug, Clone, Default)]
pub(crate) struct RobotsRules {
    rules: Vec<Rule>,
    crawl_delay: Option<Duration>,
    deny_all: bool,
}

impl RobotsRules {
    /// Builds rules that allow every path (used when `robots.txt` is absent).
    pub(crate) fn allow_all() -> Self {
        Self::default()
    }

    /// Builds rules that reject every path (used for `401`/`403` responses).
    pub(crate) fn deny_all() -> Self {
        Self {
            deny_all: true,
            ..Self::default()
        }
    }

    /// Returns true when `path_and_query` may be fetched.
    pub(crate) fn is_allowed(&self, path_and_query: &str) -> bool {
        if self.deny_all {
            return false;
        }
        self.rules
            .iter()
            .filter(|rule| pattern_matches(&rule.pattern, path_and_query))
            .max_by_key(|rule| (rule.pattern.len(), rule.allow))
            .is_none_or(|rule| rule.allow)
    }

    /// The longest `Crawl-delay` declared for the matching group.
    pub(crate) fn crawl_delay(&self) -> Option<Duration> {
        self.crawl_delay
    }
}

/// Parses `robots.txt` for the supplied product token (for example `"QuillSift"`).
pub(crate) fn parse_robots(text: &str, product_token: &str) -> RobotsRules {
    let groups = parse_groups(text);
    let token = normalize_agent(product_token);
    let specific: Vec<&Group> = groups
        .iter()
        .filter(|group| group.agents.iter().any(|agent| agent == &token))
        .collect();
    let matching: Vec<&Group> = if specific.is_empty() {
        groups
            .iter()
            .filter(|group| group.agents.iter().any(|agent| agent == "*"))
            .collect()
    } else {
        specific
    };

    let mut rules = Vec::new();
    let mut crawl_delay = None;
    for group in matching {
        rules.extend(group.rules.iter().cloned());
        if let Some(delay) = group.crawl_delay {
            crawl_delay = Some(crawl_delay.map_or(delay, |current: Duration| current.max(delay)));
        }
    }
    RobotsRules {
        rules,
        crawl_delay,
        deny_all: false,
    }
}

fn parse_groups(text: &str) -> Vec<Group> {
    let mut groups = Vec::new();
    let mut current = Group::default();
    let mut has_directives = false;

    for raw in text.lines() {
        let line = raw.split('#').next().unwrap_or("").trim();
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim().to_ascii_lowercase();
        let value = value.trim();
        match key.as_str() {
            "user-agent" => {
                if has_directives {
                    groups.push(std::mem::take(&mut current));
                    has_directives = false;
                }
                let agent = normalize_agent(value);
                if !agent.is_empty() {
                    current.agents.push(agent);
                }
            }
            "allow" | "disallow" => {
                if current.agents.is_empty() {
                    continue;
                }
                has_directives = true;
                if key == "disallow" && value.is_empty() {
                    continue;
                }
                if value.is_empty() {
                    continue;
                }
                current.rules.push(Rule {
                    allow: key == "allow",
                    pattern: value.to_owned(),
                });
            }
            "crawl-delay" => {
                if current.agents.is_empty() {
                    continue;
                }
                has_directives = true;
                if let Ok(seconds) = value.parse::<f64>() {
                    if seconds.is_finite() && seconds >= 0.0 {
                        current.crawl_delay = Some(Duration::from_secs_f64(seconds));
                    }
                }
            }
            _ => {}
        }
    }
    if !current.agents.is_empty() {
        groups.push(current);
    }
    groups
}

fn normalize_agent(value: &str) -> String {
    value
        .split('/')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase()
}

fn pattern_matches(pattern: &str, path_and_query: &str) -> bool {
    let anchored = pattern.ends_with('$');
    let pattern = pattern.strip_suffix('$').unwrap_or(pattern);
    wildcard_match(pattern.as_bytes(), path_and_query.as_bytes(), anchored)
}

fn wildcard_match(pattern: &[u8], value: &[u8], anchored: bool) -> bool {
    if pattern.is_empty() {
        return !anchored || value.is_empty();
    }
    match pattern[0] {
        b'*' => {
            wildcard_match(&pattern[1..], value, anchored)
                || (!value.is_empty() && wildcard_match(pattern, &value[1..], anchored))
        }
        byte => {
            value.first() == Some(&byte) && wildcard_match(&pattern[1..], &value[1..], anchored)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_everything_without_rules() {
        let rules = RobotsRules::allow_all();
        assert!(rules.is_allowed("/anything"));
        assert!(rules.crawl_delay().is_none());
    }

    #[test]
    fn deny_all_rejects_every_path() {
        assert!(!RobotsRules::deny_all().is_allowed("/"));
    }

    #[test]
    fn selects_specific_agent_and_longest_match() {
        let rules = parse_robots(
            "User-agent: *\nDisallow: /\n\nUser-agent: QuillSift\nDisallow: /private\nAllow: /private/public\n",
            "QuillSift/0.1.0",
        );
        assert!(rules.is_allowed("/"));
        assert!(!rules.is_allowed("/private"));
        assert!(rules.is_allowed("/private/public"));
    }

    #[test]
    fn falls_back_to_wildcard_group() {
        let rules = parse_robots("User-agent: *\nDisallow: /tmp\n", "quillsift");
        assert!(!rules.is_allowed("/tmp/file"));
        assert!(rules.is_allowed("/docs"));
    }

    #[test]
    fn supports_wildcards_and_end_anchors() {
        let rules = parse_robots(
            "User-agent: *\nDisallow: /*.pdf$\nDisallow: /search*\n",
            "quillsift",
        );
        assert!(!rules.is_allowed("/guide.pdf"));
        assert!(rules.is_allowed("/guide.pdf?download=1"));
        assert!(rules.is_allowed("/guide.pdf.html"));
        assert!(!rules.is_allowed("/search?q=owl"));
        assert!(rules.is_allowed("/docs/search"));
    }

    #[test]
    fn parses_crawl_delay_and_ignores_comments() {
        let rules = parse_robots(
            "# comment\nUser-agent: *\nCrawl-delay: 1.5\nDisallow: /x # inline\n",
            "quillsift",
        );
        assert_eq!(rules.crawl_delay(), Some(Duration::from_millis(1500)));
        assert!(!rules.is_allowed("/x/y"));
    }
}
