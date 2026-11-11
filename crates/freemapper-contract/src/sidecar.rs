//! The sidecar `freemapper.toml`: its contract keys, read once for the suite. Every other key is
//! freemapper's own record and is ignored.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::MapKind;
use crate::tags::{CONTRACT_KEYS, KEY_CRS, KEY_MAP_KIND, KEY_SCALE, KEY_TITLE, SIDECAR_FILE};

/// What `map_kind` in a sidecar says.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum SidecarKind {
    /// No `map_kind`, or one that is no string.
    #[default]
    Absent,
    /// A kind this version knows.
    Known(MapKind),
    /// A name this version does not know; a newer freemapper may write one. A reader takes no kind from
    /// the sidecar then and reports no error.
    Unknown(String),
}

impl SidecarKind {
    /// The kind, if the sidecar names one this version knows.
    pub fn known(&self) -> Option<MapKind> {
        match self {
            SidecarKind::Known(kind) => Some(*kind),
            SidecarKind::Absent | SidecarKind::Unknown(_) => None,
        }
    }

    /// The name as the sidecar writes it, known or not.
    pub fn name(&self) -> Option<&str> {
        match self {
            SidecarKind::Absent => None,
            SidecarKind::Known(kind) => Some(kind.name()),
            SidecarKind::Unknown(name) => Some(name),
        }
    }
}

/// The contract keys of a sidecar. A key that is missing or has the wrong type leaves its field empty;
/// [`Sidecar::key_types`] tells the two apart.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Sidecar {
    pub map_kind: SidecarKind,
    /// The title as written. [`Sidecar::title`] gives the usable one.
    pub raw_title: Option<String>,
    /// The map scale (denominator); absent when the value is no integer or does not fit `u32`.
    pub scale_denominator: Option<u32>,
    /// The EPSG code; absent when the value is no integer or does not fit `u32`.
    pub crs: Option<u32>,
    /// The contract keys that are present, with TOML's name of their type (`string`, `integer`, …).
    pub key_types: BTreeMap<String, String>,
}

/// A `freemapper.toml` could not be used.
#[derive(Debug, thiserror::Error)]
pub enum SidecarError {
    /// The operating system's error is the `source`, not part of the message.
    #[error("could not read {path}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("{path} is not valid: {message}")]
    Invalid { path: PathBuf, message: String },
}

impl Sidecar {
    /// Reads the contract keys from the text of a `freemapper.toml`. A byte-order mark at the start is
    /// skipped. Only text that is no TOML is an error; the error is the parser's message.
    pub fn parse(toml_text: &str) -> Result<Sidecar, String> {
        let text = toml_text.strip_prefix('\u{feff}').unwrap_or(toml_text);
        let table: toml::Table = text
            .parse()
            .map_err(|e: toml::de::Error| e.message().to_string())?;
        let text_of = |key: &str| table.get(key).and_then(|v| v.as_str()).map(str::to_owned);
        let number_of = |key: &str| {
            table
                .get(key)
                .and_then(toml::Value::as_integer)
                .and_then(|n| u32::try_from(n).ok())
        };
        Ok(Sidecar {
            map_kind: match text_of(KEY_MAP_KIND) {
                None => SidecarKind::Absent,
                Some(name) => match MapKind::from_name(&name) {
                    Some(kind) => SidecarKind::Known(kind),
                    None => SidecarKind::Unknown(name),
                },
            },
            raw_title: text_of(KEY_TITLE),
            scale_denominator: number_of(KEY_SCALE),
            crs: number_of(KEY_CRS),
            key_types: CONTRACT_KEYS
                .iter()
                .filter_map(|key| Some((key.to_string(), table.get(*key)?.type_str().to_string())))
                .collect(),
        })
    }

    /// The sidecar of the map in `map_path`: the `freemapper.toml` in the same folder. `Ok(None)` without
    /// the file, an error when the file exists but cannot be read or is no TOML.
    pub fn next_to(map_path: &Path) -> Result<Option<Sidecar>, SidecarError> {
        let Some(path) = sidecar_path(map_path) else {
            return Ok(None);
        };
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(source) => return Err(SidecarError::Read { path, source }),
        };
        Sidecar::parse(&text)
            .map(Some)
            .map_err(|message| SidecarError::Invalid { path, message })
    }

    /// The title without blanks around it; `None` without a title or with one of blanks only.
    pub fn title(&self) -> Option<&str> {
        self.raw_title
            .as_deref()
            .map(str::trim)
            .filter(|title| !title.is_empty())
    }
}

/// Where the sidecar of the map in `map_path` lies: `freemapper.toml` in the same folder. `None` for a
/// path without a folder part, such as `/`.
pub fn sidecar_path(map_path: &Path) -> Option<PathBuf> {
    Some(map_path.parent()?.join(SIDECAR_FILE))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sidecar_with_bom_and_crlf_parses() {
        let sidecar = Sidecar::parse("\u{feff}title = \"A\"\r\nmap_kind = \"sprint\"\r\n").unwrap();
        assert_eq!(sidecar.title(), Some("A"));
        assert_eq!(sidecar.map_kind, SidecarKind::Known(MapKind::Sprint));
        assert_eq!(sidecar.key_types.len(), 2);
    }

    #[test]
    fn a_key_of_the_wrong_type_is_absent_but_its_type_is_recorded() {
        let sidecar =
            Sidecar::parse("title = 3\nscale_denominator = \"1000\"\nmap_kind = 4000\n").unwrap();
        assert_eq!(sidecar.raw_title, None);
        assert_eq!(sidecar.scale_denominator, None);
        assert_eq!(sidecar.map_kind, SidecarKind::Absent);
        assert_eq!(sidecar.key_types["title"], "integer");
        assert_eq!(sidecar.key_types["scale_denominator"], "string");
        assert_eq!(sidecar.key_types["map_kind"], "integer");
    }

    #[test]
    fn a_number_that_does_not_fit_is_absent() {
        let sidecar = Sidecar::parse("scale_denominator = -4000\ncrs = 5000000000\n").unwrap();
        assert_eq!(sidecar.scale_denominator, None);
        assert_eq!(sidecar.crs, None);
        assert_eq!(sidecar.key_types["crs"], "integer");
    }

    #[test]
    fn other_keys_are_ignored() {
        let sidecar = Sidecar::parse("buffer_m = 20.0\n[osm]\ncount = 3\n").unwrap();
        assert_eq!(sidecar, Sidecar::default());
    }

    #[test]
    fn text_that_is_no_toml_is_an_error() {
        assert!(Sidecar::parse("title = ").is_err());
    }

    #[test]
    fn an_unknown_kind_keeps_its_name_and_the_title() {
        let sidecar = Sidecar::parse("map_kind = \"gym\"\ntitle = \"Halle\"\n").unwrap();
        assert_eq!(sidecar.map_kind, SidecarKind::Unknown("gym".into()));
        assert_eq!(sidecar.map_kind.known(), None);
        assert_eq!(sidecar.map_kind.name(), Some("gym"));
        assert_eq!(sidecar.title(), Some("Halle"));
        // The name is exact: a capital letter makes another name.
        let capital = Sidecar::parse("map_kind = \"Sprint\"\n").unwrap();
        assert_eq!(capital.map_kind, SidecarKind::Unknown("Sprint".into()));
    }

    #[test]
    fn a_title_of_blanks_is_no_title() {
        let sidecar = Sidecar::parse("title = \"  \"\n").unwrap();
        assert_eq!(sidecar.raw_title.as_deref(), Some("  "));
        assert_eq!(sidecar.title(), None);
        assert_eq!(
            Sidecar::parse("title = \" A b \"\n").unwrap().title(),
            Some("A b")
        );
    }

    #[test]
    fn the_sidecar_lies_next_to_the_map() {
        let dir = tempfile::tempdir().unwrap();
        let map = dir.path().join("x.omap");
        assert_eq!(sidecar_path(&map), Some(dir.path().join("freemapper.toml")));
        assert!(Sidecar::next_to(&map).unwrap().is_none());

        std::fs::write(
            dir.path().join("freemapper.toml"),
            "map_kind = \"schoolyard\"\n",
        )
        .unwrap();
        let sidecar = Sidecar::next_to(&map).unwrap().unwrap();
        assert_eq!(sidecar.map_kind.known(), Some(MapKind::Schoolyard));

        std::fs::write(dir.path().join("freemapper.toml"), "map_kind = ").unwrap();
        let error = Sidecar::next_to(&map).unwrap_err();
        assert!(matches!(error, SidecarError::Invalid { .. }), "{error}");
        assert!(error.to_string().contains("freemapper.toml"), "{error}");
    }

    #[test]
    fn a_sidecar_that_is_a_folder_is_a_read_error() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("freemapper.toml")).unwrap();
        let error = Sidecar::next_to(&dir.path().join("x.omap")).unwrap_err();
        assert!(matches!(error, SidecarError::Read { .. }), "{error}");
    }
}
