//! The map kinds of the suite and the scales freemapper draws them at.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// Kartenart: the kind of map, as freemapper's glossary defines it. The one definition for the suite: the
/// name is what `map_kind` in `freemapper.toml`, the command lines and the rule profiles say.
///
/// A name this version does not know is not a kind; see [`crate::SidecarKind::Unknown`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MapKind {
    Sprint,
    Schoolyard,
    Forest,
}

impl MapKind {
    /// Every map kind, in the order rule profiles list them.
    pub const ALL: [MapKind; 3] = [MapKind::Sprint, MapKind::Schoolyard, MapKind::Forest];

    /// The lower-case name, as files and command lines write it.
    pub fn name(self) -> &'static str {
        match self {
            MapKind::Sprint => "sprint",
            MapKind::Schoolyard => "schoolyard",
            MapKind::Forest => "forest",
        }
    }

    /// The kind with this name; the comparison is exact, so `Sprint` is no kind.
    pub fn from_name(name: &str) -> Option<MapKind> {
        MapKind::ALL.into_iter().find(|kind| kind.name() == name)
    }

    /// The scale (denominator) the standard draws this kind of map at: symbols at 100 %.
    pub fn base_scale(self) -> u32 {
        match self {
            MapKind::Sprint => 4000,
            MapKind::Schoolyard => 1000,
            MapKind::Forest => 15000,
        }
    }

    /// The map scales freemapper writes for this kind, ascending; empty for a kind freemapper does not
    /// write. A reader accepts every one of them. Print scales are another matter: a sprint map drawn at
    /// 1:4000 is printed at 1:3000 by enlarging it.
    pub fn drawing_scales(self) -> &'static [u32] {
        match self {
            MapKind::Sprint => &[4000, 5000],
            MapKind::Schoolyard => &[1000],
            MapKind::Forest => &[],
        }
    }

    /// Whether freemapper writes maps of this kind.
    pub fn is_written_by_freemapper(self) -> bool {
        !self.drawing_scales().is_empty()
    }
}

impl fmt::Display for MapKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// The name is no map kind of this version.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown map kind `{0}`")]
pub struct UnknownKind(pub String);

impl FromStr for MapKind {
    type Err = UnknownKind;

    fn from_str(name: &str) -> Result<MapKind, UnknownKind> {
        MapKind::from_name(name).ok_or_else(|| UnknownKind(name.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_round_trip_and_are_exact() {
        for kind in MapKind::ALL {
            assert_eq!(MapKind::from_name(kind.name()), Some(kind));
            assert_eq!(kind.name().parse::<MapKind>(), Ok(kind));
            assert_eq!(kind.to_string(), kind.name());
        }
        assert_eq!(MapKind::from_name("Sprint"), None);
        assert_eq!(MapKind::from_name("gym"), None);
        assert_eq!(
            "gym".parse::<MapKind>().unwrap_err().to_string(),
            "unknown map kind `gym`"
        );
    }

    #[test]
    fn serde_writes_the_name() {
        #[derive(Serialize, Deserialize)]
        struct File {
            map_kind: MapKind,
        }
        for kind in MapKind::ALL {
            let text = toml::to_string(&File { map_kind: kind }).unwrap();
            assert_eq!(text, format!("map_kind = \"{}\"\n", kind.name()));
            assert_eq!(toml::from_str::<File>(&text).unwrap().map_kind, kind);
        }
        assert!(toml::from_str::<File>("map_kind = \"gym\"\n").is_err());
    }

    #[test]
    fn the_base_scale_is_a_drawing_scale_of_every_kind_freemapper_writes() {
        for kind in MapKind::ALL {
            let scales = kind.drawing_scales();
            assert!(scales.is_sorted(), "{kind}");
            assert_eq!(
                kind.is_written_by_freemapper(),
                scales.contains(&kind.base_scale()),
                "{kind}"
            );
        }
        assert!(!MapKind::Forest.is_written_by_freemapper());
    }
}
