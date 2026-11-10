//! The rules a map and sidecar written by freemapper must keep.

use crate::class::{
    BASE_PART, LEGACY_SOURCE_SCALE_BAR, ObjectClass, ROLE_KEY, SHEET_PART, SOURCE_KEY, classify,
    tag,
};
use crate::sidecar::{KEY_CRS, KEY_MAP_KIND, KEY_SCALE, KEY_TITLE, read_sidecar};
use crate::{ContractError, read_map};

/// A rule of the contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rule {
    /// `map_kind` is `sprint` or `schoolyard`.
    KindKnown,
    /// `title` is a string with a character that is no blank.
    TitlePresent,
    /// `scale_denominator` is one of the scales of the kind (not judged when the kind is unknown).
    ScaleAllowed,
    /// `scale_denominator` equals the scale of the map.
    ScaleMatchesMap,
    /// `crs` is 25832 or 25833.
    CrsKnown,
    /// The CRS text of the map is `+init=epsg:<crs>`.
    CrsText,
    /// Every object in the part `peripheral information` is sheet objects.
    SheetPartOnlySheetObjects,
    /// No sheet objects lies in another part.
    SheetObjectsOnlyInSheetPart,
    /// Every object in the part `base` is base area.
    BasePartOnlyBase,
    /// No base area lies in another part.
    BaseOnlyInBasePart,
    /// No object carries the source `scale-bar` of freemapper v0.9.0.
    NoLegacySource,
    /// `freemapper:sheet` occurs only on sheet objects.
    RoleOnlyOnSheetObjects,
}

/// A rule that does not hold, with the part and object or the key and value it failed on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    pub rule: Rule,
    pub detail: String,
}

const CRS_CODES: [i64; 2] = [25832, 25833];

/// The map scales freemapper writes for a kind; `None` for a kind it does not write.
fn allowed_scales(kind: &str) -> Option<&'static [u32]> {
    match kind {
        "schoolyard" => Some(&[1000]),
        "sprint" => Some(&[4000, 5000]),
        _ => None,
    }
}

/// Checks a map (OOM XML) and its sidecar (the text of `freemapper.toml`) against the rules. A map or
/// sidecar that cannot be read at all is an error; everything else comes back as violations.
pub fn check(omap_xml: &str, sidecar_toml: &str) -> Result<Vec<Violation>, ContractError> {
    let sidecar = read_sidecar(sidecar_toml)?;
    let doc = read_map(omap_xml)?;
    let mut violations = Vec::new();
    let mut report = |rule: Rule, detail: String| violations.push(Violation { rule, detail });
    let shown =
        |value: Option<String>| value.unwrap_or_else(|| "missing or of another type".into());

    let scales = sidecar.map_kind.as_deref().and_then(allowed_scales);
    if scales.is_none() {
        report(
            Rule::KindKnown,
            format!("{KEY_MAP_KIND}: {}", shown(sidecar.map_kind.clone())),
        );
    }
    if !sidecar
        .title
        .as_deref()
        .is_some_and(|t| !t.trim().is_empty())
    {
        report(
            Rule::TitlePresent,
            format!(
                "{KEY_TITLE}: {}",
                shown(sidecar.title.as_ref().map(|t| format!("{t:?}")))
            ),
        );
    }
    let scale = sidecar.scale_denominator;
    if let (Some(scales), Some(scale)) = (scales, scale)
        && !scales.iter().any(|s| i64::from(*s) == scale)
    {
        report(Rule::ScaleAllowed, format!("{KEY_SCALE}: {scale}"));
    }
    let map_scale = doc.georef.scale_denominator;
    if scale != Some(i64::from(map_scale)) {
        report(
            Rule::ScaleMatchesMap,
            format!(
                "{KEY_SCALE}: {}, the map has {map_scale}",
                shown(scale.map(|s| s.to_string()))
            ),
        );
    }
    if !sidecar.crs.is_some_and(|c| CRS_CODES.contains(&c)) {
        report(
            Rule::CrsKnown,
            format!("{KEY_CRS}: {}", shown(sidecar.crs.map(|c| c.to_string()))),
        );
    }
    let crs_text = doc.georef.crs_spec.as_deref().unwrap_or_default();
    if sidecar.crs.map(|c| format!("+init=epsg:{c}")).as_deref() != Some(crs_text) {
        report(
            Rule::CrsText,
            format!(
                "the map has {crs_text:?}, {KEY_CRS}: {}",
                shown(sidecar.crs.map(|c| c.to_string()))
            ),
        );
    }

    for part in &doc.parts {
        for (index, object) in part.objects.iter().enumerate() {
            let class = classify(&object.tags);
            let mut at = |rule: Rule| report(rule, format!("part {:?}, object {index}", part.name));
            if part.name == SHEET_PART && class != ObjectClass::Sheet {
                at(Rule::SheetPartOnlySheetObjects);
            }
            if part.name != SHEET_PART && class == ObjectClass::Sheet {
                at(Rule::SheetObjectsOnlyInSheetPart);
            }
            if part.name == BASE_PART && class != ObjectClass::Base {
                at(Rule::BasePartOnlyBase);
            }
            if part.name != BASE_PART && class == ObjectClass::Base {
                at(Rule::BaseOnlyInBasePart);
            }
            if tag(&object.tags, SOURCE_KEY) == Some(LEGACY_SOURCE_SCALE_BAR) {
                at(Rule::NoLegacySource);
            }
            if tag(&object.tags, ROLE_KEY).is_some() && class != ObjectClass::Sheet {
                at(Rule::RoleOnlyOnSheetObjects);
            }
        }
    }
    Ok(violations)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{TestObject, good_map, good_parts, good_sidecar, test_map};

    const CRS: &str = "+init=epsg:25833";

    fn rules(map: &str, sidecar: &str) -> Vec<Rule> {
        check(map, sidecar)
            .unwrap()
            .iter()
            .map(|v| v.rule)
            .collect()
    }

    /// The sidecar of the good map with one line replaced.
    fn sidecar_with(key: &str, line: &str) -> String {
        good_sidecar()
            .lines()
            .map(|l| if l.starts_with(key) { line } else { l })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// The good map with `change` applied to its parts.
    fn map_with(change: impl FnOnce(&mut Vec<(&'static str, Vec<TestObject>)>)) -> String {
        let mut parts = good_parts();
        change(&mut parts);
        test_map(1000, CRS, &parts)
    }

    #[test]
    fn a_map_as_freemapper_writes_it_has_no_violation() {
        assert_eq!(check(&good_map(), &good_sidecar()).unwrap(), []);
    }

    #[test]
    fn an_unknown_kind_is_reported_and_its_scale_is_not_judged() {
        let sidecar = sidecar_with("map_kind", "map_kind = \"gym\"");
        let violations = check(&good_map(), &sidecar).unwrap();
        assert_eq!(violations.len(), 1, "{violations:?}");
        assert_eq!(violations[0].rule, Rule::KindKnown);
        assert!(violations[0].detail.contains("gym"), "{violations:?}");
    }

    #[test]
    fn a_title_of_blanks_is_no_title() {
        let sidecar = sidecar_with("title", "title = \"  \"");
        assert_eq!(rules(&good_map(), &sidecar), [Rule::TitlePresent]);
    }

    #[test]
    fn a_scale_outside_the_table_is_reported() {
        let map = test_map(2000, CRS, &good_parts());
        let sidecar = sidecar_with("scale_denominator", "scale_denominator = 2000");
        assert_eq!(rules(&map, &sidecar), [Rule::ScaleAllowed]);
    }

    #[test]
    fn sprint_at_3000_is_not_allowed() {
        let map = test_map(3000, CRS, &good_parts());
        let sidecar = sidecar_with("map_kind", "map_kind = \"sprint\"")
            .replace("scale_denominator = 1000", "scale_denominator = 3000");
        assert_eq!(rules(&map, &sidecar), [Rule::ScaleAllowed]);
    }

    #[test]
    fn sprint_at_5000_is_allowed() {
        let map = test_map(5000, CRS, &good_parts());
        let sidecar = sidecar_with("map_kind", "map_kind = \"sprint\"")
            .replace("scale_denominator = 1000", "scale_denominator = 5000");
        assert_eq!(rules(&map, &sidecar), []);
    }

    #[test]
    fn a_scale_other_than_the_maps_is_reported() {
        let map = test_map(4000, CRS, &good_parts());
        assert_eq!(rules(&map, &good_sidecar()), [Rule::ScaleMatchesMap]);
    }

    #[test]
    fn a_crs_outside_the_two_zones_is_reported() {
        let map = test_map(1000, "+init=epsg:4326", &good_parts());
        let sidecar = sidecar_with("crs", "crs = 4326");
        assert_eq!(rules(&map, &sidecar), [Rule::CrsKnown]);
    }

    #[test]
    fn a_crs_text_of_another_form_is_reported() {
        let map = test_map(1000, "EPSG:25833", &good_parts());
        assert_eq!(rules(&map, &good_sidecar()), [Rule::CrsText]);
    }

    #[test]
    fn an_untagged_object_in_the_sheet_part_is_reported() {
        let map = map_with(|parts| parts[1].1.push(TestObject::new(&[], &[(5, 5)])));
        let violations = check(&map, &good_sidecar()).unwrap();
        assert_eq!(violations.len(), 1, "{violations:?}");
        assert_eq!(violations[0].rule, Rule::SheetPartOnlySheetObjects);
        assert_eq!(
            violations[0].detail,
            "part \"peripheral information\", object 2"
        );
    }

    #[test]
    fn sheet_objects_in_another_part_is_reported() {
        let map = map_with(|parts| {
            parts[0].1.push(TestObject::new(
                &[("freemapper:source", "sheet")],
                &[(5, 5)],
            ))
        });
        assert_eq!(
            rules(&map, &good_sidecar()),
            [Rule::SheetObjectsOnlyInSheetPart]
        );
    }

    #[test]
    fn map_content_in_the_base_part_is_reported() {
        let map = map_with(|parts| {
            parts[2]
                .1
                .push(TestObject::new(&[("freemapper:source", "osm")], &[(5, 5)]))
        });
        assert_eq!(rules(&map, &good_sidecar()), [Rule::BasePartOnlyBase]);
    }

    #[test]
    fn base_area_in_another_part_is_reported() {
        let map = map_with(|parts| {
            parts[0]
                .1
                .push(TestObject::new(&[("freemapper:source", "base")], &[(5, 5)]))
        });
        assert_eq!(rules(&map, &good_sidecar()), [Rule::BaseOnlyInBasePart]);
    }

    #[test]
    fn the_scale_bar_tag_of_0_9_0_is_reported() {
        let map = map_with(|parts| {
            parts[1].1[1] = TestObject::new(
                &[("freemapper:source", "scale-bar")],
                &[(0, 60_000), (20_000, 60_000)],
            )
        });
        assert_eq!(rules(&map, &good_sidecar()), [Rule::NoLegacySource]);
    }

    #[test]
    fn a_role_on_map_content_is_reported() {
        let map = map_with(|parts| {
            parts[0].1.push(TestObject::new(
                &[
                    ("freemapper:source", "osm"),
                    ("freemapper:sheet", "scale-bar"),
                ],
                &[(5, 5)],
            ))
        });
        assert_eq!(rules(&map, &good_sidecar()), [Rule::RoleOnlyOnSheetObjects]);
    }

    #[test]
    fn a_map_without_the_reserved_parts_breaks_no_rule() {
        let map = map_with(|parts| parts.truncate(1));
        assert_eq!(rules(&map, &good_sidecar()), []);
    }

    #[test]
    fn a_sidecar_without_contract_keys_gives_violations_not_an_error() {
        assert_eq!(
            rules(&good_map(), "buffer_m = 20.0\n"),
            [
                Rule::KindKnown,
                Rule::TitlePresent,
                Rule::ScaleMatchesMap,
                Rule::CrsKnown,
                Rule::CrsText,
            ]
        );
    }

    #[test]
    fn a_sidecar_with_bom_and_crlf_is_read() {
        let sidecar = format!("\u{feff}{}", good_sidecar().replace('\n', "\r\n"));
        assert_eq!(rules(&good_map(), &sidecar), []);
    }

    #[test]
    fn what_cannot_be_read_is_an_error() {
        assert!(matches!(
            check("not xml", &good_sidecar()),
            Err(ContractError::Map(_))
        ));
        assert!(matches!(
            check(&good_map(), "title = "),
            Err(ContractError::Sidecar(_))
        ));
    }
}
