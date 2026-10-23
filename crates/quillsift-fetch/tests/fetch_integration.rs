use std::fmt::Write as _;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use quillsift_core::{FetchPolicy, ScrapeError};
use quillsift_fetch::Fetcher;
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
                302 => "Found",
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

fn html_reply() -> Reply {
    (
        200,
        vec![("Content-Type".into(), "text/html".into())],
        b"<html><body><h1>ok</h1></body></html>".to_vec(),
    )
}

fn local_policy() -> FetchPolicy {
    FetchPolicy {
        allow_private_networks: true,
        ..FetchPolicy::default()
    }
}

fn fetcher(policy: FetchPolicy) -> Fetcher {
    Fetcher::new(policy).expect("build fetcher")
}

fn url(addr: SocketAddr, path: &str) -> Url {
    Url::parse(&format!("http://{addr}{path}")).expect("fixture url")
}

#[tokio::test]
async fn fetches_html_from_local_server() {
    let addr = spawn_server(|_| html_reply());
    let document = fetcher(local_policy())
        .fetch(&url(addr, "/"))
        .await
        .unwrap();
    assert_eq!(document.status, 200);
    assert_eq!(document.content_type.as_deref(), Some("text/html"));
    assert!(document.html.contains("ok"));
}

#[tokio::test]
async fn blocks_loopback_literal_by_default() {
    let addr = spawn_server(|_| html_reply());
    let err = fetcher(FetchPolicy::default())
        .fetch(&url(addr, "/"))
        .await
        .unwrap_err();
    assert!(matches!(err, ScrapeError::AddressBlocked(_)), "{err}");
}

#[tokio::test]
async fn blocks_hostname_resolving_to_loopback() {
    let addr = spawn_server(|_| html_reply());
    let target = Url::parse(&format!("http://localhost:{}/", addr.port())).unwrap();
    let err = fetcher(FetchPolicy::default())
        .fetch(&target)
        .await
        .unwrap_err();
    assert!(matches!(err, ScrapeError::AddressBlocked(_)), "{err}");
}

#[tokio::test]
async fn rejects_non_http_schemes() {
    let target = Url::parse("file:///etc/passwd").unwrap();
    let err = fetcher(FetchPolicy::default())
        .fetch(&target)
        .await
        .unwrap_err();
    assert!(matches!(err, ScrapeError::UrlNotAllowed(_)), "{err}");
}

#[tokio::test]
async fn enforces_response_size_limit() {
    let addr = spawn_server(|_| (200, Vec::new(), vec![b'x'; 4096]));
    let policy = FetchPolicy {
        allow_private_networks: true,
        max_response_bytes: 64,
        max_decompressed_bytes: 64,
        ..FetchPolicy::default()
    };
    let err = fetcher(policy).fetch(&url(addr, "/")).await.unwrap_err();
    assert!(
        matches!(err, ScrapeError::ResponseTooLarge { limit: 64 }),
        "{err}"
    );
}

#[tokio::test]
async fn follows_relative_redirects() {
    let addr = spawn_server(|path| match path {
        "/start" => (302, vec![("Location".into(), "/final".into())], Vec::new()),
        "/final" => html_reply(),
        _ => (404, Vec::new(), b"not found".to_vec()),
    });
    let document = fetcher(local_policy())
        .fetch(&url(addr, "/start"))
        .await
        .unwrap();
    assert_eq!(document.url.path(), "/final");
    assert_eq!(document.status, 200);
}

#[tokio::test]
async fn blocks_redirect_to_cloud_metadata() {
    let addr = spawn_server(|_| {
        (
            302,
            vec![(
                "Location".into(),
                "http://169.254.169.254/latest/meta-data/".into(),
            )],
            Vec::new(),
        )
    });
    let err = fetcher(local_policy())
        .fetch(&url(addr, "/"))
        .await
        .unwrap_err();
    assert!(matches!(err, ScrapeError::AddressBlocked(_)), "{err}");
}

#[tokio::test]
async fn enforces_redirect_limit() {
    let addr = spawn_server(|_| (302, vec![("Location".into(), "/loop".into())], Vec::new()));
    let policy = FetchPolicy {
        allow_private_networks: true,
        max_redirects: 2,
        ..FetchPolicy::default()
    };
    let err = fetcher(policy).fetch(&url(addr, "/")).await.unwrap_err();
    assert!(
        matches!(err, ScrapeError::TooManyRedirects { limit: 2 }),
        "{err}"
    );
}

#[tokio::test]
async fn reports_non_success_status() {
    let addr = spawn_server(|_| (404, Vec::new(), b"gone".to_vec()));
    let err = fetcher(local_policy())
        .fetch(&url(addr, "/missing"))
        .await
        .unwrap_err();
    match err {
        ScrapeError::HttpStatus { status, .. } => assert_eq!(status, 404),
        other => panic!("unexpected error: {other}"),
    }
}

#[tokio::test]
async fn times_out_when_server_is_slow() {
    let addr = spawn_server(|_| {
        thread::sleep(Duration::from_millis(800));
        html_reply()
    });
    let policy = FetchPolicy {
        allow_private_networks: true,
        connect_timeout: Duration::from_millis(200),
        total_timeout: Duration::from_millis(300),
        ..FetchPolicy::default()
    };
    let err = fetcher(policy).fetch(&url(addr, "/")).await.unwrap_err();
    assert!(matches!(err, ScrapeError::Timeout), "{err}");
}

#[tokio::test]
async fn enforces_decompressed_size_limit() {
    use flate2::write::GzEncoder;
    use flate2::Compression;

    let mut encoder = GzEncoder::new(Vec::new(), Compression::best());
    encoder.write_all(&vec![b'a'; 1_000_000]).unwrap();
    let compressed = encoder.finish().unwrap();
    assert!(compressed.len() < 4096, "fixture should compress well");

    let addr = spawn_server(move |_| {
        (
            200,
            vec![
                ("Content-Type".into(), "text/html".into()),
                ("Content-Encoding".into(), "gzip".into()),
            ],
            compressed.clone(),
        )
    });
    let policy = FetchPolicy {
        allow_private_networks: true,
        max_response_bytes: 16 * 1024,
        max_decompressed_bytes: 16 * 1024,
        ..FetchPolicy::default()
    };
    let err = fetcher(policy).fetch(&url(addr, "/")).await.unwrap_err();
    assert!(
        matches!(err, ScrapeError::ResponseTooLarge { limit: 16_384 }),
        "{err}"
    );
}

fn spawn_echo_server() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind echo server");
    let addr = listener.local_addr().expect("echo address");
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut data = Vec::new();
            let mut chunk = [0u8; 1024];
            loop {
                match stream.read(&mut chunk) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => data.extend_from_slice(&chunk[..n]),
                }
                if data.windows(4).any(|window| window == b"\r\n\r\n") {
                    break;
                }
            }
            let header_end = data
                .windows(4)
                .position(|window| window == b"\r\n\r\n")
                .map_or(data.len(), |index| index + 4);
            let headers = String::from_utf8_lossy(&data[..header_end]).into_owned();
            let length = headers
                .lines()
                .find_map(|line| {
                    let (key, value) = line.split_once(':')?;
                    key.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().ok())
                        .flatten()
                })
                .unwrap_or(0);
            while data.len() < header_end + length {
                match stream.read(&mut chunk) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => data.extend_from_slice(&chunk[..n]),
                }
            }
            let body = &data[header_end..data.len().min(header_end + length)];
            let head = format!(
                "HTTP/1.1 201 Created\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.write_all(body);
            let _ = stream.flush();
        }
    });
    addr
}

#[tokio::test]
async fn post_json_sends_body_and_returns_status() {
    let addr = spawn_echo_server();
    let body = serde_json::json!({ "model": "test", "messages": [] });
    let response = fetcher(local_policy())
        .post_json(&url(addr, "/v1/chat/completions"), &body)
        .await
        .unwrap();
    assert_eq!(response.status, 201);
    let echoed: serde_json::Value = serde_json::from_str(&response.body).unwrap();
    assert_eq!(echoed, body);
}

#[tokio::test]
async fn post_json_enforces_ssrf_policy() {
    let addr = spawn_echo_server();
    let err = fetcher(FetchPolicy::default())
        .post_json(&url(addr, "/"), &serde_json::json!({}))
        .await
        .unwrap_err();
    assert!(matches!(err, ScrapeError::AddressBlocked(_)), "{err}");
}
