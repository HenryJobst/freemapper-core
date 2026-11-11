//! The update manifest, spec 3.1: one signed JSON file per release.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::error::UpdateError;
use crate::platform::PLATFORMS;
use crate::source::check_url;
use crate::version::Version;

/// The manifest and signature are capped at this many bytes.
pub const MAX_MANIFEST_BYTES: usize = 64 * 1024;

/// One release: what it is and where each platform's executable lives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    pub format: u32,
    pub app: String,
    pub version: Version,
    pub released: String,
    pub notes: String,
    pub min_self_update_from: Version,
    /// Only the five known platforms; entries for others are dropped.
    pub assets: BTreeMap<String, Asset>,
}

/// A gzip-compressed executable for one platform.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Asset {
    pub url: String,
    pub size: u64,
    /// SHA-256 of the `.gz`, 64 lower-case hex characters.
    pub sha256: String,
    /// SHA-256 of the unpacked executable.
    pub exe_sha256: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Raw {
    format: u32,
    app: String,
    version: Version,
    released: String,
    notes: String,
    min_self_update_from: Version,
    assets: BTreeMap<String, serde_json::Value>,
}

fn is_hash(text: &str) -> bool {
    text.len() == 64 && text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// Parses and validates a manifest for `app`. Unknown top-level keys, another app, a format other
/// than 1, malformed hashes and asset URLs outside the host rule are errors; unknown platforms are
/// ignored so a release can add one.
pub fn parse_manifest(bytes: &[u8], app: &str) -> Result<Manifest, UpdateError> {
    if bytes.len() > MAX_MANIFEST_BYTES {
        return Err(UpdateError::Manifest("larger than 64 KiB".into()));
    }
    let raw: Raw =
        serde_json::from_slice(bytes).map_err(|e| UpdateError::Manifest(e.to_string()))?;
    if raw.format != 1 {
        return Err(UpdateError::Manifest(format!(
            "unsupported format {}",
            raw.format
        )));
    }
    if raw.app != app {
        return Err(UpdateError::Manifest(format!(
            "manifest is for {:?}, not {app:?}",
            raw.app
        )));
    }
    let mut assets = BTreeMap::new();
    for (platform, value) in raw.assets {
        if !PLATFORMS.contains(&platform.as_str()) {
            continue;
        }
        let asset: Asset = serde_json::from_value(value)
            .map_err(|e| UpdateError::Manifest(format!("asset {platform}: {e}")))?;
        if !is_hash(&asset.sha256) || !is_hash(&asset.exe_sha256) {
            return Err(UpdateError::Manifest(format!(
                "asset {platform}: hash is not 64 lower-case hex digits"
            )));
        }
        check_url(&asset.url)?;
        assets.insert(platform, asset);
    }
    Ok(Manifest {
        format: raw.format,
        app: raw.app,
        version: raw.version,
        released: raw.released,
        notes: raw.notes,
        min_self_update_from: raw.min_self_update_from,
        assets,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    const H1: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    const H2: &str = "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210";

    fn manifest_json() -> Value {
        let mut assets = serde_json::Map::new();
        for platform in PLATFORMS {
            assets.insert(
                platform.into(),
                json!({
                    "url": format!("https://github.com/HenryJobst/freemapper/releases/download/v0.11.0/freemapper-v0.11.0-{platform}.gz"),
                    "size": 12345678,
                    "sha256": H1,
                    "exe_sha256": H2,
                }),
            );
        }
        json!({
            "format": 1,
            "app": "freemapper",
            "version": "0.11.0",
            "released": "2026-11-20",
            "notes": "https://github.com/HenryJobst/freemapper/releases/tag/v0.11.0",
            "min_self_update_from": "0.11.0",
            "assets": assets,
        })
    }

    fn parse(value: &Value, app: &str) -> Result<Manifest, UpdateError> {
        parse_manifest(value.to_string().as_bytes(), app)
    }

    #[test]
    fn manifest_parses_the_spec_example() {
        let m = parse(&manifest_json(), "freemapper").unwrap();
        assert_eq!(m.format, 1);
        assert_eq!(m.app, "freemapper");
        assert_eq!(m.version, "0.11.0".parse().unwrap());
        assert_eq!(m.released, "2026-11-20");
        assert_eq!(m.min_self_update_from, "0.11.0".parse().unwrap());
        assert_eq!(m.assets.len(), 5);
        let asset = &m.assets["macos-arm64"];
        assert_eq!(asset.size, 12345678);
        assert_eq!(asset.sha256, H1);
        assert_eq!(asset.exe_sha256, H2);
        assert!(asset.url.ends_with("freemapper-v0.11.0-macos-arm64.gz"));
    }

    #[test]
    fn manifest_refuses_unknown_top_level_key() {
        let mut value = manifest_json();
        value["extra"] = json!(true);
        assert!(matches!(
            parse(&value, "freemapper"),
            Err(UpdateError::Manifest(_))
        ));
    }

    #[test]
    fn manifest_ignores_unknown_platform() {
        let mut value = manifest_json();
        value["assets"]["freebsd-riscv"] = json!({ "anything": [1, 2] });
        let m = parse(&value, "freemapper").unwrap();
        assert_eq!(m.assets.len(), 5);
        assert!(!m.assets.contains_key("freebsd-riscv"));
    }

    #[test]
    fn manifest_refuses_other_app() {
        let mut value = manifest_json();
        value["app"] = json!("freemapper-print");
        assert!(matches!(
            parse(&value, "freemapper"),
            Err(UpdateError::Manifest(_))
        ));
        assert!(parse(&value, "freemapper-print").is_ok());
    }

    #[test]
    fn manifest_refuses_format_2() {
        let mut value = manifest_json();
        value["format"] = json!(2);
        assert!(matches!(
            parse(&value, "freemapper"),
            Err(UpdateError::Manifest(_))
        ));
    }

    #[test]
    fn manifest_refuses_bad_hash_length() {
        for (field, bad) in [
            ("sha256", &H1[..63]),
            ("exe_sha256", &H2[..63]),
            (
                "sha256",
                "0123456789ABCDEF0123456789abcdef0123456789abcdef0123456789abcdef",
            ),
            (
                "sha256",
                "g123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            ),
        ] {
            let mut value = manifest_json();
            value["assets"]["linux-x86_64"][field] = json!(bad);
            assert!(
                matches!(parse(&value, "freemapper"), Err(UpdateError::Manifest(_))),
                "{field} {bad}"
            );
        }
    }

    #[test]
    fn manifest_refuses_asset_url_on_foreign_host() {
        let mut value = manifest_json();
        value["assets"]["linux-arm64"]["url"] =
            json!("https://github.com.evil.example/freemapper.gz");
        assert!(matches!(
            parse(&value, "freemapper"),
            Err(UpdateError::Url(_))
        ));
    }

    #[test]
    fn manifest_refuses_junk_and_oversize() {
        assert!(matches!(
            parse_manifest(b"not json", "freemapper"),
            Err(UpdateError::Manifest(_))
        ));
        let mut value = manifest_json();
        value["notes"] = json!("x".repeat(MAX_MANIFEST_BYTES));
        assert!(matches!(
            parse(&value, "freemapper"),
            Err(UpdateError::Manifest(_))
        ));
        let mut value = manifest_json();
        value["version"] = json!("0.11.0-rc1");
        assert!(matches!(
            parse(&value, "freemapper"),
            Err(UpdateError::Manifest(_))
        ));
    }
}
