//! The minisign check of the update manifest against a list of accepted public keys.

use minisign_verify::{Error, PublicKey, Signature};

use crate::error::UpdateError;

/// Longest signature text accepted (64 KiB, like every answer of the check).
const MAX_SIGNATURE_LEN: usize = 64 * 1024;

/// Read a key given as the text of a `.pub` file or as the bare base64 line.
fn parse_key(text: &str) -> Option<PublicKey> {
    PublicKey::decode(text).ok().or_else(|| {
        let line = text
            .lines()
            .map(str::trim)
            .find(|l| !l.is_empty() && !l.starts_with("untrusted comment:"))?;
        PublicKey::from_base64(line).ok()
    })
}

/// Verify `signature` (the text of a `.minisig` file) over `manifest` with one of `keys`.
///
/// Only keys whose id matches the signature are tried. `UnknownKey` means none matched, `Signature` that a
/// matching key rejected the manifest, the signature or its trusted comment. Keys that cannot be read are
/// skipped. Legacy (non-prehashed) signatures are refused.
pub fn verify(manifest: &[u8], signature: &str, keys: &[&str]) -> Result<(), UpdateError> {
    if signature.len() > MAX_SIGNATURE_LEN {
        return Err(UpdateError::Signature);
    }
    let signature = Signature::decode(signature).map_err(|_| UpdateError::Signature)?;
    let mut matched = false;
    for key in keys.iter().filter_map(|k| parse_key(k)) {
        match key.verify(manifest, &signature, false) {
            Ok(()) => return Ok(()),
            Err(Error::UnexpectedKeyId) => {}
            Err(_) => matched = true,
        }
    }
    Err(if matched {
        UpdateError::Signature
    } else {
        UpdateError::UnknownKey
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &[u8] = include_bytes!("../tests/fixtures/manifest.json");
    const TAMPERED: &[u8] = include_bytes!("../tests/fixtures/manifest-tampered.json");
    const SIG: &str = include_str!("../tests/fixtures/manifest.json.minisig");
    const SIG_SECOND: &str = include_str!("../tests/fixtures/manifest-second.json.minisig");
    const SIG_OTHER: &str = include_str!("../tests/fixtures/manifest-other.json.minisig");
    const TEST_PUB: &str = include_str!("../tests/keys/test.pub");
    const SECOND_PUB: &str = include_str!("../tests/keys/second.pub");
    const OTHER_PUB: &str = include_str!("../tests/keys/other.pub");

    #[test]
    fn good_signature_verifies() {
        assert!(verify(MANIFEST, SIG, &[TEST_PUB]).is_ok());
    }

    #[test]
    fn bare_key_line_is_accepted() {
        let bare = TEST_PUB.lines().last().unwrap();
        assert!(verify(MANIFEST, SIG, &[bare]).is_ok());
    }

    #[test]
    fn tampered_manifest_fails_with_signature() {
        assert!(matches!(
            verify(TAMPERED, SIG, &[TEST_PUB]),
            Err(UpdateError::Signature)
        ));
    }

    #[test]
    fn signature_of_other_key_is_unknown_key() {
        assert!(matches!(
            verify(MANIFEST, SIG_OTHER, &[TEST_PUB, SECOND_PUB]),
            Err(UpdateError::UnknownKey)
        ));
    }

    #[test]
    fn second_key_in_the_list_verifies() {
        assert!(verify(MANIFEST, SIG_SECOND, &[TEST_PUB, SECOND_PUB]).is_ok());
        assert!(verify(MANIFEST, SIG, &[SECOND_PUB, TEST_PUB]).is_ok());
    }

    #[test]
    fn other_key_alone_verifies_its_own_signature() {
        assert!(verify(MANIFEST, SIG_OTHER, &[OTHER_PUB]).is_ok());
    }

    #[test]
    fn altered_trusted_comment_fails() {
        let bad = SIG.replace("freemapper test fixture", "freemapper test fixtuRe");
        assert!(matches!(
            verify(MANIFEST, &bad, &[TEST_PUB]),
            Err(UpdateError::Signature)
        ));
    }

    #[test]
    fn junk_and_oversize_signature_fail() {
        assert!(matches!(
            verify(MANIFEST, "junk", &[TEST_PUB]),
            Err(UpdateError::Signature)
        ));
        let big = format!("{SIG}{}", "x".repeat(MAX_SIGNATURE_LEN));
        assert!(matches!(
            verify(MANIFEST, &big, &[TEST_PUB]),
            Err(UpdateError::Signature)
        ));
    }

    #[test]
    fn unusable_key_in_list_is_skipped() {
        assert!(verify(MANIFEST, SIG, &["not a key", TEST_PUB]).is_ok());
        assert!(matches!(
            verify(MANIFEST, SIG, &[]),
            Err(UpdateError::UnknownKey)
        ));
    }

    #[test]
    fn fixture_manifest_parses() {
        assert!(crate::parse_manifest(MANIFEST, "freemapper").is_ok());
    }
}
