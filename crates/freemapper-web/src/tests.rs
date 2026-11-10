use super::*;
use axum::Router;
use axum::routing::{get, post};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

const CSP: &str = "default-src 'self'; frame-ancestors 'none'";

#[derive(rust_embed::RustEmbed)]
#[folder = "tests/assets"]
struct Assets;

fn routes(prefix: &str) -> Router {
    Router::new()
        .route(&format!("{prefix}/ping"), get(|| async { "pong" }))
        .route(
            &format!("{prefix}/echo"),
            post(|body: String| async move { body }),
        )
        .route(
            &format!("{prefix}/asset/{{*path}}"),
            get(
                |axum::extract::Path(path): axum::extract::Path<String>| async move {
                    embedded::<Assets>(&path)
                },
            ),
        )
        .route(
            &format!("{prefix}/cached"),
            get(|| async { ([("cache-control", "max-age=60")], "x") }),
        )
}

async fn start(strict_post: bool) -> LocalServer {
    serve(
        routes,
        ServeOptions {
            port: 0,
            csp: CSP,
            strict_post,
        },
    )
    .await
    .expect("serve")
}

fn port_of(server: &LocalServer) -> u16 {
    server
        .url
        .rsplit(':')
        .next()
        .and_then(|rest| rest.split('/').next())
        .and_then(|p| p.parse().ok())
        .expect("port in url")
}

struct Reply {
    status: u16,
    headers: Vec<(String, String)>,
    body: String,
}

impl Reply {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }
}

/// One raw HTTP/1.1 request; `headers` are sent as given, so a test can set any `Host`.
async fn request(
    port: u16,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: &str,
) -> Reply {
    let mut text = format!("{method} {path} HTTP/1.1\r\nConnection: close\r\n");
    for (name, value) in headers {
        text.push_str(&format!("{name}: {value}\r\n"));
    }
    text.push_str(&format!("Content-Length: {}\r\n\r\n{body}", body.len()));
    let mut stream = TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("connect");
    stream.write_all(text.as_bytes()).await.expect("write");
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).await.expect("read");
    let raw = String::from_utf8_lossy(&raw).into_owned();
    let (head, body) = raw.split_once("\r\n\r\n").expect("response head");
    let mut lines = head.lines();
    let status = lines
        .next()
        .and_then(|l| l.split(' ').nth(1))
        .and_then(|s| s.parse().ok())
        .expect("status");
    let headers = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(n, v)| (n.trim().to_ascii_lowercase(), v.trim().to_string()))
        .collect();
    Reply {
        status,
        headers,
        body: body.to_string(),
    }
}

fn host(port: u16) -> String {
    format!("127.0.0.1:{port}")
}

fn origin(port: u16) -> String {
    format!("http://127.0.0.1:{port}")
}

async fn get_ping(server: &LocalServer, host_header: &str) -> Reply {
    let port = port_of(server);
    let path = format!("/{}/ping", server.token);
    request(port, "GET", &path, &[("Host", host_header)], "").await
}

async fn post_echo(server: &LocalServer, origin_header: Option<&str>, content_type: &str) -> Reply {
    let port = port_of(server);
    let path = format!("/{}/echo", server.token);
    let host = host(port);
    let mut headers = vec![("Host", host.as_str()), ("Content-Type", content_type)];
    if let Some(origin) = origin_header {
        headers.push(("Origin", origin));
    }
    request(port, "POST", &path, &headers, "hi").await
}

#[tokio::test]
async fn token_is_32_hex_digits() {
    let server = start(false).await;
    assert_eq!(server.token.len(), 32);
    assert!(
        server
            .token
            .chars()
            .all(|c| matches!(c, '0'..='9' | 'a'..='f'))
    );
}

#[tokio::test]
async fn url_is_127_0_0_1_with_token() {
    let server = start(false).await;
    let port = port_of(&server);
    assert_ne!(port, 0);
    assert_eq!(
        server.url,
        format!("http://127.0.0.1:{port}/{}/", server.token)
    );
}

#[tokio::test]
async fn foreign_host_is_403() {
    let server = start(false).await;
    let port = port_of(&server);
    let reply = get_ping(&server, &format!("evil.example:{port}")).await;
    assert_eq!(reply.status, 403);
    assert_eq!(get_ping(&server, "127.0.0.1").await.status, 403);
}

#[tokio::test]
async fn localhost_host_is_allowed() {
    let server = start(false).await;
    let port = port_of(&server);
    let reply = get_ping(&server, &format!("localhost:{port}")).await;
    assert_eq!(reply.status, 200);
    assert_eq!(reply.body, "pong");
    assert_eq!(get_ping(&server, &host(port)).await.status, 200);
}

#[tokio::test]
async fn post_without_origin_is_403() {
    let server = start(true).await;
    let reply = post_echo(&server, None, "application/json").await;
    assert_eq!(reply.status, 403);
}

#[tokio::test]
async fn post_with_foreign_origin_is_403() {
    let server = start(true).await;
    let reply = post_echo(&server, Some("http://evil.example"), "application/json").await;
    assert_eq!(reply.status, 403);
    // Without strict_post a foreign Origin is refused too: it is sent, so it must match.
    let loose = start(false).await;
    let reply = post_echo(&loose, Some("http://evil.example"), "application/json").await;
    assert_eq!(reply.status, 403);
}

#[tokio::test]
async fn form_post_is_403() {
    let server = start(true).await;
    let origin = origin(port_of(&server));
    for content_type in [
        "application/x-www-form-urlencoded",
        "text/plain",
        "multipart/form-data; boundary=x",
    ] {
        let reply = post_echo(&server, Some(&origin), content_type).await;
        assert_eq!(reply.status, 403, "{content_type}");
    }
}

#[tokio::test]
async fn strict_post_allows_json_and_octet_stream() {
    let server = start(true).await;
    let origin = origin(port_of(&server));
    for content_type in [
        "application/json",
        "application/json; charset=utf-8",
        "application/octet-stream",
    ] {
        let reply = post_echo(&server, Some(&origin), content_type).await;
        assert_eq!(reply.status, 200, "{content_type}");
        assert_eq!(reply.body, "hi");
    }
}

#[tokio::test]
async fn strict_post_off_allows_form_post() {
    let server = start(false).await;
    let origin = origin(port_of(&server));
    let reply = post_echo(&server, None, "application/x-www-form-urlencoded").await;
    assert_eq!(reply.status, 200);
    let reply = post_echo(&server, Some(&origin), "text/plain").await;
    assert_eq!(reply.status, 200);
}

#[tokio::test]
async fn wrong_token_is_404() {
    let server = start(false).await;
    let port = port_of(&server);
    let wrong = "0".repeat(32);
    let path = format!("/{wrong}/ping");
    let reply = request(port, "GET", &path, &[("Host", &host(port))], "").await;
    assert_eq!(reply.status, 404);
    let reply = request(port, "GET", "/ping", &[("Host", &host(port))], "").await;
    assert_eq!(reply.status, 404);
}

#[tokio::test]
async fn headers_are_set() {
    let server = start(false).await;
    let port = port_of(&server);
    let reply = get_ping(&server, &host(port)).await;
    assert_eq!(reply.header("cache-control"), Some("no-store"));
    assert_eq!(reply.header("content-security-policy"), Some(CSP));
    assert_eq!(reply.header("x-frame-options"), Some("DENY"));
    assert_eq!(reply.header("referrer-policy"), Some("no-referrer"));
    assert_eq!(reply.header("x-content-type-options"), Some("nosniff"));
}

#[tokio::test]
async fn handler_cache_control_is_kept() {
    let server = start(false).await;
    let port = port_of(&server);
    let path = format!("/{}/cached", server.token);
    let reply = request(port, "GET", &path, &[("Host", &host(port))], "").await;
    assert_eq!(reply.header("cache-control"), Some("max-age=60"));
}

#[tokio::test]
async fn embedded_serves_mime_and_404() {
    let server = start(false).await;
    let port = port_of(&server);
    let get = |name: &str| format!("/{}/asset/{name}", server.token);
    let host = host(port);
    let page = request(port, "GET", &get("page.html"), &[("Host", &host)], "").await;
    assert_eq!(page.status, 200);
    assert!(
        page.header("content-type")
            .is_some_and(|t| t.starts_with("text/html"))
    );
    let text = request(port, "GET", &get("hello.txt"), &[("Host", &host)], "").await;
    assert_eq!(text.body, "hello\n");
    let missing = request(port, "GET", &get("nope.js"), &[("Host", &host)], "").await;
    assert_eq!(missing.status, 404);
}

#[tokio::test]
async fn bind_error_names_the_port() {
    let first = start(false).await;
    let port = port_of(&first);
    let error = serve(
        routes,
        ServeOptions {
            port,
            csp: CSP,
            strict_post: false,
        },
    )
    .await
    .expect_err("port is taken");
    assert!(matches!(error, WebError::Bind { port: p, .. } if p == port));
    assert!(error.to_string().contains(&port.to_string()));
}

#[tokio::test]
async fn shutdown_ends_finished() {
    let server = start(false).await;
    let notify = server.shutdown();
    notify.notify_one();
    let done = tokio::time::timeout(Duration::from_secs(5), server.finished())
        .await
        .expect("finished returns after shutdown");
    done.expect("clean end");
}
