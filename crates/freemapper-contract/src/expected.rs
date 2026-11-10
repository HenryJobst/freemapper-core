//! What a reader must derive from a fixture.

use serde::Deserialize;

use crate::ContractError;
use crate::census::{ClassBoxes, ClassCounts};

/// The `expected.toml` of a fixture: the values a reader must arrive at with its own code. The first
/// four come from the sidecar, the counts and boxes from the map.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Expected {
    /// The contract version the fixture was written for.
    pub contract: u32,
    /// The freemapper version and commit that wrote the fixture.
    pub producer: String,
    /// `sprint` or `schoolyard`, or `unknown` when the sidecar names a kind this contract version does not
    /// know: the reader then takes no kind from the sidecar and reports no error.
    pub map_kind: String,
    /// The `title` of the sidecar; `None` when it has none (files of freemapper v0.9.0), and the reader
    /// falls back to its own default.
    #[serde(default)]
    pub title: Option<String>,
    pub scale_denominator: u32,
    /// EPSG code.
    pub crs: u32,
    pub objects: ClassCounts,
    #[serde(default)]
    pub bbox: ClassBoxes,
    /// The rules of [`crate::check`] the fixture breaks on purpose, by the name of the [`crate::Rule`];
    /// empty for a fixture that shows what freemapper writes today.
    #[serde(default)]
    pub breaks: Vec<String>,
}

impl Expected {
    /// Reads an `expected.toml`.
    pub fn from_toml(text: &str) -> Result<Expected, ContractError> {
        toml::from_str(text).map_err(|e| ContractError::Expected(e.message().to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEAD: &str = "contract = 1\nproducer = \"freemapper 0.9.2 (728fef5)\"\nmap_kind = \"sprint\"\n\
        title = \"A\"\nscale_denominator = 4000\ncrs = 25833\n";

    #[test]
    fn reads_values_counts_and_boxes() {
        let text = format!(
            "{HEAD}[objects]\nsheet = 13\nbase = 1\nnorth_lines = 3\ncontent = 2\n\
             [bbox]\ncontent = [-30000, -43091, 30000, 42276]\n"
        );
        let expected = Expected::from_toml(&text).unwrap();
        assert_eq!(expected.contract, 1);
        assert_eq!(expected.map_kind, "sprint");
        assert_eq!(expected.title.as_deref(), Some("A"));
        assert!(expected.breaks.is_empty());
        assert_eq!(expected.scale_denominator, 4000);
        assert_eq!(expected.objects.sheet, 13);
        assert_eq!(expected.bbox.content, Some([-30000, -43091, 30000, 42276]));
        assert_eq!(expected.bbox.sheet, None);
    }

    #[test]
    fn the_boxes_may_be_left_out() {
        let text = format!("{HEAD}[objects]\nsheet = 0\nbase = 0\nnorth_lines = 0\ncontent = 0\n");
        assert_eq!(
            Expected::from_toml(&text).unwrap().bbox,
            ClassBoxes::default()
        );
    }

    #[test]
    fn the_title_may_be_left_out_and_broken_rules_named() {
        let text = format!(
            "{}breaks = [\"TitlePresent\"]\n[objects]\nsheet = 0\nbase = 0\nnorth_lines = 0\ncontent = 0\n",
            HEAD.replace("title = \"A\"\n", "")
        );
        let expected = Expected::from_toml(&text).unwrap();
        assert_eq!(expected.title, None);
        assert_eq!(expected.breaks, ["TitlePresent"]);
    }

    #[test]
    fn a_missing_value_is_an_error() {
        assert!(matches!(
            Expected::from_toml(HEAD),
            Err(ContractError::Expected(_))
        ));
    }
}
