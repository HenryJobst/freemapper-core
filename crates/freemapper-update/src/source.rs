//! The host rule: where an update may be fetched from.

use crate::error::UpdateError;

/// The only hosts a manifest, signature or asset may come from, directly or by redirect.
pub const ALLOWED_HOSTS: [&str; 3] = [
    "github.com",
    "release-assets.githubusercontent.com",
    "objects.githubusercontent.com",
];

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
