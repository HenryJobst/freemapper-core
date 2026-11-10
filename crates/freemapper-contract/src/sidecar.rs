//! The contract keys of `freemapper.toml`. Every other key is freemapper's own record and is ignored.

use std::collections::BTreeMap;

use crate::ContractError;

pub(crate) const KEY_MAP_KIND: &str = "map_kind";
pub(crate) const KEY_TITLE: &str = "title";
pub(crate) const KEY_SCALE: &str = "scale_denominator";
pub(crate) const KEY_CRS: &str = "crs";

const CONTRACT_KEYS: [&str; 4] = [KEY_MAP_KIND, KEY_TITLE, KEY_SCALE, KEY_CRS];

/// The contract keys of a sidecar. A key that is missing or has the wrong type leaves its field `None`.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Sidecar {
    pub map_kind: Option<String>,
    pub title: Option<String>,
    pub scale_denominator: Option<i64>,
    pub crs: Option<i64>,
    /// The contract keys that are present, with TOML's name of their type (`string`, `integer`, …).
    pub key_types: BTreeMap<String, String>,
}

/// Reads the contract keys from the text of a `freemapper.toml`. A byte-order mark at the start is
/// skipped. Only text that is no TOML is an error.
pub(crate) fn read_sidecar(toml_text: &str) -> Result<Sidecar, ContractError> {
    let text = toml_text.strip_prefix('\u{feff}').unwrap_or(toml_text);
    let table: toml::Table = text
        .parse()
        .map_err(|e: toml::de::Error| ContractError::Sidecar(e.message().to_string()))?;
    let text_of = |key: &str| table.get(key).and_then(|v| v.as_str()).map(str::to_owned);
    let integer_of = |key: &str| table.get(key).and_then(toml::Value::as_integer);
    Ok(Sidecar {
        map_kind: text_of(KEY_MAP_KIND),
        title: text_of(KEY_TITLE),
        scale_denominator: integer_of(KEY_SCALE),
        crs: integer_of(KEY_CRS),
        key_types: CONTRACT_KEYS
            .iter()
            .filter_map(|key| Some((key.to_string(), table.get(*key)?.type_str().to_string())))
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sidecar_with_bom_and_crlf_parses() {
        let sidecar = read_sidecar("\u{feff}title = \"A\"\r\nmap_kind = \"sprint\"\r\n").unwrap();
        assert_eq!(sidecar.title.as_deref(), Some("A"));
        assert_eq!(sidecar.map_kind.as_deref(), Some("sprint"));
        assert_eq!(sidecar.key_types.len(), 2);
    }

    #[test]
    fn a_key_of_the_wrong_type_is_absent_but_its_type_is_recorded() {
        let sidecar = read_sidecar("title = 3\nscale_denominator = \"1000\"\n").unwrap();
        assert_eq!(sidecar.title, None);
        assert_eq!(sidecar.scale_denominator, None);
        assert_eq!(sidecar.key_types["title"], "integer");
        assert_eq!(sidecar.key_types["scale_denominator"], "string");
    }

    #[test]
    fn other_keys_are_ignored() {
        let sidecar = read_sidecar("buffer_m = 20.0\n[osm]\ncount = 3\n").unwrap();
        assert_eq!(sidecar, Sidecar::default());
    }

    #[test]
    fn text_that_is_no_toml_is_an_error() {
        assert!(matches!(
            read_sidecar("title = "),
            Err(ContractError::Sidecar(_))
        ));
    }
}
