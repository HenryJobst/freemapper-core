# 0020. The shell for local web pages

Date: 2026-11-11
Status: Accepted

## Context
freemapper's area selection (`select.rs`) serves a page on 127.0.0.1 and guards it with a token, host and origin
checks and security headers. freemapper-courseplan's browser editor (M4) and freemapper-print's later editor need
the same guards. Copying them into each app would let the three copies drift apart, and a missed check on one of
them is a security hole. The suite rule is that shared code lives in core.

## Decision
- The crate `freemapper-web` holds the shell and nothing else: no tiles, maps, projects or routes. Each app
  brings its own routes through `serve(build, options)`. `build` receives the path prefix `/{token}` and the
  `Arc<tokio::sync::Notify>` that `LocalServer::shutdown()` returns and the graceful shutdown waits on, so a
  handler in the app (a `done` route) can stop the server *(agreed with freemapper)*.
- Rulings taken over from freemapper's `select.rs` *(agreed with freemapper)*:
  - Binds 127.0.0.1 only. Port 0 lets the system choose; a given port is forced, and a bind error names it.
  - The access token is a path prefix `/{token}/…`, not a header: 128 bits from `getrandom`, 32 hex digits. A
    wrong token gets 404 from the router.
  - The `Host` header must be exactly `127.0.0.1:{port}` or `localhost:{port}` (DNS rebinding); anything else
    gets 403.
  - An `Origin` header is checked only if the browser sends it, by exact compare with `http://127.0.0.1:{port}`
    and `http://localhost:{port}`; a mismatch gets 403.
  - Every response carries `Referrer-Policy: no-referrer` (the token is in the URL), `X-Frame-Options: DENY`,
    `X-Content-Type-Options: nosniff`, the app's CSP, and `Cache-Control: no-store` unless the handler set
    its own.
  - Shutdown goes through a `tokio::sync::Notify` and `with_graceful_shutdown`, without a timeout. The crate
    installs no Ctrl+C handler; the app decides.
- The CSP is a parameter (`ServeOptions::csp`), because select lists aerial-photo services in `img-src` and the
  editor does not.
- `ServeOptions::strict_post` (courseplan's stricter rule) *(Claude's call, for freemapper-courseplan)*: any
  request that is not GET, HEAD or OPTIONS needs an `Origin` equal to the page's own, and a content type of
  `application/json` or `application/octet-stream`. Neither can come from a cross-site HTML form, and a script
  needs a CORS preflight that no route answers. Default for freemapper is `false`: its POSTs may omit the
  `Origin` and use any content type, as before. Courseplan sets `true`. Checks that belong to an app, such as a
  revision header, stay in the app.
- Hardening beyond select.rs: exactly one `Host` and at most one `Origin` header; `Sec-Fetch-Site: cross-site`
  is refused; request bodies are limited by `ServeOptions::max_body` (1 MiB by default, a route may override it
  with its own `DefaultBodyLimit` layer); a CSP that is not a valid header value is `WebError::BadCsp`.
- `embedded::<E>(path)` serves a rust-embed file, or 404 for a missing file and for an empty, absolute,
  backslash, NUL, drive-prefixed or `..` path. The content type is fixed by extension for html, js, css, svg,
  png, webp, json and txt, with `; charset=utf-8` on text types. `open_browser(url)` starts the platform's
  opener without waiting and takes only the URL of `LocalServer`; other URLs are refused.
- freemapper-courseplan owns the crate (ADR 0005), as it brings it in.

## Consequences
- freemapper can replace its copy later without a change of behaviour; until then both exist.
- Pinned dependencies, all older than two weeks (ADR 0004): axum 0.8.9 (2026-04-14), rust-embed 8.13.0
  (2026-10-07), getrandom 0.4.3 (2026-06-17), tokio 1.54.1 (2026-10-21), the versions freemapper uses.
- An app that serves JSON or forms adds the axum features it needs; the crate enables only `http1` and `tokio`.
- No header-read timeouts and no connection limits: the server listens on loopback only, so only local
  processes can hold sockets.
- With `open_browser` the token is part of the opener's argument list and so visible in the process list of the
  machine while the opener runs. That is a known limit, shared with freemapper's select. A possible hardening is
  a one-time launch token in the URL that the first request exchanges for a cookie.
- `build` changed from `FnOnce(&str)` to `FnOnce(&str, Arc<Notify>)` before the first release; the app ignores
  the second argument if it does not need it. Dropping a `LocalServer` leaves the server running.
- The host allowlist assumes the browser reaches the page at the printed URL. A reverse proxy or another host
  name is refused.

Source: courseplan plan `2026-11-11-editor-m4.md` (Task 1, D2, D3), freemapper `select.rs`.
