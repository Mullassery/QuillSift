use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener};
use std::sync::{Arc, Mutex};
use std::thread;

use serde_json::{json, Value};

use quillsift_core::{FetchPolicy, ScrapeError};
use quillsift_fetch::Fetcher;
use quillsift_llm::{build_provider, ChatModel, ChatRequest, Ollama, OpenAiCompat, ProviderConfig};

#[derive(Debug, Clone)]
struct CapturedRequest {
    path: String,
    headers: String,
    body: Vec<u8>,
}

fn spawn_server(
    status: u16,
    response_body: &'static str,
) -> (SocketAddr, Arc<Mutex<Vec<CapturedRequest>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind provider server");
    let addr = listener.local_addr().expect("provider address");
    let captured = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&captured);
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut data = Vec::new();
            let mut chunk = [0u8; 2048];
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
            let body = data[header_end..data.len().min(header_end + length)].to_vec();
            let request_line = headers.lines().next().unwrap_or_default();
            let path = request_line
                .split_whitespace()
                .nth(1)
                .unwrap_or("/")
                .to_owned();
            sink.lock().unwrap().push(CapturedRequest {
                path,
                headers: headers.clone(),
                body,
            });

            let reason = match status {
                200 => "OK",
                500 => "Internal Server Error",
                _ => "Status",
            };
            let response = format!(
                "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                response_body.len(),
                response_body
            );
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();
        }
    });
    (addr, captured)
}

fn fetcher(allow_private: bool, bearer: Option<&str>) -> Fetcher {
    let mut policy = FetchPolicy {
        allow_private_networks: allow_private,
        ..FetchPolicy::default()
    };
    if let Some(token) = bearer {
        policy.headers.insert(
            "authorization",
            format!("Bearer {token}").parse().expect("header value"),
        );
    }
    Fetcher::new(policy).expect("fetcher")
}

#[tokio::test]
async fn openai_sends_messages_and_parses_content() {
    let (addr, captured) = spawn_server(200, r#"{"choices":[{"message":{"content":"hello"}}]}"#);
    let provider = OpenAiCompat::new(
        fetcher(true, Some("sk-test")),
        &format!("http://{addr}/v1"),
        "gpt-test",
    )
    .unwrap();

    let request = ChatRequest::new("system policy", "user content")
        .with_temperature(0.0)
        .with_max_tokens(64);
    assert_eq!(provider.complete(&request).await.unwrap(), "hello");
    assert_eq!(provider.provider(), "openai-compatible");
    assert_eq!(provider.model(), "gpt-test");

    let captured = captured.lock().unwrap();
    let sent = &captured[0];
    assert_eq!(sent.path, "/v1/chat/completions");
    assert!(sent
        .headers
        .to_ascii_lowercase()
        .contains("authorization: bearer sk-test"));
    let body: Value = serde_json::from_slice(&sent.body).unwrap();
    assert_eq!(body["model"], "gpt-test");
    assert_eq!(body["messages"][0]["role"], "system");
    assert_eq!(body["messages"][0]["content"], "system policy");
    assert_eq!(body["messages"][1]["role"], "user");
    assert_eq!(body["messages"][1]["content"], "user content");
    assert_eq!(body["max_tokens"], 64);
    assert_eq!(body["response_format"]["type"], "json_object");
}

#[tokio::test]
async fn openai_accepts_array_content_parts() {
    let (addr, _) = spawn_server(
        200,
        r#"{"choices":[{"message":{"content":[{"type":"text","text":"a"},{"type":"text","text":"b"}]}}]}"#,
    );
    let provider =
        OpenAiCompat::new(fetcher(true, None), &format!("http://{addr}/v1"), "m").unwrap();
    let out = provider
        .complete(&ChatRequest::new("s", "u"))
        .await
        .unwrap();
    assert_eq!(out, "ab");
}

#[tokio::test]
async fn ollama_sends_chat_options() {
    let (addr, captured) = spawn_server(200, r#"{"message":{"content":"local"}}"#);
    let provider = Ollama::new(fetcher(true, None), &format!("http://{addr}"), "llama3").unwrap();
    let out = provider
        .complete(&ChatRequest::new("sys", "usr").with_max_tokens(32))
        .await
        .unwrap();
    assert_eq!(out, "local");
    assert_eq!(provider.provider(), "ollama");

    let captured = captured.lock().unwrap();
    let sent = &captured[0];
    assert_eq!(sent.path, "/api/chat");
    let body: Value = serde_json::from_slice(&sent.body).unwrap();
    assert_eq!(body["stream"], false);
    assert_eq!(body["format"], "json");
    assert_eq!(body["options"]["temperature"], 0.0);
    assert_eq!(body["options"]["num_predict"], 32);
    assert_eq!(body["messages"][0]["content"], "sys");
}

#[tokio::test]
async fn http_errors_include_status_and_body() {
    let (addr, _) = spawn_server(500, "provider exploded");
    let provider =
        OpenAiCompat::new(fetcher(true, None), &format!("http://{addr}/v1"), "m").unwrap();
    let err = provider
        .complete(&ChatRequest::new("s", "u"))
        .await
        .unwrap_err();
    match err {
        ScrapeError::Llm(message) => {
            assert!(message.contains("500"), "{message}");
            assert!(message.contains("provider exploded"), "{message}");
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[tokio::test]
async fn invalid_json_and_missing_content_are_errors() {
    let (addr, _) = spawn_server(200, "not json");
    let provider =
        OpenAiCompat::new(fetcher(true, None), &format!("http://{addr}/v1"), "m").unwrap();
    let err = provider
        .complete(&ChatRequest::new("s", "u"))
        .await
        .unwrap_err();
    assert!(matches!(err, ScrapeError::Llm(_)), "{err}");

    let (addr, _) = spawn_server(200, "{}");
    let provider =
        OpenAiCompat::new(fetcher(true, None), &format!("http://{addr}/v1"), "m").unwrap();
    let err = provider
        .complete(&ChatRequest::new("s", "u"))
        .await
        .unwrap_err();
    assert!(matches!(err, ScrapeError::Llm(_)), "{err}");
}

#[tokio::test]
async fn provider_endpoints_obey_ssrf_policy() {
    let (addr, _) = spawn_server(200, "{}");
    let provider = OpenAiCompat::new(
        fetcher(false, None),
        &format!("http://127.0.0.1:{}/v1", addr.port()),
        "m",
    )
    .unwrap();
    let err = provider
        .complete(&ChatRequest::new("s", "u"))
        .await
        .unwrap_err();
    assert!(matches!(err, ScrapeError::AddressBlocked(_)), "{err}");
}

#[test]
fn provider_construction_validates_inputs() {
    let addr = spawn_server(200, "{}").0;
    assert!(matches!(
        OpenAiCompat::new(fetcher(true, None), "ftp://example.com/v1", "m"),
        Err(ScrapeError::UrlNotAllowed(_))
    ));
    assert!(matches!(
        OpenAiCompat::new(fetcher(true, None), "http://user:pass@example.com/v1", "m"),
        Err(ScrapeError::UrlNotAllowed(_))
    ));
    assert!(matches!(
        OpenAiCompat::new(fetcher(true, None), &format!("http://{addr}/v1"), ""),
        Err(ScrapeError::Config(_))
    ));
    assert!(matches!(
        Ollama::new(fetcher(true, None), &format!("http://{addr}"), "   "),
        Err(ScrapeError::Config(_))
    ));
}

#[test]
fn build_provider_dispatches_by_kind() {
    let addr = spawn_server(200, "{}").0;
    let config = ProviderConfig::openai_compatible(format!("http://{addr}/v1"), "m");
    let provider = build_provider(fetcher(true, None), &config).unwrap();
    assert_eq!(provider.provider(), "openai-compatible");

    let config = ProviderConfig::ollama(format!("http://{addr}"), "m");
    let provider = build_provider(fetcher(true, None), &config).unwrap();
    assert_eq!(provider.provider(), "ollama");
}

#[test]
fn chat_request_builder_sets_fields() {
    let request = ChatRequest::new("s", "u")
        .with_temperature(0.2)
        .with_max_tokens(10);
    assert_eq!(request.system, "s");
    assert_eq!(request.user, "u");
    assert!((request.temperature - 0.2).abs() < f32::EPSILON);
    assert_eq!(request.max_tokens, Some(10));
    assert_eq!(json!(request.max_tokens), json!(10));
}
