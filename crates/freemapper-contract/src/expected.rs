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
    pub map_kind: String,
    pub title: String,
    pub scale_denominator: u32,
    /// EPSG code.
    pub crs: u32,
    pub objects: ClassCounts,
    #[serde(default)]
    pub bbox: ClassBoxes,
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
            "{HEAD}[objects]\nfurniture = 13\nbase = 1\nnorth_lines = 3\ncontent = 2\n\
             [bbox]\ncontent = [-30000, -43091, 30000, 42276]\n"
        );
        let expected = Expected::from_toml(&text).unwrap();
        assert_eq!(expected.contract, 1);
        assert_eq!(expected.map_kind, "sprint");
        assert_eq!(expected.scale_denominator, 4000);
        assert_eq!(expected.objects.furniture, 13);
        assert_eq!(expected.bbox.content, Some([-30000, -43091, 30000, 42276]));
        assert_eq!(expected.bbox.furniture, None);
    }

    #[test]
    fn the_boxes_may_be_left_out() {
        let text =
            format!("{HEAD}[objects]\nfurniture = 0\nbase = 0\nnorth_lines = 0\ncontent = 0\n");
        assert_eq!(
            Expected::from_toml(&text).unwrap().bbox,
            ClassBoxes::default()
        );
    }

    #[test]
    fn a_missing_value_is_an_error() {
        assert!(matches!(
            Expected::from_toml(HEAD),
            Err(ContractError::Expected(_))
        ));
    }
}
