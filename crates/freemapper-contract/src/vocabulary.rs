//! The words of a map and its sidecar, without their content.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::class::tag;
use crate::tags::{BASE_PART, ROLE_KEY, SHEET_PART, SOURCE_KEY, TAG_PREFIX};
use crate::{ContractError, Sidecar, read_map};

/// What a reader may meet in a file freemapper wrote: part names, the tags of the two reserved parts,
/// the roles of sheet objects, the form of the CRS text, the symbol set and the contract keys of the
/// sidecar. Two maps of the same freemapper version have the same vocabulary, whatever they show. The
/// tags of map content are left out: that set is open and belongs to freemapper alone.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Vocabulary {
    /// The names of the map's parts.
    pub parts: BTreeSet<String>,
    /// The `freemapper:source` values in the part `peripheral information`.
    pub sheet_part_sources: BTreeSet<String>,
    /// The `freemapper:source` values in the part `base`.
    pub base_part_sources: BTreeSet<String>,
    /// The `freemapper:*` tag keys in those two parts.
    pub reserved_part_tag_keys: BTreeSet<String>,
    /// Every value of `freemapper:sheet`, in any part.
    pub sheet_roles: BTreeSet<String>,
    /// The map's CRS text with every digit replaced by `N`, e.g. `+init=epsg:NNNNN`; empty without one.
    pub crs_form: String,
    /// The name of the symbol set.
    pub symbol_set: String,
    /// The contract keys present in the sidecar, with TOML's name of their type.
    pub sidecar: BTreeMap<String, String>,
}

impl Vocabulary {
    /// The vocabulary of a map (OOM XML) and its sidecar (the text of `freemapper.toml`).
    pub fn of(omap_xml: &str, sidecar_toml: &str) -> Result<Vocabulary, ContractError> {
        let doc = read_map(omap_xml)?;
        let mut vocabulary = Vocabulary {
            crs_form: doc
                .georef
                .crs_spec
                .as_deref()
                .unwrap_or_default()
                .chars()
                .map(|c| if c.is_ascii_digit() { 'N' } else { c })
                .collect(),
            symbol_set: doc.symbol_set.name.clone(),
            sidecar: Sidecar::parse(sidecar_toml)
                .map_err(ContractError::Sidecar)?
                .key_types,
            ..Vocabulary::default()
        };
        for part in &doc.parts {
            vocabulary.parts.insert(part.name.clone());
            let sources = match part.name.as_str() {
                SHEET_PART => Some(&mut vocabulary.sheet_part_sources),
                BASE_PART => Some(&mut vocabulary.base_part_sources),
                _ => None,
            };
            let reserved = sources.is_some();
            if let Some(sources) = sources {
                sources.extend(
                    part.objects
                        .iter()
                        .filter_map(|o| tag(&o.tags, SOURCE_KEY))
                        .map(str::to_owned),
                );
            }
            for object in &part.objects {
                if let Some(role) = tag(&object.tags, ROLE_KEY) {
                    vocabulary.sheet_roles.insert(role.to_owned());
                }
                if reserved {
                    vocabulary.reserved_part_tag_keys.extend(
                        object
                            .tags
                            .iter()
                            .map(|(k, _)| k)
                            .filter(|k| k.starts_with(TAG_PREFIX))
                            .cloned(),
                    );
                }
            }
        }
        Ok(vocabulary)
    }

    /// Reads a `vocabulary.toml`.
    pub fn from_toml(text: &str) -> Result<Vocabulary, ContractError> {
        toml::from_str(text).map_err(|e| ContractError::Vocabulary(e.message().to_string()))
    }

    /// The text of a `vocabulary.toml`.
    pub fn to_toml(&self) -> String {
        toml::to_string(self).expect("a vocabulary holds only strings")
    }

    /// One line per field that differs from `other`, naming the field and the words only one side has.
    /// Empty when the two are equal.
    pub fn differences(&self, other: &Vocabulary) -> Vec<String> {
        let mut lines = Vec::new();
        let mut sets = |field: &str, mine: &BTreeSet<String>, theirs: &BTreeSet<String>| {
            if mine != theirs {
                let only = |a: &BTreeSet<String>, b: &BTreeSet<String>| {
                    a.difference(b).cloned().collect::<Vec<_>>().join(", ")
                };
                lines.push(format!(
                    "{field}: only here [{}], only there [{}]",
                    only(mine, theirs),
                    only(theirs, mine)
                ));
            }
        };
        sets("parts", &self.parts, &other.parts);
        sets(
            "sheet_part_sources",
            &self.sheet_part_sources,
            &other.sheet_part_sources,
        );
        sets(
            "base_part_sources",
            &self.base_part_sources,
            &other.base_part_sources,
        );
        sets(
            "reserved_part_tag_keys",
            &self.reserved_part_tag_keys,
            &other.reserved_part_tag_keys,
        );
        sets("sheet_roles", &self.sheet_roles, &other.sheet_roles);
        let pairs = |m: &BTreeMap<String, String>| -> BTreeSet<String> {
            m.iter().map(|(k, v)| format!("{k}: {v}")).collect()
        };
        sets("sidecar", &pairs(&self.sidecar), &pairs(&other.sidecar));
        let mut texts = |field: &str, mine: &str, theirs: &str| {
            if mine != theirs {
                lines.push(format!("{field}: here {mine:?}, there {theirs:?}"));
            }
        };
        texts("crs_form", &self.crs_form, &other.crs_form);
        texts("symbol_set", &self.symbol_set, &other.symbol_set);
        lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{TestObject, good_map, good_parts, good_sidecar, test_map};

    fn set(words: &[&str]) -> BTreeSet<String> {
        words.iter().map(|w| w.to_string()).collect()
    }

    #[test]
    fn lists_the_words_of_a_map() {
        let v = Vocabulary::of(&good_map(), &good_sidecar()).unwrap();
        assert_eq!(v.parts, set(&["base", "default", "peripheral information"]));
        assert_eq!(v.sheet_part_sources, set(&["sheet"]));
        assert_eq!(v.base_part_sources, set(&["base"]));
        assert_eq!(
            v.reserved_part_tag_keys,
            set(&["freemapper:sheet", "freemapper:source"])
        );
        assert_eq!(v.sheet_roles, set(&["scale-bar"]));
        assert_eq!(v.crs_form, "+init=epsg:NNNNN");
        assert_eq!(v.symbol_set, "Test");
        let sidecar: Vec<(&str, &str)> = v
            .sidecar
            .iter()
            .map(|(k, t)| (k.as_str(), t.as_str()))
            .collect();
        assert_eq!(
            sidecar,
            [
                ("crs", "integer"),
                ("map_kind", "string"),
                ("scale_denominator", "integer"),
                ("title", "string"),
            ]
        );
    }

    #[test]
    fn the_old_scale_bar_tag_changes_the_vocabulary() {
        let good = Vocabulary::of(&good_map(), &good_sidecar()).unwrap();
        let mut parts = good_parts();
        parts[1].1[1] = TestObject::new(
            &[("freemapper:source", "scale-bar")],
            &[(0, 60_000), (20_000, 60_000)],
        );
        let old =
            Vocabulary::of(&test_map(1000, "+init=epsg:25833", &parts), &good_sidecar()).unwrap();
        let differences = old.differences(&good);
        assert!(
            differences
                .iter()
                .any(|d| d.starts_with("sheet_part_sources") && d.contains("scale-bar")),
            "{differences:?}"
        );
        assert!(good.differences(&good).is_empty());
    }

    #[test]
    fn content_tags_do_not_count() {
        let good = Vocabulary::of(&good_map(), &good_sidecar()).unwrap();
        let mut parts = good_parts();
        parts[0].1.push(TestObject::new(
            &[("freemapper:source", "alkis"), ("freemapper:alkis", "y")],
            &[(1, 1)],
        ));
        let more =
            Vocabulary::of(&test_map(1000, "+init=epsg:25833", &parts), &good_sidecar()).unwrap();
        assert_eq!(more, good);
    }

    #[test]
    fn works_without_the_sheet_part_and_without_contract_keys() {
        let map = test_map(
            1000,
            "",
            &[("default", vec![TestObject::new(&[], &[(0, 0)])])],
        );
        let v = Vocabulary::of(&map, "buffer_m = 20.0\n").unwrap();
        assert_eq!(v.parts, set(&["default"]));
        assert!(v.sheet_part_sources.is_empty());
        assert!(v.base_part_sources.is_empty());
        assert!(v.reserved_part_tag_keys.is_empty());
        assert!(v.sidecar.is_empty());
        assert_eq!(v.crs_form, "");
    }

    #[test]
    fn round_trips_through_toml() {
        let v = Vocabulary::of(&good_map(), &good_sidecar()).unwrap();
        assert_eq!(Vocabulary::from_toml(&v.to_toml()).unwrap(), v);
        let empty = Vocabulary::default();
        assert_eq!(Vocabulary::from_toml(&empty.to_toml()).unwrap(), empty);
    }
}
