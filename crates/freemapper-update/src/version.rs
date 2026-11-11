//! Versions of three integers, compared by hand. Pre-release suffixes are refused.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A version `major.minor.patch`. Field order makes the derived `Ord` the version order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

/// Why a text is not a version.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("not a version of the form major.minor.patch: {0:?}")]
pub struct ParseVersionError(String);

impl FromStr for Version {
    type Err = ParseVersionError;

    /// Accepts `1.2.3` and `v1.2.3`; refuses suffixes, missing or extra parts and leading zeros.
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let bad = || ParseVersionError(text.to_string());
        let digits = text.strip_prefix('v').unwrap_or(text);
        let mut parts = digits.split('.');
        let mut next = || -> Result<u32, ParseVersionError> {
            let part = parts.next().ok_or_else(bad)?;
            let plain = !part.is_empty()
                && part.bytes().all(|b| b.is_ascii_digit())
                && (part == "0" || !part.starts_with('0'));
            if !plain {
                return Err(bad());
            }
            part.parse().map_err(|_| bad())
        };
        let version = Version {
            major: next()?,
            minor: next()?,
            patch: next()?,
        };
        if parts.next().is_some() {
            return Err(bad());
        }
        Ok(version)
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

impl Serialize for Version {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Version {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        text.parse().map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(text: &str) -> Version {
        text.parse().unwrap()
    }

    #[test]
    fn version_parses_and_orders() {
        assert!(v("0.10.0") < v("0.11.0"));
        assert!(v("0.9.9") < v("0.10.0"));
        assert!(v("1.0.0") > v("0.99.99"));
        assert_eq!(v("v1.0.0"), v("1.0.0"));
        assert_eq!(v("0.0.0").to_string(), "0.0.0");
        assert_eq!(v("v12.3.40").to_string(), "12.3.40");
    }

    #[test]
    fn version_refuses_suffixes_and_junk() {
        for bad in [
            "1.0.0-rc1",
            "1.0",
            "1.0.0.0",
            "",
            "01.0.0",
            "1.00.0",
            "1.0.01",
            "v",
            "vv1.0.0",
            "1.0.x",
            " 1.0.0",
            "1.0.0 ",
            "+1.0.0",
            "-1.0.0",
            "1..0",
            "99999999999.0.0",
            "V1.0.0",
        ] {
            assert!(bad.parse::<Version>().is_err(), "{bad:?}");
        }
    }
}
