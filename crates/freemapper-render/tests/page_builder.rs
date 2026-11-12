//! The page builder through the public API, as a browser editor uses it: map → page → hit test → map
//! (ADR 0023).

use freemapper_omap::{
    MapCoord, MapDocument, MapGeoref, MapObject, MapObjectKind, MapPart, SymbolSet, closed_ring,
};
use freemapper_render::build::{Orientation, PageSpec, Paper, SHEET_TAG, build_page};
use freemapper_render::page::RenderedPage;
use freemapper_render::pdf::{PdfMeta, write_pdf};

fn map(set: SymbolSet, objects: Vec<MapObject>) -> MapDocument {
    MapDocument {
        notes: String::new(),
        georef: MapGeoref {
            scale_denominator: 4000,
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

/// A square area of open land (401) from (0, 0) to (`side`, `side`) µm.
fn open_land(set: &SymbolSet, side: i32) -> MapObject {
    MapObject {
        symbol_id: set.id_for_code("401").unwrap(),
        kind: MapObjectKind::Path,
        coords: closed_ring(&[(0, 0), (side, 0), (side, side), (0, side)]),
        tags: Vec::new(),
        pattern: None,
    }
}

/// The distinct objects drawn on `page`.
fn drawn_objects(page: &RenderedPage) -> Vec<u32> {
    let mut objects: Vec<u32> = page.primitives.iter().map(|p| p.object).collect();
    objects.sort_unstable();
    objects.dedup();
    objects
}

#[test]
fn show_sheet_objects_adds_the_information_block() {
    let set = SymbolSet::issprom_2019();
    let sheet_line = MapObject {
        symbol_id: set.id_for_code("501.1").unwrap(),
        kind: MapObjectKind::Path,
        coords: vec![MapCoord::new(0, 50_000), MapCoord::new(30_000, 50_000)],
        tags: vec![(SHEET_TAG.0.into(), SHEET_TAG.1.into())],
        pattern: None,
    };
    let doc = map(set.clone(), vec![open_land(&set, 40_000), sheet_line]);
    let without = PageSpec::new(1.0, 1.0, Paper::A4);
    let with = PageSpec {
        show_sheet_objects: true,
        ..without.clone()
    };
    let (page, report) = build_page(&doc, &without).unwrap();
    assert_eq!(drawn_objects(&page), [0]);
    assert_eq!(
        report.skipped_objects, 0,
        "left-out sheet objects do not count"
    );
    let (page, report) = build_page(&doc, &with).unwrap();
    assert_eq!(drawn_objects(&page), [0, 1]);
    assert_eq!(report.skipped_objects, 0);
}

#[test]
fn centre_moves_the_map_under_the_page() {
    let set = SymbolSet::issprom_2019();
    let doc = map(set.clone(), vec![open_land(&set, 40_000)]);
    // 1:5000 on the 1:4000 map: 40 mm of map become 32 mm on paper.
    let auto = PageSpec {
        orientation: Orientation::Portrait,
        ..PageSpec::new(1.0, 0.8, Paper::A4)
    };
    let (page, _) = build_page(&doc, &auto).unwrap();
    assert_eq!(
        page.centre_mm,
        (16.0, 16.0),
        "the middle of what is painted"
    );
    let set_centre = PageSpec {
        centre: Some(MapCoord::new(50_000, 30_000)),
        ..auto
    };
    let (moved, _) = build_page(&doc, &set_centre).unwrap();
    assert_eq!(
        moved.centre_mm,
        (40.0, 24.0),
        "the centre times the geometry factor, in mm"
    );
    // Every primitive moves by the difference of the centres.
    let first_point = |page: &RenderedPage| match &page.primitives[0].shape {
        freemapper_render::page::Shape::Fill { rings, .. } => rings[0][0],
        other => panic!("{other:?}"),
    };
    let (a, b) = (first_point(&page), first_point(&moved));
    assert!(
        (a.0 - b.0 - 24.0).abs() < 1e-9 && (a.1 - b.1 - 8.0).abs() < 1e-9,
        "{a:?} {b:?}"
    );
    // The page mapping puts the chosen centre in the middle of the page.
    assert_eq!(moved.mapping().to_page((40.0, 24.0)), (105.0, 148.5));
}

#[test]
fn the_page_centre_hits_the_area_of_a_one_area_map() {
    let set = SymbolSet::issprom_2019();
    let doc = map(set.clone(), vec![open_land(&set, 40_000)]);
    let spec = PageSpec::new(1.0, 0.8, Paper::A4);
    let (page, _) = build_page(&doc, &spec).unwrap();
    let middle = (page.page_mm.0 / 2.0, page.page_mm.1 / 2.0);
    let hits = page.hits_at(middle, 0.0);
    assert_eq!(hits.len(), 1, "{hits:?}");
    let object = &doc.parts[0].objects[hits[0].object as usize];
    assert_eq!(hits[0].symbol, object.symbol_id);
    assert_eq!(hits[0].symbol, set.id_for_code("401").unwrap());
    // Back to the map: the middle of the page is the middle of the square, (20 mm, 20 mm) in map units.
    let (x, y) = page.mapping().to_map(middle);
    let to_um = |mm: f64| mm / spec.geometry_factor * 1000.0;
    assert!((to_um(x) - 20_000.0).abs() < 1e-6 && (to_um(y) - 20_000.0).abs() < 1e-6);
    // Just outside the square there is nothing.
    let outside = page.mapping().to_page((32.5, 16.0));
    assert!(page.hits_at(outside, 0.0).is_empty());
}

#[test]
fn a_cropped_sheet_writes_only_what_its_frame_can_show() {
    // ADR 0031. The forest sample (drawn at 1:10000, about 58 x 54 mm) printed at 1:2000 paints about 290 x 270
    // mm. A page of 100 x 80 mm shows a tenth of it; a page of 320 x 300 mm holds all of it, the uncropped sheet.
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/maps/forest-sample.omap"
    );
    let doc = freemapper_omap::read::read_map_file(std::path::Path::new(path)).unwrap();
    let at = |page_mm: (f64, f64)| PageSpec {
        page_mm: Some(page_mm),
        ..PageSpec::new(2.5, 5.0, Paper::A4)
    };
    let (whole, whole_report) = build_page(&doc, &at((320.0, 300.0))).unwrap();
    let (cropped, cropped_report) = build_page(&doc, &at((100.0, 80.0))).unwrap();
    assert_eq!(whole_report.overflow_mm, None);
    assert!(cropped_report.overflow_mm.is_some());
    // The page keeps every primitive and the report every object: only the PDF leaves out what cannot show.
    assert_eq!(cropped.primitives.len(), whole.primitives.len());
    assert_eq!(cropped_report.source_objects, whole_report.source_objects);
    let size = |page: &RenderedPage| write_pdf(page, &PdfMeta::new("forest")).unwrap().0.len();
    let (whole, cropped) = (size(&whole), size(&cropped));
    // About 31 %: the fonts, the areas that cross the frame and what lies within 5 mm of it stay.
    assert!(
        cropped * 5 < whole * 2,
        "cropped {cropped} bytes, whole {whole} bytes"
    );
}
