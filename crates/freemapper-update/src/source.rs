//! The host rule: where an update may be fetched from.

use crate::error::UpdateError;
use crate::version::Version;

/// The only hosts a manifest, signature or asset may come from, directly or by redirect.
pub const ALLOWED_HOSTS: [&str; 3] = [
    "github.com",
    "release-assets.githubusercontent.com",
    "objects.githubusercontent.com",
];

/// The network, as the apps provide it. Every URL passes [`check_url`] on the way, redirects included.
pub trait UpdateSource {
    /// Fetches `url` into memory; fails when the body exceeds `max_bytes`.
    fn get(&self, url: &str, max_bytes: u64) -> Result<Vec<u8>, UpdateError>;
    /// Streams `url` into `file`; fails when the body is not exactly `size` bytes.
    fn download_to(
        &self,
        url: &str,
        size: u64,
        file: &mut std::fs::File,
    ) -> Result<(), UpdateError>;
}

/// What the update code needs to know about the running app.
#[derive(Debug, Clone)]
pub struct App {
    /// Short name, as in the manifest's `app` field and the manifest file name.
    pub name: &'static str,
    /// Repository URL without a trailing slash.
    pub repo_url: &'static str,
    /// Trusted minisign public keys (a list, so a key can be rotated).
    pub keys: &'static [&'static str],
    /// The running version.
    pub current: Version,
}

/// The manifest URL: the latest release, or the release `v{version}`.
pub fn manifest_url(app: &App, version: Option<&Version>) -> String {
    match version {
        None => format!(
            "{}/releases/latest/download/{}-update.json",
            app.repo_url, app.name
        ),
        Some(v) => format!(
            "{}/releases/download/v{v}/{}-update.json",
            app.repo_url, app.name
        ),
    }
}

/// Accepts `https://` URLs without user info, on port 443 (or none), whose host is exactly one of
/// [`ALLOWED_HOSTS`] (ASCII case-insensitive). Parsed by hand: no URL crate.
pub fn check_url(url: &str) -> Result<(), UpdateError> {
    let refuse = |why: &str| UpdateError::Url(format!("{url}: {why}"));
    if url
        .bytes()
        .any(|b| b.is_ascii_control() || b == b' ' || b == b'\\')
    {
        return Err(refuse("unexpected character"));
    }
    let rest = match url.get(..8) {
        Some(scheme) if scheme.eq_ignore_ascii_case("https://") => &url[8..],
        _ => return Err(refuse("not https")),
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    if authority.contains('@') {
        return Err(refuse("user info"));
    }
    let (host, port) = match authority.split_once(':') {
        Some((host, port)) => (host, Some(port)),
        None => (authority, None),
    };
    if port.is_some_and(|port| port != "443") {
        return Err(refuse("port"));
    }
    if ALLOWED_HOSTS
        .iter()
        .any(|allowed| host.eq_ignore_ascii_case(allowed))
    {
        Ok(())
    } else {
        Err(refuse("host not allowed"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_url_accepts_the_three_hosts_only() {
        for ok in [
            "https://github.com/HenryJobst/freemapper/releases/latest/download/freemapper-update.json",
            "https://release-assets.githubusercontent.com/x?y=1",
            "https://objects.githubusercontent.com:443/x",
            "https://GitHub.com/x",
            "https://github.com",
        ] {
            assert!(check_url(ok).is_ok(), "{ok}");
        }
        for bad in [
            "http://github.com/x",
            "https://github.com:8443/x",
            "https://user@github.com/x",
            "https://user:pw@github.com/x",
            "https://github.com.evil.example/x",
            "https://github.com./x",
            "https://evilgithub.com/x",
            "https://example.com/github.com",
            "https://example.com#@github.com",
            "https://example.com?@github.com",
            "https://github.com:/x",
            "https://github.com\\@evil.example/x",
            "https://github.com /x",
            "ftp://github.com/x",
            "github.com/x",
            "https://",
            "",
        ] {
            assert!(matches!(check_url(bad), Err(UpdateError::Url(_))), "{bad}");
        }
    }
}
