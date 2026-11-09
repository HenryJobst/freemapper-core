//! Fitting a course file to the map (M3 Task 3, ADR 0032).

use std::path::Path;

use freemapper_course::fit::{
    FitError, FitNote, FitOptions, GEOREF_TOLERANCE_M, SkipReason, YAxis, fit,
};
use freemapper_course::model::Offset;
use freemapper_course::{
    Control, ControlKind, Course, CourseEvent, CourseKind, CourseSource, Leg, RawEvent, RawPoint,
    SourceFormat, Stop, read_iof_xml,
};
use freemapper_omap::read::read_map_file;
use freemapper_omap::{
    MapCoord, MapDocument, MapGeoref, MapObject, MapObjectKind, MapPart, SymbolSet,
};

fn luebeck() -> MapDocument {
    let path = format!(
        "{}/tests/fixtures/maps/luebeck.omap",
        env!("CARGO_MANIFEST_DIR")
    );
    read_map_file(Path::new(&path)).unwrap()
}

fn luebeck_course() -> RawEvent {
    let path = format!(
        "{}/tests/fixtures/courses/luebeck-georef.xml",
        env!("CARGO_MANIFEST_DIR")
    );
    read_iof_xml(&std::fs::read(path).unwrap()).unwrap()
}

/// A map whose objects span `min` to `max` (µm, y down), not georeferenced.
fn plain_map(min: (i32, i32), max: (i32, i32)) -> MapDocument {
    MapDocument {
        notes: String::new(),
        georef: MapGeoref {
            scale_denominator: 4000,
            ..MapGeoref::default()
        },
        symbol_set: SymbolSet::issprom_2019(),
        parts: vec![MapPart {
            name: "default part".into(),
            objects: vec![MapObject {
                symbol_id: 0,
                kind: MapObjectKind::Path,
                coords: vec![MapCoord::new(min.0, min.1), MapCoord::new(max.0, max.1)],
                tags: Vec::new(),
                pattern: None,
            }],
        }],
        templates: Vec::new(),
        unknown_objects: 0,
    }
}

fn control(code: &str, xy: Option<(f64, f64)>, geo: Option<(f64, f64)>) -> Control<RawPoint> {
    Control {
        id: None,
        code: code.into(),
        kind: ControlKind::Control,
        position: xy.map(|(x_mm, y_mm)| RawPoint { x_mm, y_mm }),
        geo,
        description: None,
    }
}

/// An event with `controls` and one course over the first two of them, with a bend and a number
/// text offset.
fn event(controls: Vec<Control<RawPoint>>) -> RawEvent {
    let codes: Vec<String> = controls.iter().map(|c| c.code.clone()).collect();
    let stop = |code: &String, at: Option<Offset>| Stop {
        control: code.clone(),
        kind: None,
        number: None,
        number_text_at: at,
        circle_gaps: Vec::new(),
        map_flip: false,
    };
    CourseEvent {
        courses: vec![Course {
            id: None,
            name: "A".into(),
            family: None,
            kind: CourseKind::Normal,
            legs: vec![Leg {
                bends: vec![RawPoint {
                    x_mm: 1.5,
                    y_mm: 2.25,
                }],
                gaps: Vec::new(),
                marked_route: false,
            }],
            stops: vec![
                stop(&codes[0], None),
                stop(
                    &codes[1],
                    Some(Offset {
                        dx_mm: 4.0,
                        dy_mm: 3.0,
                    }),
                ),
            ],
            climb_m: None,
            length_m: None,
            print_scale_hint: None,
            course_obj_ratio: None,
            secondary_title: None,
        }],
        controls,
        specials: Vec::new(),
        source: CourseSource {
            format: SourceFormat::IofXml,
            event_name: None,
            map_scale: None,
            item_scaling: None,
        },
        notes: Vec::new(),
    }
}

fn pos(ev: &freemapper_course::FittedEvent, code: &str) -> MapCoord {
    ev.controls
        .iter()
        .find(|c| c.code == code)
        .and_then(|c| c.position)
        .unwrap()
}

#[test]
fn y_up_detected() {
    // Map from (0, 0) to (200, 100) mm, y down: a file with negative y only fits with y up.
    let map = plain_map((0, 0), (200_000, 100_000));
    let ev = event(vec![
        control("31", Some((50.0, -30.0)), None),
        control("32", Some((150.0, -80.0)), None),
    ]);
    let fitted = fit(ev, &map, &FitOptions::default()).unwrap();
    assert!(fitted.notes.contains(&FitNote::YUpDetected));
    assert_eq!(pos(&fitted.event, "31"), MapCoord::new(50_000, 30_000));
    assert_eq!(pos(&fitted.event, "32"), MapCoord::new(150_000, 80_000));
    let course = &fitted.event.courses[0];
    assert_eq!(course.legs[0].bends, [MapCoord::new(1_500, -2_250)]);
    // Offsets take the flip, no translation.
    assert_eq!(
        course.stops[1].number_text_at,
        Some(Offset {
            dx_mm: 4.0,
            dy_mm: -3.0
        })
    );
    assert!(
        fitted
            .notes
            .contains(&FitNote::GeorefSkipped(SkipReason::NotGeoreferenced))
    );
}

#[test]
fn y_down_kept() {
    let map = plain_map((0, 0), (200_000, 100_000));
    let ev = event(vec![
        control("31", Some((50.0, 30.0)), None),
        control("32", Some((150.0, 80.0)), None),
    ]);
    let fitted = fit(ev, &map, &FitOptions::default()).unwrap();
    assert!(!fitted.notes.contains(&FitNote::YUpDetected));
    assert_eq!(pos(&fitted.event, "31"), MapCoord::new(50_000, 30_000));
    let course = &fitted.event.courses[0];
    assert_eq!(course.legs[0].bends, [MapCoord::new(1_500, 2_250)]);
    assert_eq!(
        course.stops[1].number_text_at,
        Some(Offset {
            dx_mm: 4.0,
            dy_mm: 3.0
        })
    );
}

#[test]
fn both_fit_without_lat_lng_prefers_y_up() {
    let map = plain_map((-100_000, -100_000), (100_000, 100_000));
    let ev = event(vec![
        control("31", Some((10.0, 20.0)), None),
        control("32", Some((-30.0, 40.0)), None),
    ]);
    let fitted = fit(ev.clone(), &map, &FitOptions::default()).unwrap();
    // Nothing decided it: y up is assumed, and that is a warning, not a detection.
    assert!(!fitted.notes.contains(&FitNote::YUpDetected));
    assert!(fitted.notes.contains(&FitNote::OrientationAssumed));
    assert!(FitNote::OrientationAssumed.is_warning());
    assert!(!FitNote::YUpDetected.is_warning());
    assert_eq!(pos(&fitted.event, "31"), MapCoord::new(10_000, -20_000));
    // The project file can set the axis.
    let down = FitOptions {
        y_axis: Some(YAxis::Down),
        ..FitOptions::default()
    };
    let fitted = fit(ev, &map, &down).unwrap();
    assert!(!fitted.notes.contains(&FitNote::OrientationAssumed));
    assert_eq!(pos(&fitted.event, "31"), MapCoord::new(10_000, 20_000));
}

#[test]
fn set_y_axis_must_still_fit() {
    let map = plain_map((0, 0), (200_000, 100_000));
    let ev = event(vec![
        control("31", Some((50.0, 30.0)), None),
        control("32", Some((150.0, 80.0)), None),
    ]);
    let up = FitOptions {
        y_axis: Some(YAxis::Up),
        ..FitOptions::default()
    };
    assert!(matches!(
        fit(ev, &map, &up),
        Err(FitError::OutsideMap {
            inside: 0,
            total: 2
        })
    ));
}

#[test]
fn file_scale_must_match_the_map() {
    let map = plain_map((0, 0), (200_000, 100_000));
    let mut ev = event(vec![
        control("31", Some((50.0, 30.0)), None),
        control("32", Some((150.0, 80.0)), None),
    ]);
    let fitted = fit(ev.clone(), &map, &FitOptions::default()).unwrap();
    assert!(fitted.notes.contains(&FitNote::ScaleUnchecked));
    ev.source.map_scale = Some(4000);
    let fitted = fit(ev.clone(), &map, &FitOptions::default()).unwrap();
    assert!(!fitted.notes.contains(&FitNote::ScaleUnchecked));
    ev.source.map_scale = Some(10000);
    let err = fit(ev, &map, &FitOptions::default()).unwrap_err();
    assert_eq!(
        err,
        FitError::ScaleMismatch {
            file: 10000,
            map: 4000
        }
    );
    assert!(err.to_string().contains("10000"), "{err}");
}

#[test]
fn ten_mm_margin_around_the_map() {
    let map = plain_map((0, 0), (200_000, 100_000));
    // 9.9 mm beyond the left edge fits, 10.1 mm does not.
    let near = event(vec![
        control("31", Some((-9.9, 50.0)), None),
        control("32", Some((150.0, 80.0)), None),
    ]);
    assert!(fit(near, &map, &FitOptions::default()).is_ok());
    let far = event(vec![
        control("31", Some((-10.1, 50.0)), None),
        control("32", Some((150.0, 80.0)), None),
    ]);
    assert!(matches!(
        fit(far, &map, &FitOptions::default()),
        Err(FitError::OutsideMap {
            inside: 1,
            total: 2
        })
    ));
}

#[test]
fn course_outside_map_is_error() {
    let map = plain_map((0, 0), (200_000, 100_000));
    let ev = event(vec![
        control("31", Some((500.0, 30.0)), None),
        control("32", Some((600.0, 80.0)), None),
        control("33", Some((50.0, 40.0)), None),
    ]);
    let err = fit(ev, &map, &FitOptions::default()).unwrap_err();
    assert!(
        matches!(
            err,
            FitError::OutsideMap {
                inside: 1,
                total: 3
            }
        ),
        "{err:?}"
    );
    assert!(err.to_string().contains("1 of 3"), "{err}");
}

#[test]
fn georef_checked_on_luebeck() {
    let fitted = fit(luebeck_course(), &luebeck(), &FitOptions::default()).unwrap();
    assert!(fitted.notes.contains(&FitNote::YUpDetected));
    let checked = fitted.notes.iter().find_map(|n| match n {
        FitNote::GeorefChecked { controls, metres } => Some((*controls, *metres)),
        _ => None,
    });
    let (controls, metres) = checked.expect("GeorefChecked");
    assert_eq!(controls, 4);
    // Positions and lat/lng were made to agree to 0.1 mm (fixture README).
    assert!(metres < 0.01, "{metres}");
    let ev = &fitted.event;
    assert_eq!(pos(ev, "S1"), MapCoord::new(-50_000, -60_000));
    assert_eq!(pos(ev, "32"), MapCoord::new(100_000, 120_000));
    assert_eq!(pos(ev, "F1"), MapCoord::new(60_000, -20_000));
    // Control 33 has only lat/lng; the georeferencing places it at (-80000, 90000).
    let p = pos(ev, "33");
    assert!(
        (p.x + 80_000).abs() <= 1 && (p.y - 90_000).abs() <= 1,
        "{p:?}"
    );
    assert!(
        fitted
            .notes
            .contains(&FitNote::PlacedByGeoref(vec!["33".into()]))
    );
    assert_eq!(
        ev.courses[0].stops[1].number_text_at,
        Some(Offset {
            dx_mm: 4.0,
            dy_mm: -3.0
        })
    );
}

#[test]
fn georef_mismatch_is_error() {
    let mut ev = luebeck_course();
    // Control 31 moved 12.5 mm east on the map: 50 m at 1:4000.
    let c = ev.controls.iter_mut().find(|c| c.code == "31").unwrap();
    c.position.as_mut().unwrap().x_mm += 12.5;
    let err = fit(ev, &luebeck(), &FitOptions::default()).unwrap_err();
    match &err {
        FitError::GeorefMismatch { control, metres } => {
            assert_eq!(control, "31");
            assert!((metres - 50.0).abs() < 0.1, "{metres}");
        }
        other => panic!("{other:?}"),
    }
    assert!(err.to_string().contains("31"), "{err}");
    assert!(
        err.to_string()
            .contains(&format!("{GEOREF_TOLERANCE_M} m allowed")),
        "{err}"
    );
}

#[test]
fn lat_lng_near_the_x_axis_decides_nothing() {
    // One control with lat/lng 1 mm above the map's x axis: y up and y down lie 2 mm (8 m) apart,
    // both within the tolerance. Lat/lng computed like the fixture's (README), map (20000, -1000).
    let ev = event(vec![
        control(
            "N1",
            Some((20.0, 1.0)),
            Some((53.865_978_767, 10.688_217_049)),
        ),
        control("S1", Some((-50.0, 60.0)), None),
        control("F1", Some((60.0, 20.0)), None),
    ]);
    let fitted = fit(ev, &luebeck(), &FitOptions::default()).unwrap();
    assert!(fitted.notes.contains(&FitNote::OrientationAssumed));
    assert!(!fitted.notes.contains(&FitNote::YUpDetected));
    assert!(
        fitted
            .notes
            .iter()
            .any(|n| matches!(n, FitNote::GeorefChecked { controls: 1, .. }))
    );
    assert_eq!(pos(&fitted.event, "N1"), MapCoord::new(20_000, -1_000));
}

#[test]
fn georef_check_off_ignores_lat_lng() {
    let off = FitOptions {
        check_georef: false,
        ..FitOptions::default()
    };
    // Geo-only control 33 cannot be placed.
    let err = fit(luebeck_course(), &luebeck(), &off).unwrap_err();
    assert!(
        matches!(
            &err,
            FitError::NoPositionNoGeoref { controls, reason: SkipReason::CheckOff }
                if controls == &["33".to_string()]
        ),
        "{err:?}"
    );
    // A control 50 m off is not an error, and lat/lng no longer pick y down.
    let mut ev = luebeck_course();
    ev.controls.retain(|c| c.position.is_some());
    ev.controls[1].position.as_mut().unwrap().x_mm += 12.5;
    let fitted = fit(ev, &luebeck(), &off).unwrap();
    assert!(fitted.notes.contains(&FitNote::GeorefCheckOff));
    assert!(fitted.notes.contains(&FitNote::YUpDetected));
    let src = luebeck_course();
    let controls = src
        .controls
        .into_iter()
        .filter(|c| ["S1", "31", "F1"].contains(&c.code.as_str()))
        .map(|mut c| {
            let p = c.position.as_mut().unwrap();
            p.y_mm = -p.y_mm;
            c
        })
        .collect();
    let fitted = fit(event(controls), &luebeck(), &off).unwrap();
    assert!(fitted.notes.contains(&FitNote::OrientationAssumed));
    assert_eq!(pos(&fitted.event, "S1"), MapCoord::new(-50_000, 60_000));
}

#[test]
fn blank_crs_is_not_georeferenced() {
    let mut map = plain_map((0, 0), (200_000, 100_000));
    map.georef.crs_spec = Some("  ".into());
    map.georef.ref_projected = Some((0.0, 0.0));
    let ev = event(vec![
        control("31", Some((50.0, 30.0)), Some((53.86, 10.68))),
        control("32", Some((150.0, 80.0)), None),
    ]);
    let fitted = fit(ev, &map, &FitOptions::default()).unwrap();
    assert!(
        fitted
            .notes
            .contains(&FitNote::GeorefSkipped(SkipReason::NotGeoreferenced))
    );
}

#[test]
fn lat_lng_decides_when_both_orientations_fit() {
    // S1, 31 and F1 of the Lübeck fixture, written y down: both orientations fit the map, and only
    // y down agrees with the lat/lng.
    let src = luebeck_course();
    let controls = src
        .controls
        .into_iter()
        .filter(|c| ["S1", "31", "F1"].contains(&c.code.as_str()))
        .map(|mut c| {
            let p = c.position.as_mut().unwrap();
            p.y_mm = -p.y_mm;
            c
        })
        .collect();
    let fitted = fit(event(controls), &luebeck(), &FitOptions::default()).unwrap();
    assert!(!fitted.notes.contains(&FitNote::YUpDetected));
    assert_eq!(pos(&fitted.event, "S1"), MapCoord::new(-50_000, -60_000));
    assert!(
        fitted
            .notes
            .iter()
            .any(|n| matches!(n, FitNote::GeorefChecked { controls: 3, .. }))
    );
}

#[test]
fn geo_only_control_without_georef_is_error() {
    let map = plain_map((0, 0), (200_000, 100_000));
    let ev = event(vec![
        control("31", Some((50.0, 30.0)), None),
        control("32", None, Some((53.86, 10.68))),
    ]);
    let err = fit(ev, &map, &FitOptions::default()).unwrap_err();
    match &err {
        FitError::NoPositionNoGeoref { controls, reason } => {
            assert_eq!(controls, &["32".to_string()]);
            assert_eq!(reason, &SkipReason::NotGeoreferenced);
        }
        other => panic!("{other:?}"),
    }
    assert!(err.to_string().contains("32"), "{err}");
}

#[test]
fn other_crs_skips_the_check() {
    let mut map = plain_map((0, 0), (200_000, 100_000));
    map.georef.crs_spec = Some("+proj=lcc +lat_1=52 +lat_2=54".into());
    map.georef.ref_projected = Some((0.0, 0.0));
    let ev = event(vec![
        control("31", Some((50.0, 30.0)), Some((53.86, 10.68))),
        control("32", Some((150.0, 80.0)), None),
    ]);
    let fitted = fit(ev, &map, &FitOptions::default()).unwrap();
    assert!(fitted.notes.iter().any(|n| matches!(
        n,
        FitNote::GeorefSkipped(SkipReason::UnsupportedCrs(spec)) if spec.contains("lcc")
    )));
}

#[test]
fn no_lat_lng_in_file_skips_the_check() {
    let mut ev = luebeck_course();
    ev.controls.retain(|c| c.position.is_some());
    for c in &mut ev.controls {
        c.geo = None;
    }
    let fitted = fit(ev, &luebeck(), &FitOptions::default()).unwrap();
    assert!(
        fitted
            .notes
            .contains(&FitNote::GeorefSkipped(SkipReason::NoLatLng))
    );
}

#[test]
fn circle_gaps_flip_with_y() {
    let map = plain_map((0, 0), (200_000, 100_000));
    let with_gaps = || {
        let mut ev = event(vec![
            control("31", Some((50.0, -30.0)), None),
            control("32", Some((150.0, -80.0)), None),
        ]);
        ev.courses[0].stops[0].circle_gaps = vec![(-30.0, 60.0), (100.0, 150.0)];
        ev
    };
    // y up: the file's counter-clockwise angles become map-frame (y down) angles, (a, b) -> (-b, -a).
    let fitted = fit(with_gaps(), &map, &FitOptions::default()).unwrap();
    assert!(fitted.notes.contains(&FitNote::YUpDetected));
    assert_eq!(
        fitted.event.courses[0].stops[0].circle_gaps,
        [(-60.0, 30.0), (-150.0, -100.0)]
    );
    // y down: nothing flips.
    let mut ev = with_gaps();
    for c in &mut ev.controls {
        c.position = c.position.map(|p| RawPoint {
            x_mm: p.x_mm,
            y_mm: -p.y_mm,
        });
    }
    let fitted = fit(ev, &map, &FitOptions::default()).unwrap();
    assert_eq!(
        fitted.event.courses[0].stops[0].circle_gaps,
        [(-30.0, 60.0), (100.0, 150.0)]
    );
}

#[test]
fn circle_gaps_flip_for_a_control_placed_by_georef() {
    let mut ev = luebeck_course();
    // Control 33 has only lat/lng. One gap crosses +x after the flip: (-30, 60) -> (-60, 30).
    for stop in ev.courses.iter_mut().flat_map(|c| &mut c.stops) {
        if stop.control == "33" {
            stop.circle_gaps = vec![(-30.0, 60.0), (100.0, 150.0)];
        }
    }
    let fitted = fit(ev, &luebeck(), &FitOptions::default()).unwrap();
    assert!(fitted.notes.contains(&FitNote::YUpDetected));
    assert!(
        fitted
            .notes
            .contains(&FitNote::PlacedByGeoref(vec!["33".into()]))
    );
    let stop = fitted
        .event
        .courses
        .iter()
        .flat_map(|c| &c.stops)
        .find(|s| s.control == "33")
        .unwrap();
    assert_eq!(stop.circle_gaps, [(-60.0, 30.0), (-150.0, -100.0)]);
    assert!(stop.circle_gaps[0].0 < 0.0 && stop.circle_gaps[0].1 > 0.0);
}

/// `ev` as if Purple Pen had written it.
fn as_ppen(mut ev: RawEvent) -> RawEvent {
    ev.source.format = SourceFormat::PurplePen;
    ev
}

#[test]
fn purple_pen_files_are_y_up() {
    // Ruling R36: Purple Pen always writes y up. Where both readings fit, nothing is assumed or detected.
    let map = plain_map((-100_000, -100_000), (100_000, 100_000));
    let ev = as_ppen(event(vec![
        control("31", Some((10.0, 20.0)), None),
        control("32", Some((-30.0, 40.0)), None),
    ]));
    let fitted = fit(ev, &map, &FitOptions::default()).unwrap();
    assert!(
        !fitted.notes.iter().any(|n| matches!(
            n,
            FitNote::OrientationAssumed | FitNote::YUpDetected | FitNote::AxisAgainstFormat { .. }
        )),
        "{:?}",
        fitted.notes
    );
    assert!(!fitted.notes.iter().any(FitNote::is_warning));
    assert_eq!(pos(&fitted.event, "31"), MapCoord::new(10_000, -20_000));
}

#[test]
fn purple_pen_file_that_fits_only_y_down_is_outside_the_map() {
    // Never mirrored silently: read y up, the controls lie outside the map.
    let map = plain_map((0, 0), (200_000, 100_000));
    let ev = as_ppen(event(vec![
        control("31", Some((50.0, 30.0)), None),
        control("32", Some((150.0, 80.0)), None),
    ]));
    assert_eq!(
        fit(ev, &map, &FitOptions::default()).unwrap_err(),
        FitError::OutsideMap {
            inside: 0,
            total: 2
        }
    );
}

#[test]
fn project_axis_against_purple_pen_is_a_warning() {
    // The project file still decides, but a y axis down on a Purple Pen file is noted as a warning: the sheet
    // would print the course mirrored.
    let map = plain_map((-100_000, -100_000), (100_000, 100_000));
    let ev = as_ppen(event(vec![
        control("31", Some((10.0, 20.0)), None),
        control("32", Some((-30.0, 40.0)), None),
    ]));
    let down = FitOptions {
        y_axis: Some(YAxis::Down),
        ..FitOptions::default()
    };
    let fitted = fit(ev.clone(), &map, &down).unwrap();
    let note = FitNote::AxisAgainstFormat {
        set: YAxis::Down,
        format: YAxis::Up,
    };
    assert!(fitted.notes.contains(&note), "{:?}", fitted.notes);
    assert!(note.is_warning());
    assert_eq!(pos(&fitted.event, "31"), MapCoord::new(10_000, 20_000));
    // Setting what the format says anyway is fine.
    let up = FitOptions {
        y_axis: Some(YAxis::Up),
        ..FitOptions::default()
    };
    let fitted = fit(ev, &map, &up).unwrap();
    assert!(!fitted.notes.iter().any(FitNote::is_warning));
}

#[test]
fn geo_only_controls_leave_offsets_and_bends_undecided() {
    // Every control has only lat/lng: they place themselves, but the bend and the number offset in the file
    // still depend on the y axis, and nothing decides it for IOF XML. That is assumed, with the warning.
    let mut ev = luebeck_course();
    for c in &mut ev.controls {
        c.position = None;
    }
    let fitted = fit(ev.clone(), &luebeck(), &FitOptions::default()).unwrap();
    assert!(
        fitted.notes.contains(&FitNote::OrientationAssumed),
        "{:?}",
        fitted.notes
    );
    // Without offsets, bends, gaps and specials the axis does not matter.
    for course in &mut ev.courses {
        for stop in &mut course.stops {
            stop.number_text_at = None;
            stop.circle_gaps.clear();
        }
        for leg in &mut course.legs {
            leg.bends.clear();
        }
    }
    ev.specials.clear();
    let fitted = fit(ev, &luebeck(), &FitOptions::default()).unwrap();
    assert!(!fitted.notes.contains(&FitNote::OrientationAssumed));
}

#[test]
fn file_scale_against_a_map_without_scale_reads_plainly() {
    let mut map = plain_map((0, 0), (200_000, 100_000));
    map.georef.scale_denominator = 0;
    let mut ev = event(vec![
        control("31", Some((50.0, 30.0)), None),
        control("32", Some((150.0, 80.0)), None),
    ]);
    ev.source.map_scale = Some(4000);
    let err = fit(ev, &map, &FitOptions::default()).unwrap_err();
    assert_eq!(err, FitError::MapScaleUnknown { file: 4000 });
    assert!(!err.to_string().contains("1:0"), "{err}");
}
