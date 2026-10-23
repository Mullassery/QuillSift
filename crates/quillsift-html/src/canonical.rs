//! URL canonicalization used for deduplication and output normalization.

use url::Url;

/// Returns a canonical form of `url` for deduplication.
///
/// The fragment is removed. Host case and default ports are already normalized
/// by the [`url`] crate on parse. Path, query and trailing slashes are
/// preserved because altering them can change the resource identity.
pub fn canonicalize(url: &Url) -> Url {
    let mut canonical = url.clone();
    canonical.set_fragment(None);
    canonical
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_fragment() {
        let url = Url::parse("https://example.com/a?b=1#section").unwrap();
        assert_eq!(canonicalize(&url).as_str(), "https://example.com/a?b=1");
    }

    #[test]
    fn keeps_query_and_path() {
        let url = Url::parse("https://example.com/path/?x=1").unwrap();
        assert_eq!(canonicalize(&url).as_str(), "https://example.com/path/?x=1");
    }

    #[test]
    fn removes_default_port() {
        let url = Url::parse("https://example.com:443/a").unwrap();
        assert_eq!(canonicalize(&url).as_str(), "https://example.com/a");
    }
}
