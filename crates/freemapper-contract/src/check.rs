// SPDX-License-Identifier: AGPL-3.0-or-later

//! The rules a map and sidecar written by freemapper must keep, and those of the template map of the
//! paper frames.

use freemapper_omap::coord::FLAG_CLOSE;
use freemapper_omap::{MapObject, MapObjectKind};

use crate::class::{ObjectClass, tag};
use crate::tags::{
    BASE_PART, KEY_CRS, KEY_MAP_KIND, KEY_SCALE, KEY_TITLE, LEGACY_SOURCE_SCALE_BAR,
    ORIENTATION_KEY, ORIENTATIONS, PAPER_KEY, PAPERS, ROLE_KEY, SHEET_PART, SOURCE_KEY,
    SOURCE_PAPER_FRAME,
};
use crate::{ContractError, Sidecar, read_map};

/// A rule of the contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rule {
    /// `map_kind` is a kind freemapper writes: `sprint` or `schoolyard`.
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
    /// Every object in the part `peripheral information` is a sheet object.
    SheetPartOnlySheetObjects,
    /// No sheet object lies in another part.
    SheetObjectsOnlyInSheetPart,
    /// Every object in the part `base` is base area.
    BasePartOnlyBase,
    /// No base area lies in another part.
    BaseOnlyInBasePart,
    /// No object carries the source `scale-bar` of freemapper v0.9.0.
    NoLegacySource,
    /// `freemapper:sheet` occurs only on sheet objects.
    RoleOnlyOnSheetObjects,
    /// No object of the map carries the source `paper-frame`: the frames live in their template map, so
    /// that map can never pass for the map itself.
    NoPaperFrameInMap,
    /// Every object of the paper frame template carries the source `paper-frame` ([`check_paper_frames`]).
    PaperFrameFileOnlyFrames,
    /// A paper frame carries `freemapper:paper` and `freemapper:orientation` with a reserved value.
    PaperFrameTags,
    /// A paper frame is a closed line: a path whose last point closes it on its first.
    PaperFrameClosedLine,
    /// The paper frame template has the georeferencing of the map, so its coordinates are the map's.
    PaperFrameGeorefOfMap,
}

/// A rule that does not hold, with the part and object or the key and value it failed on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    pub rule: Rule,
    pub detail: String,
}

const CRS_CODES: [u32; 2] = [25832, 25833];

/// Checks a map (OOM XML) and its sidecar (the text of `freemapper.toml`) against the rules. A map or
/// sidecar that cannot be read at all is an error; everything else comes back as violations.
pub fn check(omap_xml: &str, sidecar_toml: &str) -> Result<Vec<Violation>, ContractError> {
    let sidecar = Sidecar::parse(sidecar_toml).map_err(ContractError::Sidecar)?;
    let doc = read_map(omap_xml)?;
    let mut violations = Vec::new();
    let mut report = |rule: Rule, detail: String| violations.push(Violation { rule, detail });
    let shown =
        |value: Option<String>| value.unwrap_or_else(|| "missing or of another type".into());

    let scales = sidecar
        .map_kind
        .known()
        .map(|kind| kind.drawing_scales())
        .filter(|scales| !scales.is_empty());
    if scales.is_none() {
        report(
            Rule::KindKnown,
            format!(
                "{KEY_MAP_KIND}: {}",
                shown(sidecar.map_kind.name().map(str::to_owned))
            ),
        );
    }
    if sidecar.title().is_none() {
        report(
            Rule::TitlePresent,
            format!(
                "{KEY_TITLE}: {}",
                shown(sidecar.raw_title.as_ref().map(|t| format!("{t:?}")))
            ),
        );
    }
    let scale = sidecar.scale_denominator;
    if let (Some(scales), Some(scale)) = (scales, scale)
        && !scales.contains(&scale)
    {
        report(Rule::ScaleAllowed, format!("{KEY_SCALE}: {scale}"));
    }
    let map_scale = doc.georef.scale_denominator;
    if scale != Some(map_scale) {
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
            let class = ObjectClass::of(&object.tags);
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
            if tag(&object.tags, SOURCE_KEY) == Some(SOURCE_PAPER_FRAME) {
                at(Rule::NoPaperFrameInMap);
            }
        }
    }
    Ok(violations)
}

/// Checks the template map of the paper frames (OOM XML of `templates/paper-frame.omap`) against the
/// rules of the frames; `omap_xml` is the map it belongs to. A file that cannot be read at all is an
/// error; everything else comes back as violations.
pub fn check_paper_frames(
    frames_xml: &str,
    omap_xml: &str,
) -> Result<Vec<Violation>, ContractError> {
    let frames = read_map(frames_xml)?;
    let map = read_map(omap_xml)?;
    let mut violations = Vec::new();
    let mut report = |rule: Rule, detail: String| violations.push(Violation { rule, detail });
    if frames.georef != map.georef {
        let shown = |doc: &freemapper_omap::MapDocument| {
            format!(
                "1:{} in {:?}",
                doc.georef.scale_denominator,
                doc.georef.crs_spec.as_deref().unwrap_or_default()
            )
        };
        report(
            Rule::PaperFrameGeorefOfMap,
            format!(
                "the template has {}, the map {}, or another reference point or grivation",
                shown(&frames),
                shown(&map)
            ),
        );
    }
    for part in &frames.parts {
        for (index, object) in part.objects.iter().enumerate() {
            let at = format!("part {:?}, object {index}", part.name);
            if tag(&object.tags, SOURCE_KEY) != Some(SOURCE_PAPER_FRAME) {
                report(Rule::PaperFrameFileOnlyFrames, at);
                continue;
            }
            let paper = tag(&object.tags, PAPER_KEY);
            let orientation = tag(&object.tags, ORIENTATION_KEY);
            if !paper.is_some_and(|p| PAPERS.contains(&p))
                || !orientation.is_some_and(|o| ORIENTATIONS.contains(&o))
            {
                let shown = |value: Option<&str>| value.unwrap_or("missing").to_string();
                report(
                    Rule::PaperFrameTags,
                    format!(
                        "{at}: {PAPER_KEY} {}, {ORIENTATION_KEY} {}",
                        shown(paper),
                        shown(orientation)
                    ),
                );
            }
            if !is_closed_line(object) {
                report(Rule::PaperFrameClosedLine, at);
            }
        }
    }
    Ok(violations)
}

/// Whether `object` is a path of at least three corners whose last point carries the close flag and lies
/// on its first.
fn is_closed_line(object: &MapObject) -> bool {
    let (Some(first), Some(last)) = (object.coords.first(), object.coords.last()) else {
        return false;
    };
    object.kind == MapObjectKind::Path
        && object.coords.len() >= 4
        && last.flags & FLAG_CLOSE != 0
        && (first.x, first.y) == (last.x, last.y)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{
        TestObject, frames_map, good_frame, good_map, good_parts, good_sidecar, test_map,
    };

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
    fn a_paper_frame_in_the_map_is_reported() {
        let map = map_with(|parts| parts[0].1.push(good_frame()));
        assert_eq!(rules(&map, &good_sidecar()), [Rule::NoPaperFrameInMap]);
    }

    #[test]
    fn the_frame_template_does_not_pass_for_the_map() {
        let frames = frames_map(vec![good_frame(), good_frame()]);
        assert_eq!(
            rules(&frames, &good_sidecar()),
            [Rule::NoPaperFrameInMap, Rule::NoPaperFrameInMap]
        );
    }

    fn frame_rules(frames: &str, map: &str) -> Vec<Rule> {
        check_paper_frames(frames, map)
            .unwrap()
            .iter()
            .map(|v| v.rule)
            .collect()
    }

    #[test]
    fn frames_as_freemapper_writes_them_have_no_violation() {
        let frames = frames_map(vec![good_frame(), good_frame()]);
        assert_eq!(check_paper_frames(&frames, &good_map()).unwrap(), []);
        // A project whose frames all lie beyond the map keeps the part and no object.
        assert_eq!(frame_rules(&frames_map(Vec::new()), &good_map()), []);
    }

    #[test]
    fn every_paper_and_orientation_is_allowed_and_no_other() {
        let with = |paper: &'static str, orientation: &'static str| {
            let mut frame = good_frame();
            frame.tags = vec![
                ("freemapper:source", "paper-frame"),
                ("freemapper:paper", paper),
                ("freemapper:orientation", orientation),
            ];
            frame_rules(&frames_map(vec![frame]), &good_map())
        };
        for paper in crate::tags::PAPERS {
            for orientation in crate::tags::ORIENTATIONS {
                assert_eq!(with(paper, orientation), [], "{paper} {orientation}");
            }
        }
        assert_eq!(with("A1", "portrait"), [Rule::PaperFrameTags]);
        assert_eq!(with("a4", "portrait"), [Rule::PaperFrameTags]);
        assert_eq!(with("A4", "Hochformat"), [Rule::PaperFrameTags]);
    }

    #[test]
    fn a_frame_without_its_tags_is_reported_with_what_it_lacks() {
        let mut frame = good_frame();
        frame.tags = vec![("freemapper:source", "paper-frame")];
        let violations = check_paper_frames(&frames_map(vec![frame]), &good_map()).unwrap();
        assert_eq!(violations.len(), 1, "{violations:?}");
        assert_eq!(violations[0].rule, Rule::PaperFrameTags);
        assert_eq!(
            violations[0].detail,
            "part \"paper frames\", object 0: freemapper:paper missing, freemapper:orientation missing"
        );
    }

    #[test]
    fn an_object_that_is_no_frame_is_reported_in_the_template() {
        let other = TestObject::closed(&[("freemapper:source", "osm")], &[(0, 0), (1, 0), (1, 1)]);
        let untagged = TestObject::new(&[], &[(0, 0)]);
        assert_eq!(
            frame_rules(
                &frames_map(vec![good_frame(), other, untagged]),
                &good_map()
            ),
            [
                Rule::PaperFrameFileOnlyFrames,
                Rule::PaperFrameFileOnlyFrames
            ]
        );
    }

    #[test]
    fn a_frame_that_is_not_closed_is_reported() {
        let mut open = good_frame();
        open.closed = false;
        // Closed on its first point, but a line there and back.
        let mut short = good_frame();
        short.coords = vec![short.coords[0], short.coords[1], short.coords[0]];
        let mut gap = good_frame();
        gap.coords.last_mut().unwrap().0 += 1;
        // Three corners and the first again are the fewest points of a closed line.
        let mut triangle = good_frame();
        triangle.coords.remove(3);
        assert_eq!(triangle.coords.len(), 4);
        assert_eq!(frame_rules(&frames_map(vec![triangle]), &good_map()), []);
        for frame in [open, short, gap] {
            assert_eq!(
                frame_rules(&frames_map(vec![frame]), &good_map()),
                [Rule::PaperFrameClosedLine]
            );
        }
    }

    #[test]
    fn a_template_of_another_georeferencing_is_reported() {
        let other_scale = test_map(4000, CRS, &[("paper frames", vec![good_frame()])]);
        let violations = check_paper_frames(&other_scale, &good_map()).unwrap();
        assert_eq!(violations.len(), 1, "{violations:?}");
        assert_eq!(violations[0].rule, Rule::PaperFrameGeorefOfMap);
        assert!(violations[0].detail.contains("1:4000"), "{violations:?}");
        let other_crs = test_map(
            1000,
            "+init=epsg:25832",
            &[("paper frames", vec![good_frame()])],
        );
        assert_eq!(
            frame_rules(&other_crs, &good_map()),
            [Rule::PaperFrameGeorefOfMap]
        );
    }

    #[test]
    fn a_frame_template_that_cannot_be_read_is_an_error() {
        assert!(matches!(
            check_paper_frames("not xml", &good_map()),
            Err(ContractError::Map(_))
        ));
        assert!(matches!(
            check_paper_frames(&frames_map(vec![good_frame()]), "not xml"),
            Err(ContractError::Map(_))
        ));
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
