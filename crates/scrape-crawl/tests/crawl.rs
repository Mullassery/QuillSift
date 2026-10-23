use std::fmt::Write as _;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener};
use std::sync::Arc;
use std::thread;

use scrape_core::{CrawlPolicy, FetchPolicy};
use scrape_crawl::{CrawlFailureKind, Crawler};
use url::Url;

type Reply = (u16, Vec<(String, String)>, Vec<u8>);

fn spawn_server<F>(handler: F) -> SocketAddr
where
    F: Fn(&str) -> Reply + Send + Sync + 'static,
{
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind fixture server");
    let addr = listener.local_addr().expect("fixture address");
    let handler = Arc::new(handler);
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut request = Vec::new();
            let mut chunk = [0u8; 1024];
            while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                match stream.read(&mut chunk) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => request.extend_from_slice(&chunk[..n]),
                }
            }
            let text = String::from_utf8_lossy(&request);
            let path = text.split_whitespace().nth(1).unwrap_or("/").to_owned();
            let (status, headers, body) = handler(&path);
            let reason = match status {
                200 => "OK",
                404 => "Not Found",
                _ => "Status",
            };
            let mut response = format!("HTTP/1.1 {status} {reason}\r\n");
            for (key, value) in &headers {
                let _ = write!(response, "{key}: {value}\r\n");
            }
            let _ = write!(
                response,
                "Content-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.write_all(&body);
            let _ = stream.flush();
        }
    });
    addr
}

fn html(body: &str) -> Reply {
    (
        200,
        vec![("Content-Type".into(), "text/html; charset=utf-8".into())],
        body.as_bytes().to_vec(),
    )
}

fn not_found() -> Reply {
    (
        404,
        vec![("Content-Type".into(), "text/plain".into())],
        Vec::new(),
    )
}

fn url(addr: SocketAddr, path: &str) -> Url {
    Url::parse(&format!("http://{addr}{path}")).expect("fixture url")
}

fn crawler(policy: CrawlPolicy) -> Crawler {
    let fetch = FetchPolicy {
        allow_private_networks: true,
        ..FetchPolicy::default()
    };
    Crawler::new(fetch, policy).expect("build crawler")
}

fn default_policy() -> CrawlPolicy {
    CrawlPolicy {
        respect_robots: false,
        ..CrawlPolicy::default()
    }
}

#[tokio::test]
async fn crawls_breadth_first_and_deduplicates() {
    let addr = spawn_server(|path| match path {
        "/" => html("<a href='/a'>a</a><a href='/b'>b</a><a href='/#frag'>again</a>"),
        "/a" => html("<a href='/c'>c</a><a href='/'>home</a>"),
        "/b" => html("<p>b</p>"),
        "/c" => html("<p>c</p>"),
        _ => not_found(),
    });
    let policy = CrawlPolicy {
        max_depth: 2,
        max_pages: 10,
        ..default_policy()
    };
    let report = crawler(policy).crawl(&[url(addr, "/")]).await.unwrap();

    let paths: Vec<&str> = report.pages.iter().map(|page| page.url.path()).collect();
    assert_eq!(paths, ["/", "/a", "/b", "/c"]);
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert!(!report.truncated);
}

#[tokio::test]
async fn stops_at_depth_limit() {
    let addr = spawn_server(|path| match path {
        "/" => html("<a href='/next'>next</a>"),
        "/next" => html("<a href='/deep'>deep</a>"),
        _ => html("<p>deep</p>"),
    });
    let policy = CrawlPolicy {
        max_depth: 1,
        max_pages: 10,
        ..default_policy()
    };
    let report = crawler(policy).crawl(&[url(addr, "/")]).await.unwrap();

    let paths: Vec<&str> = report.pages.iter().map(|page| page.url.path()).collect();
    assert_eq!(paths, ["/", "/next"]);
}

#[tokio::test]
async fn enforces_page_limit_and_marks_report_truncated() {
    let addr = spawn_server(|path| match path {
        "/" => html("<a href='/a'>a</a><a href='/b'>b</a>"),
        other => html(&format!("<p>{other}</p>")),
    });
    let policy = CrawlPolicy {
        max_depth: 2,
        max_pages: 2,
        ..default_policy()
    };
    let report = crawler(policy).crawl(&[url(addr, "/")]).await.unwrap();

    assert_eq!(report.pages.len(), 2);
    assert!(report.truncated);
}

#[tokio::test]
async fn keeps_crawl_same_origin_by_default() {
    let other = spawn_server(|_| html("<p>other</p>"));
    let addr = spawn_server(move |_| {
        html(&format!(
            "<a href='http://{other}/x'>other</a><a href='/local'>local</a>"
        ))
    });
    let report = crawler(CrawlPolicy {
        max_depth: 1,
        ..default_policy()
    })
    .crawl(&[url(addr, "/")])
    .await
    .unwrap();

    let paths: Vec<&str> = report.pages.iter().map(|page| page.url.path()).collect();
    assert_eq!(paths, ["/", "/local"]);
}

#[tokio::test]
async fn honors_robots_disallow_rules() {
    let addr = spawn_server(|path| match path {
        "/robots.txt" => (
            200,
            vec![("Content-Type".into(), "text/plain".into())],
            b"User-agent: *\nDisallow: /private\n".to_vec(),
        ),
        "/" => html("<a href='/private'>private</a><a href='/public'>public</a>"),
        "/public" => html("<p>public</p>"),
        "/private" => html("<p>secret</p>"),
        _ => not_found(),
    });
    let report = crawler(CrawlPolicy {
        max_depth: 1,
        respect_robots: true,
        ..CrawlPolicy::default()
    })
    .crawl(&[url(addr, "/")])
    .await
    .unwrap();

    let paths: Vec<&str> = report.pages.iter().map(|page| page.url.path()).collect();
    assert_eq!(paths, ["/", "/public"]);
    assert!(report
        .failures
        .iter()
        .any(|failure| failure.kind == CrawlFailureKind::Robots));
}

#[tokio::test]
async fn disallows_crawl_when_robots_is_unavailable() {
    let addr = spawn_server(|path| match path {
        "/robots.txt" => (500, Vec::new(), Vec::new()),
        _ => html("<p>page</p>"),
    });
    let report = crawler(CrawlPolicy {
        max_depth: 1,
        respect_robots: true,
        ..CrawlPolicy::default()
    })
    .crawl(&[url(addr, "/")])
    .await
    .unwrap();

    assert!(report.pages.is_empty());
    assert!(report
        .failures
        .iter()
        .any(|failure| failure.kind == CrawlFailureKind::Robots));
}

#[tokio::test]
async fn records_parseable_and_non_html_pages() {
    let addr = spawn_server(|path| match path {
        "/" => html("<a href='/data'>data</a><a href='/missing'>missing</a>"),
        "/data" => (
            200,
            vec![("Content-Type".into(), "application/json".into())],
            b"{\"ok\":true}".to_vec(),
        ),
        _ => not_found(),
    });
    let report = crawler(CrawlPolicy {
        max_depth: 1,
        ..default_policy()
    })
    .crawl(&[url(addr, "/")])
    .await
    .unwrap();

    let data = report
        .pages
        .iter()
        .find(|page| page.url.path() == "/data")
        .expect("data page");
    assert!(data.content.is_none());
    assert!(report
        .failures
        .iter()
        .any(|failure| failure.kind == CrawlFailureKind::Fetch));
}

#[tokio::test]
async fn rejects_empty_seed_list() {
    let error = crawler(default_policy()).crawl(&[]).await.unwrap_err();
    assert!(matches!(error, scrape_core::ScrapeError::Config(_)));
}
