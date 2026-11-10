//! The fixtures: maps and sidecars exactly as freemapper wrote them, each with its expected values and
//! its vocabulary.

use std::path::{Path, PathBuf};

use crate::{Expected, Vocabulary};

/// A map and its sidecar as freemapper wrote them, with the two files that describe them.
#[derive(Debug, Clone, Copy)]
pub struct Fixture {
    /// The fixture's name, e.g. `schoolyard-1000`.
    pub name: &'static str,
    /// The map as OOM XML.
    pub omap_xml: &'static str,
    /// The text of `freemapper.toml`.
    pub sidecar_toml: &'static str,
    /// The text of `expected.toml`; see [`Fixture::expected`].
    pub expected_toml: &'static str,
    /// The text of `vocabulary.toml`; see [`Fixture::vocabulary`].
    pub vocabulary_toml: &'static str,
}

macro_rules! fixture {
    ($name:literal) => {
        Fixture {
            name: $name,
            omap_xml: include_str!(concat!("../fixtures/v1/", $name, "/map.omap")),
            sidecar_toml: include_str!(concat!("../fixtures/v1/", $name, "/freemapper.toml")),
            expected_toml: include_str!(concat!("../fixtures/v1/", $name, "/expected.toml")),
            vocabulary_toml: include_str!(concat!("../fixtures/v1/", $name, "/vocabulary.toml")),
        }
    };
}

static FIXTURES: [Fixture; 2] = [fixture!("schoolyard-1000"), fixture!("sprint-4000")];

impl Fixture {
    /// What a reader must derive from this fixture.
    ///
    /// # Panics
    /// Never for a fixture of this crate: the crate's tests read every `expected.toml`.
    pub fn expected(&self) -> Expected {
        Expected::from_toml(self.expected_toml).expect("the crate's tests read every expected.toml")
    }

    /// The vocabulary freemapper's output must have.
    ///
    /// # Panics
    /// Never for a fixture of this crate: the crate's tests read every `vocabulary.toml`.
    pub fn vocabulary(&self) -> Vocabulary {
        Vocabulary::from_toml(self.vocabulary_toml)
            .expect("the crate's tests read every vocabulary.toml")
    }

    /// Writes the map as `<name>.omap` and the sidecar as `freemapper.toml` into `dir`, the way a project
    /// folder holds them, and returns the map's path.
    pub fn write_into(&self, dir: &Path) -> std::io::Result<PathBuf> {
        let map = dir.join(format!("{}.omap", self.name));
        std::fs::write(&map, self.omap_xml)?;
        std::fs::write(dir.join("freemapper.toml"), self.sidecar_toml)?;
        Ok(map)
    }
}

/// Every fixture of contract version [`crate::CONTRACT_VERSION`].
pub fn fixtures() -> &'static [Fixture] {
    &FIXTURES
}

/// The fixture called `name`.
pub fn fixture(name: &str) -> Option<&'static Fixture> {
    FIXTURES.iter().find(|f| f.name == name)
}
