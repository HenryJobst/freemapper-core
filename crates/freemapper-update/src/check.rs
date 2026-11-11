//! The update check (spec 3.2): fetch the signed manifest, verify it, compare versions.

use crate::error::UpdateError;
use crate::manifest::{Asset, MAX_MANIFEST_BYTES, parse_manifest};
use crate::platform::current_platform;
use crate::signature::verify;
use crate::source::{App, UpdateSource, manifest_url};
use crate::version::Version;

const MAX_SIGNATURE_BYTES: u64 = 64 * 1024;

/// What a check found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Nothing to do.
    Current,
    /// A release to install, with the asset for this platform.
    Available {
        version: Version,
        notes: String,
        asset: Asset,
    },
}

/// Checks for an update. Without `version` the latest release counts, and only a newer one is
/// `Available`. With `version` that release is fetched: the same version as the running one is
/// `Current`, any other (also an older one, a deliberate downgrade) is `Available`.
/// A bad signature or unknown key is an error, never `Current`.
pub fn check_for_update(
    app: &App,
    source: &dyn UpdateSource,
    version: Option<&Version>,
) -> Result<Outcome, UpdateError> {
    let url = manifest_url(app, version);
    let signature_url = format!("{url}.minisig");
    let bytes = source.get(&url, MAX_MANIFEST_BYTES as u64)?;
    let signature = source.get(&signature_url, MAX_SIGNATURE_BYTES)?;
    let signature = String::from_utf8(signature).map_err(|_| UpdateError::Signature)?;
    verify(&bytes, &signature, app.keys)?;
    let manifest = parse_manifest(&bytes, app.name)?;
    if let Some(wanted) = version
        && manifest.version != *wanted
    {
        return Err(UpdateError::Manifest(format!(
            "asked for {wanted}, the manifest is for {}",
            manifest.version
        )));
    }
    let newer = manifest.version > app.current;
    if manifest.version == app.current || (version.is_none() && !newer) {
        return Ok(Outcome::Current);
    }
    if newer && manifest.min_self_update_from > app.current {
        return Err(UpdateError::TooOldToSelfUpdate {
            needed: manifest.min_self_update_from,
        });
    }
    let platform =
        current_platform().ok_or_else(|| UpdateError::NoAssetForPlatform("unknown".into()))?;
    let asset = manifest
        .assets
        .get(platform)
        .cloned()
        .ok_or_else(|| UpdateError::NoAssetForPlatform(platform.into()))?;
    Ok(Outcome::Available {
        version: manifest.version,
        notes: manifest.notes,
        asset,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    const TEST_PUB: &str = include_str!("../tests/keys/test.pub");

    const REPO: &str = "https://github.com/HenryJobst/freemapper";

    /// Serves committed fixtures by URL.
    struct FakeSource(HashMap<String, Vec<u8>>);

    impl UpdateSource for FakeSource {
        fn get(&self, url: &str, max_bytes: u64) -> Result<Vec<u8>, UpdateError> {
            crate::check_url(url)?;
            let body = self
                .0
                .get(url)
                .ok_or_else(|| UpdateError::Download(format!("{url}: 404")))?;
            if body.len() as u64 > max_bytes {
                return Err(UpdateError::Download("too large".into()));
            }
            Ok(body.clone())
        }

        fn download_to(
            &self,
            _url: &str,
            _size: u64,
            _file: &mut std::fs::File,
        ) -> Result<(), UpdateError> {
            unreachable!("the check does not download assets")
        }
    }

    fn fixture(name: &str) -> Vec<u8> {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        std::fs::read(dir.join(name)).unwrap()
    }

    fn app(current: &str, keys: &'static [&'static str]) -> App {
        App {
            name: "freemapper",
            repo_url: REPO,
            keys,
            current: current.parse().unwrap(),
        }
    }

    /// Serves `manifest` and `signature` as the latest release and as release v0.11.0 / v0.12.0.
    fn serving(manifest: &str, signature: &str) -> FakeSource {
        let mut map = HashMap::new();
        let latest = manifest_url(&app("0.1.0", &[]), None);
        for version in ["0.11.0", "0.12.0"] {
            let url = manifest_url(&app("0.1.0", &[]), Some(&version.parse().unwrap()));
            map.insert(url.clone(), fixture(manifest));
            map.insert(format!("{url}.minisig"), fixture(signature));
        }
        map.insert(latest.clone(), fixture(manifest));
        map.insert(format!("{latest}.minisig"), fixture(signature));
        FakeSource(map)
    }

    fn standard() -> FakeSource {
        serving("manifest.json", "manifest.json.minisig")
    }

    static KEYS: [&str; 1] = [TEST_PUB];

    #[test]
    fn manifest_urls() {
        let a = app("0.10.0", &KEYS);
        assert_eq!(
            manifest_url(&a, None),
            format!("{REPO}/releases/latest/download/freemapper-update.json")
        );
        assert_eq!(
            manifest_url(&a, Some(&"0.9.1".parse().unwrap())),
            format!("{REPO}/releases/download/v0.9.1/freemapper-update.json")
        );
    }

    #[test]
    fn check_finds_newer_version() {
        let source = serving("manifest-next.json", "manifest-next.json.minisig");
        let outcome = check_for_update(&app("0.11.0", &KEYS), &source, None).unwrap();
        let Outcome::Available {
            version,
            notes,
            asset,
        } = outcome
        else {
            panic!("expected Available");
        };
        assert_eq!(version, "0.12.0".parse().unwrap());
        assert!(notes.ends_with("v0.11.0"));
        assert!(asset.url.contains(current_platform().unwrap()));
    }

    #[test]
    fn check_equal_version_is_current() {
        let outcome = check_for_update(&app("0.11.0", &KEYS), &standard(), None).unwrap();
        assert_eq!(outcome, Outcome::Current);
    }

    #[test]
    fn check_older_latest_is_current() {
        let outcome = check_for_update(&app("0.12.0", &KEYS), &standard(), None).unwrap();
        assert_eq!(outcome, Outcome::Current);
    }

    #[test]
    fn check_bad_signature_is_an_error_not_current() {
        // Tampered manifest under the signature of the real one.
        let source = serving("manifest-tampered.json", "manifest.json.minisig");
        assert!(matches!(
            check_for_update(&app("0.11.0", &KEYS), &source, None),
            Err(UpdateError::Signature)
        ));
        // Signed by a key the app does not trust.
        let source = serving("manifest.json", "manifest-other.json.minisig");
        assert!(matches!(
            check_for_update(&app("0.11.0", &KEYS), &source, None),
            Err(UpdateError::UnknownKey)
        ));
        // A missing signature is an error too.
        let mut source = standard();
        source.0.retain(|url, _| !url.ends_with(".minisig"));
        assert!(check_for_update(&app("0.10.0", &KEYS), &source, None).is_err());
    }

    #[test]
    fn check_missing_platform_is_no_asset_error() {
        let source = serving("manifest-noassets.json", "manifest-noassets.json.minisig");
        assert!(matches!(
            check_for_update(&app("0.11.0", &KEYS), &source, None),
            Err(UpdateError::NoAssetForPlatform(_))
        ));
        // Nothing to install, nothing missing.
        assert_eq!(
            check_for_update(&app("0.12.0", &KEYS), &source, None).unwrap(),
            Outcome::Current
        );
    }

    #[test]
    fn check_too_old_to_self_update() {
        let source = serving("manifest-min12.json", "manifest-min12.json.minisig");
        let err = check_for_update(&app("0.10.0", &KEYS), &source, None).unwrap_err();
        assert!(
            matches!(err, UpdateError::TooOldToSelfUpdate { needed } if needed == "0.12.0".parse().unwrap())
        );
        // Current enough: fine.
        assert!(matches!(
            check_for_update(&app("0.12.0", &KEYS), &source, None).unwrap(),
            Outcome::Current
        ));
    }

    #[test]
    fn explicit_version_may_be_older() {
        let wanted: Version = "0.11.0".parse().unwrap();
        let outcome = check_for_update(&app("0.12.0", &KEYS), &standard(), Some(&wanted)).unwrap();
        assert!(matches!(outcome, Outcome::Available { version, .. } if version == wanted));
        let same = check_for_update(&app("0.11.0", &KEYS), &standard(), Some(&wanted)).unwrap();
        assert_eq!(same, Outcome::Current);
    }

    #[test]
    fn explicit_version_must_match_the_manifest() {
        // The v0.12.0 URL serves the 0.11.0 manifest: a replayed older release.
        let wanted: Version = "0.12.0".parse().unwrap();
        assert!(matches!(
            check_for_update(&app("0.10.0", &KEYS), &standard(), Some(&wanted)),
            Err(UpdateError::Manifest(_))
        ));
    }
}
