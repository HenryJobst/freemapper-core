//! Shell for the local web pages of the suite.
//!
//! [`serve`] starts an HTTP server on 127.0.0.1 and wraps the app's routes in the guards every local page
//! needs: a random token as the first path segment, a `Host` allowlist against DNS rebinding, an `Origin`
//! check, and fixed security headers. The app brings its own routes, handlers and assets; this crate knows
//! nothing about maps, projects or tiles. See decision 0020.
#![forbid(unsafe_code)]

use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use axum::extract::Request;
use axum::http::{HeaderValue, Method, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use tokio::sync::Notify;

#[cfg(test)]
mod tests;

/// What can go wrong before the server runs.
#[derive(Debug, thiserror::Error)]
pub enum WebError {
    /// 127.0.0.1 could not be bound; `port` is the requested one (0 = system-chosen).
    #[error("cannot listen on 127.0.0.1:{port}: {source}")]
    Bind {
        port: u16,
        #[source]
        source: std::io::Error,
    },
    /// The operating system has no random source for the token.
    #[error("no random source: {0}")]
    NoRandom(String),
}

/// How [`serve`] starts.
#[derive(Debug, Clone, Copy)]
pub struct ServeOptions {
    /// Port on 127.0.0.1; 0 lets the system choose a free one.
    pub port: u16,
    /// The `Content-Security-Policy` header value sent with every response.
    pub csp: &'static str,
    /// Strict rules for requests that change state (every method but GET, HEAD and OPTIONS): an `Origin`
    /// header equal to the page's origin is required, and the content type must be `application/json` or
    /// `application/octet-stream`. A cross-site form cannot meet either. With `false`, an `Origin` that is
    /// sent must still match, but a POST may omit it and use any content type.
    pub strict_post: bool,
}

/// A running local server.
#[derive(Debug)]
pub struct LocalServer {
    /// `http://127.0.0.1:{port}/{token}/`
    pub url: String,
    /// 128 random bits as 32 hex digits, the first path segment of every route.
    pub token: String,
    done: Arc<Notify>,
    task: tokio::task::JoinHandle<std::io::Result<()>>,
}

impl LocalServer {
    /// Call `notify_one()` on it to stop the server gracefully. There is no timeout.
    pub fn shutdown(&self) -> Arc<Notify> {
        Arc::clone(&self.done)
    }

    /// Waits until the server has stopped.
    pub async fn finished(self) -> std::io::Result<()> {
        self.task.await.map_err(std::io::Error::other)?
    }
}

struct Guard {
    hosts: [String; 2],
    origins: [String; 2],
    csp: HeaderValue,
    strict_post: bool,
}

/// Binds 127.0.0.1, calls `build` with the path prefix `/{token}` and serves the routes it returns behind
/// the guards.
pub async fn serve(
    build: impl FnOnce(&str) -> Router,
    options: ServeOptions,
) -> Result<LocalServer, WebError> {
    let bind_error = |source| WebError::Bind {
        port: options.port,
        source,
    };
    let listener = tokio::net::TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], options.port)))
        .await
        .map_err(bind_error)?;
    let port = listener.local_addr().map_err(bind_error)?.port();
    let token = token()?;
    let hosts = [format!("127.0.0.1:{port}"), format!("localhost:{port}")];
    let guard = Arc::new(Guard {
        origins: hosts.clone().map(|host| format!("http://{host}")),
        hosts,
        csp: HeaderValue::from_static(options.csp),
        strict_post: options.strict_post,
    });
    let app = build(&format!("/{token}")).layer(middleware::from_fn(
        move |request: Request, next: Next| {
            let guard = Arc::clone(&guard);
            async move { guarded(&guard, request, next).await }
        },
    ));
    let done = Arc::new(Notify::new());
    let stop = Arc::clone(&done);
    let task = tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async move { stop.notified().await })
            .await
    });
    Ok(LocalServer {
        url: format!("http://127.0.0.1:{port}/{token}/"),
        token,
        done,
        task,
    })
}

/// 128 random bits from the operating system, as hex.
fn token() -> Result<String, WebError> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|e| WebError::NoRandom(e.to_string()))?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

async fn guarded(guard: &Guard, request: Request, next: Next) -> Response {
    let headers = request.headers();
    let host = headers
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .unwrap_or_default();
    let origin = headers.get(header::ORIGIN);
    let mutating = !matches!(
        *request.method(),
        Method::GET | Method::HEAD | Method::OPTIONS
    );
    let allowed = guard.hosts.iter().any(|allowed| allowed == host)
        // Browsers send Origin on cross-site requests: only the page itself may call.
        && origin.is_none_or(|origin| guard.origins.iter().any(|allowed| allowed == origin))
        && (!(guard.strict_post && mutating) || (origin.is_some() && strict_content_type(headers)));
    let mut response = if allowed {
        next.run(request).await
    } else {
        StatusCode::FORBIDDEN.into_response()
    };
    let headers = response.headers_mut();
    // A handler may set its own lifetime (for example for static assets).
    if !headers.contains_key(header::CACHE_CONTROL) {
        headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    }
    headers.insert(header::CONTENT_SECURITY_POLICY, guard.csp.clone());
    headers.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    // The token is in the URL: never send it to another site.
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    response
}

/// `application/json` or `application/octet-stream`, parameters ignored. Neither is allowed in a plain
/// cross-site form, and a script needs a CORS preflight, which no route answers.
fn strict_content_type(headers: &axum::http::HeaderMap) -> bool {
    let Some(value) = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
    else {
        return false;
    };
    let essence = value.split(';').next().unwrap_or_default().trim();
    essence.eq_ignore_ascii_case("application/json")
        || essence.eq_ignore_ascii_case("application/octet-stream")
}

/// An embedded file by path, with the MIME type rust-embed derives from its name; 404 if there is none.
pub fn embedded<E: rust_embed::RustEmbed>(path: &str) -> Response {
    match E::get(path) {
        Some(file) => (
            [(header::CONTENT_TYPE, file.metadata.mimetype().to_string())],
            file.data.into_owned(),
        )
            .into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

/// Opens `url` in the default browser without waiting for it.
pub fn open_browser(url: &str) -> std::io::Result<()> {
    let mut command = if cfg!(target_os = "macos") {
        std::process::Command::new("open")
    } else if cfg!(windows) {
        let mut command = std::process::Command::new("cmd");
        command.args(["/C", "start", ""]);
        command
    } else {
        std::process::Command::new("xdg-open")
    };
    let mut child = command
        .arg(url)
        // The caller's stdin may be a protocol pipe (MCP).
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;
    std::thread::spawn(move || child.wait());
    Ok(())
}
