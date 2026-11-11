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
use axum::extract::{DefaultBodyLimit, Path, Request};
use axum::http::{HeaderValue, Method, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use tokio::sync::Notify;

mod language;
mod revision;
#[cfg(test)]
mod tests;

pub use language::texts_language;
pub use revision::{REVISION_HEADER, refused, revision, stale};

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
    /// [`ServeOptions::csp`] is not a valid header value (for example it holds a line break).
    #[error("the Content-Security-Policy is not a valid header value")]
    BadCsp,
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
    ///
    /// A request without a body is not exempt: a body-less POST must still send
    /// `Content-Type: application/json` (in `fetch`: `headers` with that type and `body: "{}"`), or it gets 403.
    pub strict_post: bool,
    /// Largest request body in bytes, applied to every route through [`DefaultBodyLimit`]; a larger body gets
    /// 413 from the axum extractors (`Bytes`, `String`, `Json` …). A route may raise or lower it with its own
    /// `DefaultBodyLimit::max(..)` layer, which is inner and wins. [`DEFAULT_MAX_BODY`] by default.
    pub max_body: usize,
}

/// The default of [`ServeOptions::max_body`]: 1 MiB.
pub const DEFAULT_MAX_BODY: usize = 1024 * 1024;

impl ServeOptions {
    /// System-chosen port, no strict POST rules, [`DEFAULT_MAX_BODY`], and the given CSP.
    pub fn new(csp: &'static str) -> Self {
        Self {
            port: 0,
            csp,
            strict_post: false,
            max_body: DEFAULT_MAX_BODY,
        }
    }
}

/// A running local server.
///
/// Dropping it does not stop the server: the task keeps running until the process ends or the
/// [`Notify`] from [`LocalServer::shutdown`] is notified. Call [`LocalServer::finished`] to wait for it.
pub struct LocalServer {
    /// `http://127.0.0.1:{port}/{token}/`
    pub url: String,
    /// 128 random bits as 32 hex digits, the first path segment of every route.
    pub token: String,
    done: Arc<Notify>,
    task: tokio::task::JoinHandle<std::io::Result<()>>,
}

impl std::fmt::Debug for LocalServer {
    /// Prints no field: `url` and `token` are the access secret and must not reach a log.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LocalServer").finish_non_exhaustive()
    }
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

/// The suite assets of decision 0030: `suite.css`, `api.js`, `texts.js`, `status.js` and `tree.js`.
#[derive(rust_embed::RustEmbed)]
#[folder = "suite"]
struct Suite;

/// A suite asset by its exact name, or 404. The name is compared with the list first: in a debug build
/// rust-embed reads from the disk, and a case-insensitive file system would also find `SUITE.CSS`.
fn suite_file(path: &str) -> Response {
    if Suite::iter().any(|name| name == path) {
        embedded::<Suite>(path)
    } else {
        StatusCode::NOT_FOUND.into_response()
    }
}

/// Binds 127.0.0.1, calls `build` with the path prefix `/{token}` and the shutdown [`Notify`], and serves the
/// routes it returns behind the guards. The `Notify` is the one [`LocalServer::shutdown`] returns and the
/// graceful shutdown waits on, so a handler (for example `done`) can call `notify_one()` to stop the server.
///
/// Next to the app's routes it serves the suite assets at `/{token}/suite/<file>` (decision 0030), behind the
/// same guards and headers. The app must not add routes below `/{token}/suite/`; axum panics on the overlap.
pub async fn serve(
    build: impl FnOnce(&str, Arc<Notify>) -> Router,
    options: ServeOptions,
) -> Result<LocalServer, WebError> {
    let csp = HeaderValue::from_str(options.csp).map_err(|_| WebError::BadCsp)?;
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
        csp,
        strict_post: options.strict_post,
    });
    let done = Arc::new(Notify::new());
    let app = build(&format!("/{token}"), Arc::clone(&done))
        .route(
            &format!("/{token}/suite/{{*path}}"),
            get(|Path(path): Path<String>| async move { suite_file(&path) }),
        )
        .layer(DefaultBodyLimit::max(options.max_body))
        .layer(middleware::from_fn(move |request: Request, next: Next| {
            let guard = Arc::clone(&guard);
            async move { guarded(&guard, request, next).await }
        }));
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
    // Exactly one Host and at most one Origin: with duplicates the app could see another value than we checked.
    let mut hosts = headers.get_all(header::HOST).iter();
    let host = match (hosts.next(), hosts.next()) {
        (Some(host), None) => host.to_str().unwrap_or_default(),
        _ => "",
    };
    let mut origins = headers.get_all(header::ORIGIN).iter();
    let origin = origins.next();
    let one_origin = origins.next().is_none();
    let cross_site = headers
        .get("sec-fetch-site")
        .is_some_and(|v| v == "cross-site");
    let mutating = !matches!(
        *request.method(),
        Method::GET | Method::HEAD | Method::OPTIONS
    );
    let allowed = one_origin
        && !cross_site
        && guard.hosts.iter().any(|allowed| allowed == host)
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

/// An embedded file by path; 404 if there is none or the path is not a plain relative one (empty, absolute,
/// with a backslash, NUL, a `:` drive prefix or a `..` segment).
///
/// The content type comes from the extension for html, js, css, svg, png, webp, json and txt, and from
/// rust-embed otherwise. Text types carry `; charset=utf-8`.
pub fn embedded<E: rust_embed::RustEmbed>(path: &str) -> Response {
    if !plain_relative(path) {
        return StatusCode::NOT_FOUND.into_response();
    }
    match E::get(path) {
        Some(file) => {
            let content_type = content_type(path, file.metadata.mimetype());
            (
                [(header::CONTENT_TYPE, content_type)],
                file.data.into_owned(),
            )
                .into_response()
        }
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

fn plain_relative(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.contains(['\\', '\0'])
        && path.as_bytes().get(1) != Some(&b':')
        && path.split('/').all(|segment| segment != "..")
}

fn content_type(path: &str, guessed: &str) -> String {
    let extension = path.rsplit_once('.').map_or("", |(_, e)| e);
    let fixed = match extension.to_ascii_lowercase().as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "svg" => "image/svg+xml; charset=utf-8",
        "json" => "application/json; charset=utf-8",
        "txt" => "text/plain; charset=utf-8",
        "png" => "image/png",
        "webp" => "image/webp",
        _ => {
            return if guessed.starts_with("text/") && !guessed.contains("charset") {
                format!("{guessed}; charset=utf-8")
            } else {
                guessed.to_string()
            };
        }
    };
    fixed.to_string()
}

/// Opens `url` in the default browser without waiting for it. Only [`LocalServer::url`] belongs here: a URL
/// that does not start with `http://127.0.0.1:` is refused with an `InvalidInput` error, because the opener
/// is a shell command on some platforms. The token is then visible in the process list while the opener runs.
pub fn open_browser(url: &str) -> std::io::Result<()> {
    if !url.starts_with("http://127.0.0.1:")
        || url.contains(|c: char| c.is_whitespace() || c == '"')
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "only the URL of the local server can be opened",
        ));
    }
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
