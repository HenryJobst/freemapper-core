//! The page builder's tests, moved from freemapper-print's `print.rs` (ADR 0023). Each print scale of the
//! old tests is replaced by the two factors print's scale policy computes for it.

use super::*;
use crate::bounds::shape_bounds;
use crate::draw::FillRule;
use crate::pdf::{PdfMeta, write_pdf};
use freemapper_omap::{MapGeoref, MapPart};

fn map(scale: u32, set: SymbolSet, objects: Vec<MapObject>) -> MapDocument {
    MapDocument {
        notes: String::new(),
        georef: MapGeoref {
            scale_denominator: scale,
            declination_deg: 0.0,
            grivation_deg: 0.0,
            ref_projected: None,
            crs_spec: None,
            combined_scale_factor: 1.0,
            map_ref_mm: (0.0, 0.0),
        },
        symbol_set: set,
        parts: vec![MapPart {
            name: "default part".into(),
            objects,
        }],
        templates: Vec::new(),
        unknown_objects: 0,
    }
}

fn line(symbol_id: u32, to: (i32, i32)) -> MapObject {
    MapObject {
        symbol_id,
        kind: MapObjectKind::Path,
        coords: vec![MapCoord::new(0, 0), MapCoord::new(to.0, to.1)],
        tags: Vec::new(),
        pattern: None,
    }
}

fn edge() -> u32 {
    SymbolSet::issprom_2019().id_for_code("501.1").unwrap()
}

/// The map scale on A4: both factors 1 (print's old options: no print scale, a sprint map).
fn options(orientation: Orientation) -> PageSpec {
    PageSpec {
        orientation,
        ..PageSpec::new(1.0, 1.0, Paper::A4)
    }
}

fn render(doc: &MapDocument, options: &PageSpec) -> (RenderedPage, BuildReport) {
    build_page(doc, options).unwrap()
}

#[test]
fn primitives_carry_symbol_class_north_line_and_object() {
    use crate::draw::SymbolClass;
    let set = SymbolSet::issprom_2019();
    let id = |code: &str| set.id_for_code(code).unwrap();
    let square = |symbol_id: u32| {
        let mut coords: Vec<MapCoord> =
            [(0, 0), (20_000, 0), (20_000, 20_000), (0, 20_000), (0, 0)]
                .into_iter()
                .map(|(x, y)| MapCoord::new(x, y))
                .collect();
        coords[4].flags = freemapper_omap::coord::FLAG_CLOSE;
        MapObject {
            symbol_id,
            kind: MapObjectKind::Path,
            coords,
            tags: Vec::new(),
            pattern: None,
        }
    };
    let boulder = MapObject {
        symbol_id: id("204"),
        kind: MapObjectKind::Point { rotation_rad: 0.0 },
        coords: vec![MapCoord::new(5_000, 5_000)],
        tags: Vec::new(),
        pattern: None,
    };
    let objects = vec![
        boulder,
        line(id("501.1"), (30_000, 0)),
        square(id("401")),
        // Uncrossable water: a combined symbol of a fill and a bank line, closed.
        square(id("301")),
        line(id("601"), (0, 40_000)),
    ];
    // The same combined symbol on an open path.
    let objects = [objects, vec![line(id("301"), (0, -30_000))]].concat();
    let doc = map(4000, set.clone(), objects);
    let (page, _) = render(&doc, &options(Orientation::Portrait));
    let classes = |object: u32| -> Vec<SymbolClass> {
        let mut c: Vec<SymbolClass> = page
            .primitives
            .iter()
            .filter(|p| p.object == object)
            .map(|p| p.class)
            .collect();
        c.dedup();
        c
    };
    assert_eq!(classes(0), [SymbolClass::Point]);
    assert_eq!(classes(1), [SymbolClass::Line]);
    assert_eq!(classes(2), [SymbolClass::Area]);
    // A combined symbol counts by its path, as freemapper-courseplan reads maps: a closed one (the water
    // with its bank line) is an area, an open one a line.
    assert_eq!(classes(3), [SymbolClass::Area]);
    assert_eq!(classes(5), [SymbolClass::Line]);
    assert!(!page.primitives.iter().any(|p| p.object > 5));
    for p in &page.primitives {
        assert_eq!(p.north_line, p.object == 4, "{p:?}");
    }
    assert!(page.primitives.iter().any(|p| p.north_line));
}

#[test]
fn auto_turns_a_wide_map_to_landscape() {
    let doc = map(
        4000,
        SymbolSet::issprom_2019(),
        vec![line(edge(), (250_000, 150_000))],
    );
    let (page, report) = render(&doc, &options(Orientation::Auto));
    assert_eq!(page.page_mm, (297.0, 210.0));
    assert_eq!(report.overflow_mm, None);
    let (_, report) = render(&doc, &options(Orientation::Portrait));
    // 250 mm of line plus half its 0.1 mm width (501.1) at each end, on 200 mm of printable
    // width.
    let (dx, dy) = report.overflow_mm.unwrap();
    assert!((dx - 50.1).abs() < 1e-9 && dy == 0.0, "{dx} {dy}");
}

#[test]
fn auto_turns_a_long_straight_line_too() {
    // No overflowing area in either orientation; the overflow length decides.
    let doc = map(
        4000,
        SymbolSet::issprom_2019(),
        vec![line(edge(), (250_000, 0))],
    );
    let (page, _) = render(&doc, &options(Orientation::Auto));
    assert_eq!(page.page_mm, (297.0, 210.0));
}

#[test]
fn auto_prefers_portrait_on_a_tie() {
    let doc = map(
        4000,
        SymbolSet::issprom_2019(),
        vec![line(edge(), (20_000, 20_000))],
    );
    let (page, _) = render(&doc, &options(Orientation::Auto));
    assert_eq!(page.page_mm, (210.0, 297.0));
    let empty = map(4000, SymbolSet::issprom_2019(), Vec::new());
    let (page, report) = render(&empty, &options(Orientation::Auto));
    assert_eq!(page.page_mm, (210.0, 297.0));
    assert!(page.primitives.is_empty() && report.overflow_mm.is_none());
}

#[test]
fn sheet_tagged_north_line_variants_are_drawn() {
    // A variant 601.x (601.5, the blue north line) is a north line too (ADR 0023); a code
    // that only starts with the digits is not.
    let set = SymbolSet::issprom_2019().with_additions(
        r#"<symbol type="2" id="@" code="6011" name="other"><line_symbol color="2" line_width="100"/></symbol>"#,
    );
    let sheet = |code: &str| MapObject {
        tags: vec![(SHEET_TAG.0.into(), SHEET_TAG.1.into())],
        ..line(set.id_for_code(code).unwrap(), (25_000, 0))
    };
    let doc = map(4000, set.clone(), vec![sheet("601.5")]);
    let (page, _) = render(&doc, &options(Orientation::Portrait));
    assert_eq!(page.primitives.len(), 1);
    let doc = map(4000, set.clone(), vec![sheet("6011")]);
    let (page, _) = render(&doc, &options(Orientation::Portrait));
    assert!(page.primitives.is_empty());
}

#[test]
fn scale_bar_tagged_objects_count_as_sheet_objects() {
    // freemapper v0.9.0 tags the scale bar `scale-bar` instead of `sheet`; it counts as one, so it is left out of the
    // page like the rest of the information block.
    let set = SymbolSet::issprom_2019();
    let code = set.id_for_code("601").unwrap();
    let bar = MapObject {
        tags: vec![("freemapper:source".into(), "scale-bar".into())],
        ..line(code, (25_000, 0))
    };
    let doc = map(4000, set, vec![bar]);
    assert!(super::is_sheet_object(&doc.parts[0].objects[0]));
}

#[test]
fn hidden_symbols_are_skipped() {
    let set = SymbolSet::issprom_2019().with_additions(
        r#"<symbol type="2" id="@" code="999.1" name="hidden" is_hidden="true"><line_symbol color="2" line_width="100"/></symbol>"#,
    );
    let hidden = set.id_for_code("999.1").unwrap();
    assert!(set.is_hidden(hidden));
    let doc = map(4000, set, vec![line(hidden, (25_000, 0))]);
    let (page, report) = render(&doc, &options(Orientation::Portrait));
    assert!(page.primitives.is_empty());
    assert_eq!(report.skipped_objects, 1);
}

#[test]
fn centre_option_puts_a_map_point_in_the_middle() {
    let doc = map(
        4000,
        SymbolSet::issprom_2019(),
        vec![line(edge(), (25_000, 0))],
    );
    // 1:5000 on a 1:4000 sprint map: the geometry shrinks by 0.8, the symbols keep their size.
    let at_origin = PageSpec {
        geometry_factor: 0.8,
        centre: Some(MapCoord::new(0, 0)),
        ..options(Orientation::Portrait)
    };
    let (page, _) = render(&doc, &at_origin);
    let Shape::Stroke { path, .. } = &page.primitives[0].shape else {
        panic!()
    };
    assert!((path[0].0 - 105.0).abs() < 1e-9 && (path[0].1 - 148.5).abs() < 1e-9);
    assert!((path[1].0 - 125.0).abs() < 1e-9, "{path:?}");
}

#[test]
fn text_anchor_scales_and_size_follows_the_symbol() {
    let set = SymbolSet::issprom_2019();
    let id = set.id_for_code("704").unwrap();
    let text = MapObject {
        symbol_id: id,
        kind: MapObjectKind::Text {
            text: "Wald".into(),
            h_align: 0,
            v_align: 0,
            rotation_rad: 0.0,
            box_size: None,
        },
        coords: vec![MapCoord::new(10_000, 0)],
        tags: Vec::new(),
        pattern: None,
    };
    let doc = map(15000, set, vec![text]);
    // A 1:15000 forest map at 1:15000 (factors 1) and at 1:10000 (geometry and symbols 1.5).
    let at = |factor| {
        let options = PageSpec {
            symbol_factor: factor,
            geometry_factor: factor,
            centre: Some(MapCoord::new(0, 0)),
            ..options(Orientation::Portrait)
        };
        let (page, _) = render(&doc, &options);
        match &page.primitives[..] {
            [p] => match &p.shape {
                Shape::Text { origin, size, .. } => (origin.0 - 105.0, *size),
                other => panic!("{other:?}"),
            },
            other => panic!("{other:?}"),
        }
    };
    let (x_base, size_base) = at(1.0);
    let (x_big, size_big) = at(1.5);
    assert!((x_big / x_base - 1.5).abs() < 1e-6, "{x_base} {x_big}");
    assert!(
        (size_big / size_base - 1.5).abs() < 1e-3,
        "{size_base} {size_big}"
    );
}

#[test]
fn box_scales_with_the_geometry() {
    let set = SymbolSet::issprom_2019();
    let text = MapObject {
        symbol_id: set.id_for_code("704").unwrap(),
        kind: MapObjectKind::Text {
            text: "Wald".into(),
            h_align: 0,
            v_align: 1,
            rotation_rad: 0.0,
            box_size: Some((40_000.0, 20_000.0)),
        },
        coords: vec![MapCoord::new(10_000, 0)],
        tags: Vec::new(),
        pattern: None,
    };
    let MapObjectKind::Text { box_size, .. } = scaled(&text, 0.8).kind else {
        panic!()
    };
    assert_eq!(box_size, Some((32_000.0, 16_000.0)));
}

#[test]
fn substituted_fonts_are_reported() {
    let mut set = SymbolSet::issprom_2019();
    set.symbols_xml = set
        .symbols_xml
        .replace(r#"family="Arial""#, r#"family="Frutiger""#);
    let id = set.id_for_code("704").unwrap();
    let text = MapObject {
        symbol_id: id,
        kind: MapObjectKind::Text {
            text: "Wald".into(),
            h_align: 0,
            v_align: 0,
            rotation_rad: 0.0,
            box_size: None,
        },
        coords: vec![MapCoord::new(0, 0)],
        tags: Vec::new(),
        pattern: None,
    };
    let doc = map(4000, set, vec![text.clone(), text]);
    let (_, report) = render(&doc, &options(Orientation::Portrait));
    assert_eq!(report.substituted_fonts, ["Frutiger"]);
}

/// A sprint set plus `999.1`, an area with a hatch `spacing` µm apart.
fn hatched(spacing: u32) -> (SymbolSet, u32) {
    let set = SymbolSet::issprom_2019().with_additions(&format!(
        r#"<symbol type="4" id="@" code="999.1" name="hatch"><area_symbol inner_color="-1" min_area="0" patterns="1"><pattern type="1" angle="0" rotatable="false" line_spacing="{spacing}" line_offset="0" offset_along_line="0" color="2" line_width="10"/></area_symbol></symbol>"#
    ));
    let id = set.id_for_code("999.1").unwrap();
    (set, id)
}

fn area(symbol_id: u32, w: i32, h: i32) -> MapObject {
    MapObject {
        symbol_id,
        kind: MapObjectKind::Path,
        coords: freemapper_omap::closed_ring(&[(0, 0), (w, 0), (w, h), (0, h)]),
        tags: Vec::new(),
        pattern: None,
    }
}

#[test]
fn page_budget_bounds_the_page_and_counts_the_rest() {
    // Each area: a 10 mm square hatched 0.1 mm apart, 101 lines.
    let (set, id) = hatched(100);
    let doc = map(4000, set, vec![area(id, 10_000, 10_000); 20]);
    let (page, report) = build_page_within(&doc, &options(Orientation::Portrait), 250).unwrap();
    // A unit per line and one per outline: two whole areas (102 units each), 45 lines of the
    // third.
    assert_eq!(page.primitives.len(), 247);
    // Two whole areas, the third cut short, the other 17 not drawn.
    assert_eq!(report.truncated_objects, 1);
    assert_eq!(report.objects_not_drawn_page_limit, 17);
    let (page, report) = render(&doc, &options(Orientation::Portrait));
    assert_eq!(page.primitives.len(), 20 * 101);
    assert_eq!(
        (
            report.truncated_objects,
            report.objects_not_drawn_page_limit
        ),
        (0, 0)
    );
}

/// A map of `objects` objects of 20 000 points each, whose combined symbol fills the object
/// 1000 times (final review of M1: three of them took 1.2 GB).
fn thousand_fills(objects: usize) -> MapDocument {
    let parts = r#"<part symbol="0"/>"#.repeat(1000);
    let coords: String = (0..20_000)
        .map(|i| format!("{} {};", i % 1000 * 10, i / 1000 * 10))
        .collect();
    let object = format!(
        r#"<object type="1" symbol="1"><coords count="20001">{coords}0 0 18;</coords></object>"#
    );
    let xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<map xmlns="http://openorienteering.org/apps/mapper/xml/v2" version="9">
<georeferencing scale="4000"/>
<colors count="1"><color priority="0" name="Black" c="0" m="0" y="0" k="1" opacity="1"/></colors>
<symbols count="2" id="ISSprOM 2019"><symbol type="4" id="0" code="999" name="a"><area_symbol inner_color="0" min_area="0" patterns="0"/></symbol><symbol type="16" id="1" code="998" name="c"><combined_symbol parts="1000">{parts}</combined_symbol></symbol></symbols>
<parts count="1" current="0"><part name="p"><objects count="{objects}">{}</objects></part></parts>
<templates count="0"/>
</map>"#,
        object.repeat(objects)
    );
    freemapper_omap::read::read_map(&xml).unwrap()
}

/// Points the fills and strokes of `page` keep.
fn kept_points(page: &RenderedPage) -> usize {
    page.primitives
        .iter()
        .map(|p| match &p.shape {
            Shape::Fill { rings, .. } => rings.iter().map(Vec::len).sum(),
            Shape::Stroke { path, .. } => path.len(),
            _ => 0,
        })
        .sum()
}

#[test]
fn page_budget_counts_work_units() {
    use crate::draw::{BUDGET_SLACK, KEPT_POINTS_PER_UNIT, work_units};
    // One object: its own budget cuts it.
    let before = work_units();
    let (page, report) = render(&thousand_fills(1), &options(Orientation::Portrait));
    let work = work_units() - before;
    assert_eq!(
        (
            report.truncated_objects,
            report.objects_not_drawn_page_limit
        ),
        (1, 0)
    );
    assert!(work <= MAX_UNITS_PER_OBJECT + BUDGET_SLACK, "{work}");
    // Three: two use up the page, the third is not drawn.
    let before = work_units();
    let (page3, report) = render(&thousand_fills(3), &options(Orientation::Portrait));
    let work = work_units() - before;
    assert_eq!(
        (
            report.truncated_objects,
            report.objects_not_drawn_page_limit
        ),
        (2, 1)
    );
    assert!(work <= MAX_UNITS_PER_PAGE + 2 * BUDGET_SLACK, "{work}");
    let points = kept_points(&page3);
    assert!(
        points <= MAX_UNITS_PER_PAGE * KEPT_POINTS_PER_UNIT,
        "{points}"
    );
    assert_eq!(page3.primitives.len(), 2 * page.primitives.len());
}

#[test]
fn the_forest_sample_stays_far_below_the_page_budget() {
    // print's test of the real maps, for the map this crate has (Lübeck stays in print). The forest sample is
    // drawn at 1:10000; the rows are print's forest table (print scale, IOF size factor), and the factors are
    // the ones print's scale policy computes from them.
    use crate::draw::work_units;
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/maps/forest-sample.omap"
    );
    let doc = freemapper_omap::read::read_map_file(std::path::Path::new(path)).unwrap();
    assert_eq!(doc.georef.scale_denominator, 10000);
    let map_size = 1.5;
    for (scale, size) in [
        (5000u32, 3.0),
        (7500, 2.0),
        (10000, 1.5),
        (15000, 1.0),
        (20000, 1.0),
    ] {
        let options = PageSpec {
            symbol_factor: size / map_size,
            geometry_factor: f64::from(doc.georef.scale_denominator) / f64::from(scale),
            show_sheet_objects: true,
            ..options(Orientation::Auto)
        };
        let before = work_units();
        let (page, report) = render(&doc, &options);
        let units = work_units() - before;
        eprintln!(
            "forest-sample.omap 1:{scale}: {units} units, {} primitives, {} kept points",
            page.primitives.len(),
            kept_points(&page)
        );
        assert!(units < MAX_UNITS_PER_PAGE / 10, "1:{scale}: {units}");
        assert_eq!(
            (
                report.truncated_objects,
                report.objects_not_drawn_page_limit
            ),
            (0, 0)
        );
    }
}

#[test]
fn primitives_come_in_paint_order() {
    // An edge line (501.1, black) before a water area (blue, a higher number): water is
    // painted first, the line on top, though the file lists the line first.
    let set = SymbolSet::issprom_2019();
    let water = set.id_for_code("301.1").unwrap();
    let doc = map(
        4000,
        set,
        vec![line(edge(), (25_000, 0)), area(water, 10_000, 10_000)],
    );
    let (page, _) = render(&doc, &options(Orientation::Portrait));
    let priorities: Vec<i32> = page.primitives.iter().map(|p| p.priority).collect();
    assert!(
        priorities.windows(2).all(|w| w[0] >= w[1]),
        "{priorities:?}"
    );
    assert!(priorities.len() >= 2);
}

#[test]
fn overflow_counts_what_is_painted() {
    // A text anchored right at the edge of the printable area still sticks out with its
    // glyphs: the overflow measures them, not the anchor.
    let set = SymbolSet::issprom_2019();
    let id = set.id_for_code("704").unwrap();
    let text = MapObject {
        symbol_id: id,
        kind: MapObjectKind::Text {
            text: "Wald".into(),
            h_align: 0,
            v_align: 0,
            rotation_rad: 0.0,
            box_size: None,
        },
        coords: vec![MapCoord::new(100_000, 0)],
        tags: Vec::new(),
        pattern: None,
    };
    let doc = map(4000, set, vec![line(edge(), (100_000, 0)), text]);
    let centred = PageSpec {
        centre: Some(MapCoord::new(0, 0)),
        ..options(Orientation::Portrait)
    };
    // The line ends 5 mm inside the page edge (x = 205 mm), on the clip; the text runs on.
    let (_, report) = render(&doc, &centred);
    let (dx, _) = report.overflow_mm.expect("the text sticks out");
    assert!(dx > 3.0, "{dx}");
}

#[test]
fn frame_box_is_the_page_minus_the_margin() {
    let frame = |min, max| PaperBox { min, max };
    assert_eq!(
        frame_box((210.0, 297.0), PAGE_MARGIN_MM),
        frame((5.0, 5.0), (205.0, 292.0))
    );
    // A margin wider than half the page leaves a frame of zero width, then zero height.
    assert_eq!(
        frame_box((210.0, 297.0), 120.0),
        frame((105.0, 120.0), (105.0, 177.0))
    );
    assert_eq!(
        frame_box((210.0, 297.0), 500.0),
        frame((105.0, 148.5), (105.0, 148.5))
    );
    assert_eq!(
        frame_box((210.0, 297.0), f64::NAN),
        frame((0.0, 0.0), (210.0, 297.0))
    );
}

#[test]
fn auto_orientation_measures_against_the_frame() {
    // 196 mm wide, 150 mm tall: fits in portrait with the 5 mm margin (200 mm), and only in
    // landscape with a 10 mm margin (190 mm wide in portrait).
    let doc = map(
        4000,
        SymbolSet::issprom_2019(),
        vec![line(edge(), (196_000, 150_000))],
    );
    let (page, _) = render(&doc, &options(Orientation::Auto));
    assert_eq!(page.page_mm, (210.0, 297.0));
    let wide = PageSpec {
        frame_margin_mm: Some(10.0),
        ..options(Orientation::Auto)
    };
    let (page, report) = render(&doc, &wide);
    assert_eq!(page.page_mm, (297.0, 210.0));
    assert_eq!(page.clip, frame_box((297.0, 210.0), 10.0));
    assert_eq!(report.overflow_mm, None);
}

#[test]
fn dropped_patterns_and_unknown_objects_are_reported() {
    // A spacing of 0.01 mm breaks the 0.05 mm limit: the pattern is dropped when read.
    let (set, id) = hatched(10);
    let mut doc = map(
        4000,
        set,
        vec![area(id, 10_000, 10_000), line(edge(), (5_000, 0))],
    );
    doc.unknown_objects = 3;
    let (page, report) = render(&doc, &options(Orientation::Portrait));
    assert_eq!(report.dropped_pattern_objects, 1);
    assert_eq!(report.unknown_type_objects, 3);
    assert_eq!(page.primitives.len(), 1, "only the line");
}

#[test]
fn nested_pattern_outlines_stay_shared_on_the_page() {
    // A hatched square at every point of an outer pattern: each hatch line is clipped to
    // the outer area and, inside that, to its own square. Moving the page must not copy
    // either outline per line.
    let set = SymbolSet::issprom_2019().with_additions(concat!(
        r#"<symbol type="4" id="@" code="999.2" name="nested"><area_symbol inner_color="-1" min_area="0" patterns="1">"#,
        r#"<pattern type="2" angle="0" line_spacing="2000" line_offset="0" offset_along_line="0" point_distance="2000">"#,
        r#"<symbol type="1" code=""><point_symbol inner_radius="0" inner_color="-1" outer_width="0" outer_color="-1" elements="1"><element>"#,
        r#"<symbol type="4" code=""><area_symbol inner_color="-1" min_area="0" patterns="1"><pattern type="1" angle="0" line_spacing="100" line_offset="0" offset_along_line="0" color="2" line_width="10"/></area_symbol></symbol>"#,
        r#"<object type="1"><coords count="5">-500 -500;500 -500;500 500;-500 500;-500 -500 18;</coords></object>"#,
        r#"</element></point_symbol></symbol></pattern></area_symbol></symbol>"#,
    ));
    let id = set.id_for_code("999.2").unwrap();
    let doc = map(4000, set, vec![area(id, 10_000, 10_000)]);
    let (page, _) = render(&doc, &options(Orientation::Portrait));
    // Every outline (by content) is one Arc.
    let mut outlines: Vec<&Rings> = Vec::new();
    let mut clipped = 0;
    for p in &page.primitives {
        let mut shape = &p.shape;
        while let Shape::Clipped { rings, inner } = shape {
            clipped += 1;
            match outlines.iter().find(|o| o[..] == rings[..]) {
                Some(o) => assert!(Arc::ptr_eq(o, rings), "an outline was copied"),
                None => outlines.push(rings),
            }
            shape = inner;
        }
    }
    // A square at each of the 36 placements, ten lines each, two clips per line: the outer
    // outline and one per square.
    assert!(clipped > 400, "{clipped}");
    assert_eq!(outlines.len(), 37);
}

/// A map of `objects` circles of `points` points, each with `patterns` line patterns of one
/// colour, clipped and unclipped (`no_clipping="2"`) in turn, one line each.
fn interleaved(patterns: usize, points: u32, objects: usize) -> MapDocument {
    let xml: String = (0..patterns)
        .map(|i| {
            format!(
                r#"<pattern type="1" angle="0" no_clipping="{}" line_spacing="1000000" line_offset="0" offset_along_line="0" color="2" line_width="10"/>"#,
                if i % 2 == 0 { 0 } else { 2 }
            )
        })
        .collect();
    let set = SymbolSet::issprom_2019().with_additions(&format!(
        r#"<symbol type="4" id="@" code="999.3" name="interleaved"><area_symbol inner_color="-1" min_area="0" patterns="{patterns}">{xml}</area_symbol></symbol>"#
    ));
    let circle: Vec<(i32, i32)> = (0..points)
        .map(|i| {
            let a = f64::from(i) / f64::from(points) * std::f64::consts::TAU;
            (
                (50_000.0 * a.cos()).round() as i32,
                (50_000.0 * a.sin()).round() as i32,
            )
        })
        .collect();
    let object = MapObject {
        coords: freemapper_omap::closed_ring(&circle),
        ..area(set.id_for_code("999.3").unwrap(), 0, 0)
    };
    map(4000, set, vec![object; objects])
}

#[test]
fn every_clip_block_the_pdf_writes_is_charged() {
    // 40 patterns on a 2000 point circle: the PDF writes the outline 20 times.
    use crate::draw::{KEPT_POINTS_PER_UNIT, work_units};
    let doc = interleaved(40, 2000, 1);
    let before = work_units();
    let (page, report) = render(&doc, &options(Orientation::Portrait));
    let work = work_units() - before;
    assert_eq!(report.truncated_objects, 0);
    let (pdf, _) = write_pdf(&page, &PdfMeta::new("clips")).unwrap();
    let content = crate::pdf::content_of(&pdf);
    let blocks = content.windows(4).filter(|w| w == b"W*\nn").count();
    assert_eq!(blocks, 20);
    // Each block writes the 2000 point outline: the units pay for all of them.
    assert!(
        work * KEPT_POINTS_PER_UNIT >= blocks * 2000,
        "{work} units for {blocks} blocks"
    );
}

#[test]
fn a_spent_object_asks_for_no_more_outlines() {
    // The reviewer's probe, smaller: 300 clipped patterns on a 20 000 point circle need
    // 750 000 units. Each object is cut at its own budget, and the patterns after that do not
    // ask for their outlines, so the second object still gets the rest of the page.
    use crate::draw::{BUDGET_SLACK, work_units};
    let doc = interleaved(600, 20_000, 4);
    let before = work_units();
    let (_, report) = render(&doc, &options(Orientation::Portrait));
    let work = work_units() - before;
    assert_eq!(
        (
            report.truncated_objects,
            report.objects_not_drawn_page_limit
        ),
        (2, 2)
    );
    // One outline (2500 units) may be asked for beyond each object's budget.
    assert!(
        work <= MAX_UNITS_PER_PAGE + 2 * (2_500 + BUDGET_SLACK),
        "{work}"
    );
}

#[test]
fn pattern_rings_stay_shared_after_moving() {
    let rings: Rings = vec![vec![(0.0, 0.0), (1.0, 0.0), (1.0, 1.0)]].into();
    let clipped = |rings: &Rings| {
        Primitive::new(
            2,
            Shape::Clipped {
                rings: Arc::clone(rings),
                inner: Box::new(Shape::Fill {
                    rings: vec![vec![(0.0, 0.0), (1.0, 1.0), (0.0, 1.0)]],
                    rule: FillRule::EvenOdd,
                }),
            },
        )
    };
    let mut primitives = vec![clipped(&rings), clipped(&rings)];
    translate(&mut primitives, (10.0, 20.0));
    let (Shape::Clipped { rings: a, inner }, Shape::Clipped { rings: b, .. }) =
        (&primitives[0].shape, &primitives[1].shape)
    else {
        panic!()
    };
    assert!(Arc::ptr_eq(a, b));
    assert_eq!(a[0][1], (11.0, 20.0));
    assert_eq!(
        **inner,
        Shape::Fill {
            rings: vec![vec![(10.0, 20.0), (11.0, 21.0), (10.0, 21.0)]],
            rule: FillRule::EvenOdd,
        }
    );
}

// --- The base area of sprint maps (ADR 0044) ---

/// A small sprint map near the origin on the walkable-area palette: an edge line from (0, 0) to `to`
/// and a building, with freemapper's base rectangle when `base` is set. The rectangle is far larger
/// than the map and lies off-centre, so it would move the centre and the overflow if it counted.
fn sprint_map(to: (i32, i32), base: bool) -> MapDocument {
    let set = base_fixture::sprint_set();
    let building = area(set.id_for_code("521").unwrap(), 20_000, 10_000);
    let mut doc = map(
        4000,
        set.clone(),
        vec![line(set.id_for_code("501.1").unwrap(), to), building],
    );
    if base {
        let part = base_fixture::base_part(&set, (-900_000, -300_000), (400_000, 700_000));
        doc.parts.insert(0, part);
    }
    doc
}

#[test]
fn base_area_does_not_move_the_page_centre() {
    for (to, orientation) in [
        ((60_000, 40_000), Orientation::Auto),
        // 250 mm wide: overflows a portrait A4 by 50.1 mm, and only by that.
        ((250_000, 150_000), Orientation::Portrait),
    ] {
        let plain = render(&sprint_map(to, false), &options(orientation));
        let based = render(&sprint_map(to, true), &options(orientation));
        assert_eq!(based.0.centre_mm, plain.0.centre_mm);
        assert_eq!(based.0.page_mm, plain.0.page_mm);
        assert_eq!(based.0.clip, plain.0.clip);
        assert_eq!(based.1.overflow_mm, plain.1.overflow_mm);
    }
    let (_, report) = render(
        &sprint_map((250_000, 150_000), true),
        &options(Orientation::Portrait),
    );
    // The line's 250 mm and its stroke ends on 200 mm of frame; the base area adds nothing.
    let (dx, dy) = report.overflow_mm.unwrap();
    assert!((50.0..50.2).contains(&dx) && dy == 0.0, "{dx} {dy}");
}

#[test]
fn base_area_is_still_painted() {
    let doc = sprint_map((60_000, 40_000), true);
    let base_id = doc.symbol_set.id_for_code("990").unwrap();
    let (page, report) = render(&doc, &options(Orientation::Auto));
    let walkable = page
        .colours
        .iter()
        .find(|c| c.name == "Lower brown 30% for walkable area")
        .expect("the colour is read like any other");
    assert_eq!(walkable.priority, 32);
    let base: Vec<usize> = (0..page.primitives.len())
        .filter(|&i| page.primitives[i].base)
        .collect();
    assert!(!base.is_empty(), "the base area is drawn");
    for &i in &base {
        let p = &page.primitives[i];
        assert_eq!((p.priority, p.symbol), (32, base_id), "{p:?}");
    }
    // Only the base object is flagged.
    assert!(
        page.primitives
            .iter()
            .all(|p| p.base == (p.symbol == base_id))
    );
    // Painted first: before every primitive of a lower priority number.
    let last_base = *base.last().unwrap();
    let first_other = page
        .primitives
        .iter()
        .position(|p| p.priority < 32)
        .unwrap();
    assert!(last_base < first_other, "{last_base} {first_other}");
    // It reaches far beyond the frame; the page clips it there like every map primitive.
    let (min, max) = shape_bounds(&page.primitives[base[0]].shape).unwrap();
    let frame = page.clip;
    assert!(min.0 < frame.min.0 && min.1 < frame.min.1);
    assert!(max.0 > frame.max.0 && max.1 > frame.max.1);
    assert_eq!(frame, frame_box(page.page_mm, PAGE_MARGIN_MM));
    assert_eq!(report.overflow_mm, None);
    // The PDF paints it in its CMYK, clipped to the frame.
    let (pdf, _) = write_pdf(&page, &PdfMeta::new("base")).unwrap();
    let content = String::from_utf8_lossy(&crate::pdf::content_of(&pdf)).into_owned();
    let clip = content.find("re\nW").expect("the frame clip");
    let fill = content
        .find("0.06 0.23 0.33 0 k")
        .expect("the walkable area colour");
    assert!(clip < fill, "{content:.400}");
}
