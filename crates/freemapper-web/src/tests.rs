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

fn routes(prefix: &str, done: Arc<Notify>) -> Router {
    Router::new()
        .route(
            &format!("{prefix}/done"),
            get(move || {
                let done = Arc::clone(&done);
                async move {
                    done.notify_one();
                    "bye"
                }
            }),
        )
        .route(
            &format!("{prefix}/big"),
            post(|body: axum::body::Bytes| async move { body.len().to_string() })
                .layer(DefaultBodyLimit::max(4096)),
        )
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
    start_with(ServeOptions {
        strict_post,
        ..ServeOptions::new(CSP)
    })
    .await
}

async fn start_with(options: ServeOptions) -> LocalServer {
    serve(routes, options).await.expect("serve")
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
async fn bind_error_names_the_port() {
    let first = start(false).await;
    let port = port_of(&first);
    let error = serve(
        routes,
        ServeOptions {
            port,
            ..ServeOptions::new(CSP)
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

async fn asset(server: &LocalServer, name: &str) -> Reply {
    let port = port_of(server);
    let path = format!("/{}/asset/{name}", server.token);
    request(port, "GET", &path, &[("Host", &host(port))], "").await
}

#[tokio::test]
async fn embedded_sets_content_types() {
    let server = start(false).await;
    for (name, content_type) in [
        ("page.html", "text/html; charset=utf-8"),
        ("app.js", "text/javascript; charset=utf-8"),
        ("style.css", "text/css; charset=utf-8"),
        ("icon.svg", "image/svg+xml; charset=utf-8"),
        ("data.json", "application/json; charset=utf-8"),
        ("hello.txt", "text/plain; charset=utf-8"),
        ("pixel.png", "image/png"),
        ("pixel.webp", "image/webp"),
    ] {
        let reply = asset(&server, name).await;
        assert_eq!(reply.status, 200, "{name}");
        assert_eq!(reply.header("content-type"), Some(content_type), "{name}");
    }
    assert_eq!(asset(&server, "hello.txt").await.body, "hello\n");
    assert_eq!(asset(&server, "nope.js").await.status, 404);
}

#[test]
fn embedded_refuses_odd_paths() {
    for path in [
        "",
        "/page.html",
        "/etc/passwd",
        "..",
        "../page.html",
        "a/../page.html",
        "a/..",
        "..\\page.html",
        "a\\page.html",
        "page.html\0",
        "C:/page.html",
        "C:page.html",
    ] {
        assert_eq!(
            embedded::<Assets>(path).status(),
            StatusCode::NOT_FOUND,
            "{path:?}"
        );
    }
    assert_eq!(embedded::<Assets>("page.html").status(), StatusCode::OK);
}

#[tokio::test]
async fn done_route_stops_the_server() {
    let server = start(false).await;
    let port = port_of(&server);
    let path = format!("/{}/done", server.token);
    let reply = request(port, "GET", &path, &[("Host", &host(port))], "").await;
    assert_eq!(reply.body, "bye");
    tokio::time::timeout(Duration::from_secs(5), server.finished())
        .await
        .expect("finished returns after the route notified")
        .expect("clean end");
}

#[tokio::test]
async fn shutdown_with_an_open_idle_connection() {
    let server = start(false).await;
    let port = port_of(&server);
    let _idle = TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("connect");
    let mut kept = TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("connect");
    let text = format!(
        "GET /{}/ping HTTP/1.1\r\nHost: {}\r\n\r\n",
        server.token,
        host(port)
    );
    kept.write_all(text.as_bytes()).await.expect("write");
    let mut buffer = [0u8; 1024];
    let _ = kept.read(&mut buffer).await.expect("read");
    server.shutdown().notify_one();
    tokio::time::timeout(Duration::from_secs(5), server.finished())
        .await
        .expect("finished returns with open connections")
        .expect("clean end");
}

#[tokio::test]
async fn strict_post_bodyless_without_content_type_is_403() {
    let server = start(true).await;
    let port = port_of(&server);
    let path = format!("/{}/echo", server.token);
    let (host, origin) = (host(port), origin(port));
    let headers = [("Host", host.as_str()), ("Origin", origin.as_str())];
    let reply = request(port, "POST", &path, &headers, "").await;
    assert_eq!(reply.status, 403);
    let with_type = [headers[0], headers[1], ("Content-Type", "application/json")];
    let reply = request(port, "POST", &path, &with_type, "").await;
    assert_eq!(reply.status, 200);
}

#[test]
fn default_max_body_is_one_mebibyte() {
    assert_eq!(ServeOptions::new(CSP).max_body, 1024 * 1024);
    assert_eq!(DEFAULT_MAX_BODY, 1_048_576);
}

#[tokio::test]
async fn body_over_max_body_is_413_and_route_override_wins() {
    let server = start_with(ServeOptions {
        max_body: 1024,
        ..ServeOptions::new(CSP)
    })
    .await;
    let port = port_of(&server);
    let host = host(port);
    let headers = [("Host", host.as_str())];
    let echo = format!("/{}/echo", server.token);
    let big = format!("/{}/big", server.token);
    let ok = "a".repeat(1024);
    let over = "a".repeat(1025);
    assert_eq!(
        request(port, "POST", &echo, &headers, &ok).await.status,
        200
    );
    assert_eq!(
        request(port, "POST", &echo, &headers, &over).await.status,
        413
    );
    // The route's own, larger limit replaces the shell's.
    let reply = request(port, "POST", &big, &headers, &"a".repeat(3000)).await;
    assert_eq!(reply.status, 200);
    assert_eq!(reply.body, "3000");
    let reply = request(port, "POST", &big, &headers, &"a".repeat(5000)).await;
    assert_eq!(reply.status, 413);
}

#[tokio::test]
async fn duplicate_host_or_origin_is_403() {
    let server = start(false).await;
    let port = port_of(&server);
    let path = format!("/{}/ping", server.token);
    let good = host(port);
    let reply = request(
        port,
        "GET",
        &path,
        &[("Host", &good), ("Host", "evil.example")],
        "",
    )
    .await;
    assert_eq!(reply.status, 403);
    let reply = request(port, "GET", &path, &[], "").await;
    assert_eq!(reply.status, 403, "no Host at all");
    let o = origin(port);
    let reply = request(
        port,
        "GET",
        &path,
        &[("Host", &good), ("Origin", &o), ("Origin", &o)],
        "",
    )
    .await;
    assert_eq!(reply.status, 403);
    let reply = request(port, "GET", &path, &[("Host", &good), ("Origin", &o)], "").await;
    assert_eq!(reply.status, 200);
}

#[tokio::test]
async fn origin_null_and_odd_hosts_are_403() {
    let server = start(false).await;
    let port = port_of(&server);
    let path = format!("/{}/ping", server.token);
    let good = host(port);
    for method in ["GET", "OPTIONS"] {
        let reply = request(
            port,
            method,
            &path,
            &[("Host", &good), ("Origin", "null")],
            "",
        )
        .await;
        assert_eq!(reply.status, 403, "{method}");
    }
    let upper = origin(port).to_uppercase();
    let reply = request(
        port,
        "GET",
        &path,
        &[("Host", &good), ("Origin", &upper)],
        "",
    )
    .await;
    assert_eq!(reply.status, 403);
    for odd in [
        format!("LOCALHOST:{port}"),
        format!("localhost.:{port}"),
        format!("[::1]:{port}"),
        format!("a@127.0.0.1:{port}"),
        "127.0.0.1:1".to_string(),
    ] {
        assert_eq!(get_ping(&server, &odd).await.status, 403, "{odd}");
    }
}

#[tokio::test]
async fn foreign_host_on_unknown_path_is_403_with_headers() {
    let server = start(false).await;
    let port = port_of(&server);
    let reply = request(
        port,
        "GET",
        "/nothing/here",
        &[("Host", "evil.example")],
        "",
    )
    .await;
    assert_eq!(reply.status, 403);
    assert_eq!(reply.header("content-security-policy"), Some(CSP));
    assert_eq!(reply.header("x-content-type-options"), Some("nosniff"));
}

#[tokio::test]
async fn sec_fetch_site_cross_site_is_403() {
    let server = start(false).await;
    let port = port_of(&server);
    let path = format!("/{}/ping", server.token);
    let good = host(port);
    for (site, status) in [("cross-site", 403), ("same-origin", 200), ("none", 200)] {
        let headers = [("Host", good.as_str()), ("Sec-Fetch-Site", site)];
        assert_eq!(
            request(port, "GET", &path, &headers, "").await.status,
            status,
            "{site}"
        );
    }
}

#[tokio::test]
async fn bad_csp_is_an_error_not_a_panic() {
    let error = serve(routes, ServeOptions::new("default-src 'self'\nx"))
        .await
        .expect_err("line break in header value");
    assert!(matches!(error, WebError::BadCsp));
}

#[tokio::test]
async fn debug_does_not_print_the_token() {
    let server = start(false).await;
    let text = format!("{server:?}");
    assert!(!text.contains(&server.token));
}

#[test]
fn open_browser_refuses_foreign_urls() {
    for url in [
        "https://evil.example/",
        "http://localhost:1/",
        "-x",
        "http://127.0.0.1:1/ & calc",
    ] {
        let error = open_browser(url).expect_err(url);
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput, "{url}");
    }
}

#[test]
fn tokens_are_unique() {
    let tokens: std::collections::HashSet<String> =
        (0..100).map(|_| token().expect("token")).collect();
    assert_eq!(tokens.len(), 100);
}
