//! Purple Pen `.ppen` writer from scratch (M3b Task 4).

use std::collections::{BTreeMap, BTreeSet};

use freemapper_course::model::{
    ControlKind, CourseFilter, Description, MAX_CONTROLS, MAX_POINTS_PER_PATH, NumberPlacement,
    RawPoint, Special, Stop,
};
use freemapper_course::{
    Control, Course, CourseEvent, CourseKind, CourseSource, DescriptionStandard, Leg, LineEnding,
    MapStandard, PpenMap, PpenWriteError, RawEvent, ScratchOptions, SourceFormat, WriteNote,
    read_ppen, write_ppen,
};

const FIXTURES: [&str; 3] = ["lincoln-park.ppen", "overlapper.ppen", "sampleevent8.ppen"];

fn fixture(name: &str) -> Vec<u8> {
    let path = format!(
        "{}/tests/fixtures/courses/ppen/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read(path).unwrap()
}

fn read(name: &str) -> RawEvent {
    read_ppen(&fixture(name)).unwrap()
}

/// The options each fixture's own `<event>` element states (no `<standards>`: map 2000, descriptions 2004).
fn options_for(name: &str) -> ScratchOptions {
    let (map, description_standard) = match name {
        "lincoln-park.ppen" => (
            PpenMap::Bitmap {
                path: "LincolnNov12.gif".into(),
                dpi: 300.0,
            },
            DescriptionStandard::Iof2004,
        ),
        "overlapper.ppen" => (PpenMap::None, DescriptionStandard::Iof2004),
        "sampleevent8.ppen" => (PpenMap::None, DescriptionStandard::Iof2018),
        other => panic!("no options for {other}"),
    };
    ScratchOptions {
        map,
        map_standard: MapStandard::Isom2000,
        description_standard,
        description_lang: "en".into(),
        line_ending: LineEnding::Crlf,
    }
}

fn write(name: &str, ev: &RawEvent) -> Result<String, PpenWriteError> {
    write_ppen(ev, &options_for(name)).map(|w| w.text)
}

fn f32p(p: &RawPoint) -> (f32, f32) {
    (p.x_mm as f32, p.y_mm as f32)
}

fn f32r(r: &[(f64, f64)]) -> Vec<(f32, f32)> {
    r.iter().map(|(a, b)| (*a as f32, *b as f32)).collect()
}

type StopKey = (
    String,
    Option<ControlKind>,
    Option<(f32, f32)>,
    Vec<(f32, f32)>,
    bool,
);

fn stop_key(s: &Stop) -> StopKey {
    assert!(s.number.is_none());
    (
        s.control.clone(),
        s.kind,
        s.number_text_at.map(|o| (o.dx_mm as f32, o.dy_mm as f32)),
        f32r(&s.circle_gaps),
        s.map_flip,
    )
}

type LegKey = (Vec<(f32, f32)>, Vec<(f32, f32)>, bool);

fn leg_key(l: &Leg<RawPoint>) -> LegKey {
    (
        l.bends.iter().map(f32p).collect(),
        f32r(&l.gaps),
        l.marked_route,
    )
}

fn special_key(s: &Special<RawPoint>) -> (String, Option<u32>, Vec<(f32, f32)>, CourseFilter) {
    let (kind, points): (&str, Vec<&RawPoint>) = match s {
        Special::OutOfBounds { rings, .. } => ("oob", rings.iter().flatten().collect()),
        Special::Dangerous { rings, .. } => ("dangerous", rings.iter().flatten().collect()),
        Special::WhiteOut { rings, .. } => ("white-out", rings.iter().flatten().collect()),
        Special::ForbiddenRoute { path, .. } => ("forbidden", path.iter().collect()),
        Special::Boundary { path, .. } => ("boundary", path.iter().collect()),
        Special::Line { path, .. } => ("line", path.iter().collect()),
        Special::MarkedRoute { path, .. } => ("marked", path.iter().collect()),
        Special::FirstAid { pt, .. } => ("first-aid", vec![pt]),
        Special::Water { pt, .. } => ("water", vec![pt]),
        Special::Text { pt, .. } => ("text", vec![pt]),
    };
    (
        kind.into(),
        s.id(),
        points.into_iter().map(f32p).collect(),
        s.courses().clone(),
    )
}

#[test]
fn round_trip_of_the_fixtures() {
    for name in FIXTURES {
        let ev = read(name);
        let text = write(name, &ev).unwrap_or_else(|e| panic!("{name}: {e}"));
        let back = read_ppen(text.as_bytes()).unwrap_or_else(|e| panic!("{name}: {e}\n{text}"));

        assert_eq!(back.source.map_scale, ev.source.map_scale, "{name}");
        assert_eq!(back.source.event_name, ev.source.event_name, "{name}");
        assert_eq!(back.controls.len(), ev.controls.len(), "{name}");
        for (b, a) in back.controls.iter().zip(&ev.controls) {
            assert_eq!(b.id, a.id, "{name} {}", a.code);
            assert_eq!(b.code, a.code, "{name}");
            assert_eq!(b.kind, a.kind, "{name} {}", a.code);
            assert_eq!(
                b.position.as_ref().map(f32p),
                a.position.as_ref().map(f32p),
                "{name} {}",
                a.code
            );
            assert_eq!(b.description, a.description, "{name} {}", a.code);
        }
        assert_eq!(back.courses.len(), ev.courses.len(), "{name}");
        for (b, a) in back.courses.iter().zip(&ev.courses) {
            let what = format!("{name} {}", a.name);
            assert_eq!(b.id, a.id, "{what}");
            assert_eq!(b.name, a.name, "{what}");
            assert_eq!(b.kind, a.kind, "{what}");
            assert_eq!(
                b.stops.iter().map(stop_key).collect::<Vec<_>>(),
                a.stops.iter().map(stop_key).collect::<Vec<_>>(),
                "{what}"
            );
            assert_eq!(
                b.legs.iter().map(leg_key).collect::<Vec<_>>(),
                a.legs.iter().map(leg_key).collect::<Vec<_>>(),
                "{what}"
            );
            assert_eq!(b.climb_m.map(|v| v as f32), a.climb_m.map(|v| v as f32));
            assert_eq!(b.length_m.map(|v| v as f32), a.length_m.map(|v| v as f32));
            assert_eq!(b.print_scale_hint, a.print_scale_hint, "{what}");
            assert_eq!(b.secondary_title, a.secondary_title, "{what}");
        }
        assert_eq!(
            back.specials.iter().map(special_key).collect::<Vec<_>>(),
            ev.specials.iter().map(special_key).collect::<Vec<_>>(),
            "{name}"
        );
    }
}

#[test]
fn ids_are_stable() {
    for name in FIXTURES {
        let ev = read(name);
        let a = write_ppen(&ev, &options_for(name)).unwrap();
        let b = write_ppen(&ev, &options_for(name)).unwrap();
        assert_eq!(a.text, b.text, "{name}");
        let back = read_ppen(a.text.as_bytes()).unwrap();
        let ids = |e: &RawEvent| -> (Vec<_>, Vec<_>, Vec<_>) {
            (
                e.controls.iter().map(|c| c.id).collect(),
                e.courses.iter().map(|c| c.id).collect(),
                e.specials.iter().map(|s| s.id()).collect(),
            )
        };
        assert_eq!(ids(&back), ids(&ev), "{name}");
        let given = ids(&ev);
        assert_eq!(
            a.ids.controls.iter().map(|i| Some(*i)).collect::<Vec<_>>(),
            given.0
        );
        assert_eq!(
            a.ids.courses.iter().map(|i| Some(*i)).collect::<Vec<_>>(),
            given.1
        );
        assert_eq!(
            a.ids.specials.iter().map(|i| Some(*i)).collect::<Vec<_>>(),
            given.2
        );
        assert!(a.kept.is_empty());
    }
    // A repeated id and a missing one get max + 1 of the store, in event order.
    let mut ev = read("lincoln-park.ppen");
    assert_eq!(
        ev.controls.iter().map(|c| c.id).collect::<Vec<_>>(),
        [Some(1), Some(2), Some(3), Some(5)]
    );
    ev.controls[1].id = Some(1);
    ev.controls[2].id = None;
    ev.courses[0].id = None;
    let w = write("lincoln-park.ppen", &ev).unwrap();
    let back = read_ppen(w.as_bytes()).unwrap();
    assert_eq!(
        back.controls.iter().map(|c| c.id).collect::<Vec<_>>(),
        [Some(1), Some(6), Some(7), Some(5)]
    );
    assert_eq!(back.courses[0].id, Some(1));
    let w = write_ppen(&ev, &options_for("lincoln-park.ppen")).unwrap();
    assert_eq!(w.ids.controls, [1, 6, 7, 5]);
    assert_eq!(w.ids.courses, [1]);
    // Course-controls 1.. in course order, legs 1.. in first-use order.
    assert!(w.text.contains("<course-control id=\"1\" control=\"1\">"));
    assert!(w.text.contains("<course-control id=\"4\" control=\"5\" />"));
    assert!(
        w.text
            .contains("<leg id=\"1\" start-control=\"7\" end-control=\"5\">")
    );
    assert!(
        w.text
            .contains("<course id=\"1\" kind=\"normal\" order=\"1\">")
    );
}

#[test]
fn leg_gap_written_as_start_and_length() {
    let ev = read("lincoln-park.ppen");
    let text = write("lincoln-park.ppen", &ev).unwrap();
    assert!(
        text.contains("<gap start=\"70.4939\" length=\"15.66552\" />"),
        "{text}"
    );
    let back = read_ppen(text.as_bytes()).unwrap();
    let gaps = &back.courses[0].legs[2].gaps;
    assert_eq!(gaps.len(), 1);
    assert!((gaps[0].0 - 70.4939).abs() < 1e-4, "{gaps:?}");
    assert!((gaps[0].1 - 86.15942).abs() < 1e-4, "{gaps:?}");
}

/// Lincoln Park with a second course "Long", a copy of "Short" changed by `change`.
fn with_second_course(change: impl FnOnce(&mut Course<RawPoint>)) -> RawEvent {
    let mut ev = read("lincoln-park.ppen");
    let mut long = ev.courses[0].clone();
    long.id = None;
    long.name = "Long".into();
    change(&mut long);
    ev.courses.push(long);
    ev
}

#[test]
fn shared_leg_with_different_bends_is_refused() {
    // The same course twice is fine.
    let ev = with_second_course(|_| {});
    write("lincoln-park.ppen", &ev).unwrap();
    let ev = with_second_course(|c| c.legs[2].bends[0].x_mm += 1.0);
    match write("lincoln-park.ppen", &ev) {
        Err(PpenWriteError::SharedLeg {
            from,
            to,
            first,
            second,
        }) => {
            assert_eq!((from.as_str(), to.as_str()), ("32", "FIN1"));
            assert_eq!((first.as_str(), second.as_str()), ("Short", "Long"));
        }
        other => panic!("{other:?}"),
    }
    // A leg without bends against one with them, and different flagging, are different too.
    let ev = with_second_course(|c| c.legs[2].bends.clear());
    assert!(matches!(
        write("lincoln-park.ppen", &ev),
        Err(PpenWriteError::SharedLeg { .. })
    ));
    let ev = with_second_course(|c| c.legs[0].marked_route = true);
    match write("lincoln-park.ppen", &ev) {
        Err(PpenWriteError::SharedLeg { from, to, .. }) => {
            assert_eq!((from.as_str(), to.as_str()), ("STA1", "31"));
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn shared_circle_gaps_differ_is_refused() {
    let ev = with_second_course(|c| c.stops[1].circle_gaps = vec![(10.0, 20.0)]);
    match write("lincoln-park.ppen", &ev) {
        Err(PpenWriteError::SharedCircleGaps {
            control,
            scale,
            first,
            second,
        }) => {
            assert_eq!(control, "31");
            assert_eq!(scale, 5000);
            assert_eq!((first.as_str(), second.as_str()), ("Short", "Long"));
        }
        other => panic!("{other:?}"),
    }
    // At another print scale the same control may have other gaps.
    let ev = with_second_course(|c| {
        c.print_scale_hint = Some(7500);
        c.stops[1].circle_gaps = vec![(10.0, 20.0)];
    });
    let text = write("lincoln-park.ppen", &ev).unwrap();
    assert!(text.contains("<circle-gaps scale=\"5000\">224.4232:330.1188</circle-gaps>"));
    assert!(text.contains("<circle-gaps scale=\"7500\">10:20</circle-gaps>"));
    let back = read_ppen(text.as_bytes()).unwrap();
    assert_eq!(back.courses[1].stops[1].circle_gaps, [(10.0, 20.0)]);
}

#[test]
fn description_standard_must_match_options() {
    let ev = read("lincoln-park.ppen");
    let mut options = options_for("lincoln-park.ppen");
    options.description_standard = DescriptionStandard::Iof2018;
    match write_ppen(&ev, &options) {
        Err(PpenWriteError::Invalid(m)) => assert!(m.contains("2004"), "{m}"),
        other => panic!("{other:?}"),
    }
}

/// The text of the top-level `<tag id="id" …>` element up to its end.
fn element<'a>(text: &'a str, tag: &str, id: u32) -> &'a str {
    let start = text
        .find(&format!("<{tag} id=\"{id}\""))
        .unwrap_or_else(|| panic!("no {tag} {id}"));
    let rest = &text[start..];
    let line_end = rest.find('\n').unwrap();
    if rest[..line_end].trim_end().ends_with("/>") {
        return &rest[..line_end];
    }
    let end = rest.find(&format!("</{tag}>")).unwrap();
    &rest[..end]
}

#[test]
fn start_and_finish_have_no_code() {
    let text = write("lincoln-park.ppen", &read("lincoln-park.ppen")).unwrap();
    assert_eq!(
        element(&text, "control", 1),
        "<control id=\"1\" kind=\"start\">\r\n    <location x=\"140.20718\" y=\"57.44655\" />\r\n  "
    );
    let finish = element(&text, "control", 5);
    assert!(finish.starts_with("<control id=\"5\" kind=\"finish\">"));
    assert!(!finish.contains("<code>"), "{finish}");
    assert!(element(&text, "control", 2).contains("<code>31</code>"));
}

#[test]
fn crossing_point_description_box_all() {
    let text = write("overlapper.ppen", &read("overlapper.ppen")).unwrap();
    let cross = element(&text, "control", 17);
    assert!(cross.starts_with("<control id=\"17\" kind=\"crossing-point\""));
    assert!(cross.contains("<description box=\"all\" iof-2004-ref=\"13.3\" />"));
    let finish = element(&text, "control", 11);
    assert!(finish.contains("<description box=\"all\" iof-2004-ref=\"14.3\" />"));
    // Other controls name their boxes; column F text is the text of box F.
    let text = write("sampleevent8.ppen", &read("sampleevent8.ppen")).unwrap();
    let c59 = element(&text, "control", 59);
    assert!(
        c59.contains("<description box=\"F\">5x5|3</description>"),
        "{c59}"
    );
    assert!(c59.contains("<description box=\"H\" iof-2004-ref=\"12.4\" />"));
    // A finish with a second column cannot be written.
    let mut ev = read("overlapper.ppen");
    let finish = ev.controls.iter_mut().find(|c| c.code == "FIN1").unwrap();
    finish.description.as_mut().unwrap().columns[1] = Some("1.1".into());
    assert!(matches!(
        write("overlapper.ppen", &ev),
        Err(PpenWriteError::Unsupported(_))
    ));
}

#[test]
fn map_exchange_and_flip_at_a_control() {
    let ev = read("sampleevent8.ppen");
    let text = write("sampleevent8.ppen", &ev).unwrap();
    // Exchanges: 1, 59, 53, 41 (exchange), 72, 47, 81, 71, 77, 78, 43, 75 (exchange and flip), …
    assert!(
        text.contains("<course-control id=\"4\" control=\"41\" map-exchange=\"true\">"),
        "{text}"
    );
    assert!(text.contains(
        "<course-control id=\"12\" control=\"75\" map-exchange=\"true\" map-flip=\"true\">"
    ));
    // Purple Pen counts course parts from the flag alone, so a map-exchange control carries it too
    // (`CourseControl.Validate`, sampleevent8 line 292).
    assert!(text.contains("<course-control id=\"7\" control=\"81\" map-exchange=\"true\">"));
    let back = read_ppen(text.as_bytes()).unwrap();
    let stops = &back.courses[0].stops;
    assert_eq!(stops[3].kind, Some(ControlKind::MapExchange));
    assert!(!stops[3].map_flip);
    assert_eq!(stops[11].kind, Some(ControlKind::MapExchange));
    assert!(stops[11].map_flip);
    // A flip without a map exchange would be lost.
    let mut ev = read("sampleevent8.ppen");
    ev.courses[0].stops[4].map_flip = true;
    assert!(matches!(
        write("sampleevent8.ppen", &ev),
        Err(PpenWriteError::Unsupported(_))
    ));
    // A flip at a map-exchange control is one Purple Pen allows.
    let mut ev = read("sampleevent8.ppen");
    assert_eq!(ev.courses[0].stops[6].control, "XCHG1");
    ev.courses[0].stops[6].map_flip = true;
    let text = write("sampleevent8.ppen", &ev).unwrap();
    assert!(text.contains(
        "<course-control id=\"7\" control=\"81\" map-exchange=\"true\" map-flip=\"true\">"
    ));
    assert!(read_ppen(text.as_bytes()).unwrap().courses[0].stops[6].map_flip);
}

#[test]
fn crossing_point_has_an_orientation() {
    // `ControlPoint.ReadAttributesAndContent` requires `orientation` on a crossing point.
    let text = write("overlapper.ppen", &read("overlapper.ppen")).unwrap();
    assert!(
        element(&text, "control", 17)
            .starts_with("<control id=\"17\" kind=\"crossing-point\" orientation=\"0\">")
    );
}

#[test]
fn options_always_carry_the_print_scale() {
    // `Course.ReadAttributesAndContent` requires `options/@print-scale` and `options/@description-kind`.
    let mut ev = read("lincoln-park.ppen");
    ev.courses[0].print_scale_hint = None;
    ev.courses[0].climb_m = Some(20.0);
    let text = write("lincoln-park.ppen", &ev).unwrap();
    assert!(
        text.contains(
            "<options print-scale=\"15000\" climb=\"20\" description-kind=\"symbols\" />"
        ),
        "{text}"
    );
    ev.courses[0].climb_m = None;
    let text = write("lincoln-park.ppen", &ev).unwrap();
    assert!(!text.contains("<options"), "{text}");
}

#[test]
fn descriptions_fit_the_control_kind() {
    // Column F text only at normal controls and starts (`ControlPoint.Validate`).
    let mut ev = read("sampleevent8.ppen");
    let map = ev.controls.iter_mut().find(|c| c.code == "XCHG1").unwrap();
    assert_eq!(map.kind, ControlKind::MapExchange);
    map.description.get_or_insert_with(|| Description {
        columns: Default::default(),
        standard: "2018".into(),
        column_f_text: None,
    });
    map.description.as_mut().unwrap().column_f_text = Some("2x2".into());
    match write("sampleevent8.ppen", &ev) {
        Err(PpenWriteError::Unsupported(m)) => assert!(m.contains("column F"), "{m}"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn specials_purple_pen_cannot_draw_are_refused() {
    let p = |x: f64| RawPoint { x_mm: x, y_mm: 1.0 };
    let any = CourseFilter::All;
    let one = vec![p(1.0)];
    let cases = [
        Special::Boundary {
            id: None,
            path: one.clone(),
            courses: any.clone(),
        },
        Special::Line {
            id: None,
            path: one.clone(),
            courses: any.clone(),
        },
        Special::OutOfBounds {
            id: None,
            rings: vec![one.clone()],
            courses: any.clone(),
        },
        Special::Dangerous {
            id: None,
            rings: vec![one.clone()],
            courses: any.clone(),
        },
        Special::WhiteOut {
            id: None,
            rings: vec![one.clone()],
            courses: any.clone(),
        },
    ];
    for s in cases {
        let mut ev = read("lincoln-park.ppen");
        ev.specials.push(s.clone());
        match write("lincoln-park.ppen", &ev) {
            Err(PpenWriteError::Invalid(m)) => assert!(m.contains("2 points"), "{m}"),
            other => panic!("{s:?}: {other:?}"),
        }
    }
    // A line gets the appearance Purple Pen gives a new line (`NormalCourseAppearance.lineSpecial*`).
    let mut ev = read("lincoln-park.ppen");
    ev.specials.push(Special::Line {
        id: Some(9),
        path: vec![p(1.0), p(2.0)],
        courses: any,
    });
    let text = write("lincoln-park.ppen", &ev).unwrap();
    assert!(
        text.contains(
            "<special-object id=\"9\" kind=\"line\">\r\n    \
             <appearance line-kind=\"single\" color=\"purple\" line-width=\"0.35\" />\r\n    \
             <location x=\"1\" y=\"1\" />"
        ),
        "{text}"
    );
    assert_eq!(read_ppen(text.as_bytes()).unwrap().specials, ev.specials);
}

#[test]
fn empty_language_and_map_path_are_refused() {
    let ev = read("overlapper.ppen");
    let mut options = options_for("overlapper.ppen");
    options.description_lang = String::new();
    match write_ppen(&ev, &options) {
        Err(PpenWriteError::Invalid(m)) => assert!(m.contains("language"), "{m}"),
        other => panic!("{:?}", other.map(|w| w.text)),
    }
    let mut options = options_for("overlapper.ppen");
    options.map = PpenMap::Ocad {
        path: String::new(),
    };
    match write_ppen(&ev, &options) {
        Err(PpenWriteError::Invalid(m)) => assert!(m.contains("map path"), "{m}"),
        other => panic!("{:?}", other.map(|w| w.text)),
    }
}

/// What a top-level object looks like: its attribute names and, per child element name, the attribute
/// names of those children.
type Shape = (BTreeSet<String>, BTreeMap<String, BTreeSet<String>>);

/// The shape of every object of a `.ppen`, by a key that does not depend on the ids the writer renumbers:
/// controls, courses and special objects by id, course-controls by course id and position in the course,
/// legs by their pair of control ids. The `<event>` element is left out.
fn shapes(text: &str) -> BTreeMap<String, Shape> {
    use quick_xml::Reader;
    use quick_xml::events::Event;
    struct Obj {
        name: String,
        attrs: BTreeMap<String, String>,
        shape: Shape,
        first: Option<String>,
        next: Option<String>,
    }
    let mut reader = Reader::from_str(text);
    let mut depth = 0;
    let mut objs: Vec<Obj> = Vec::new();
    loop {
        let event = reader.read_event().unwrap();
        let (e, empty) = match &event {
            Event::Start(e) => (e, false),
            Event::Empty(e) => (e, true),
            Event::End(_) => {
                depth -= 1;
                continue;
            }
            Event::Eof => break,
            _ => continue,
        };
        let name = e.name().as_ref().to_string();
        let attrs: BTreeMap<String, String> = e
            .attributes()
            .map(|a| {
                let a = a.unwrap();
                (a.key.as_ref().to_string(), a.value.to_string())
            })
            .collect();
        match depth {
            1 if name != "event" => objs.push(Obj {
                name,
                shape: (attrs.keys().cloned().collect(), BTreeMap::new()),
                attrs,
                first: None,
                next: None,
            }),
            2 if !objs.is_empty() => {
                let o = objs.last_mut().unwrap();
                if name == "first" {
                    o.first = attrs.get("course-control").cloned();
                }
                if name == "next" {
                    o.next = attrs.get("course-control").cloned();
                }
                o.shape.1.entry(name).or_default().extend(attrs.into_keys());
            }
            _ => {}
        }
        if !empty {
            depth += 1;
        }
    }
    let ccs: BTreeMap<&str, &Obj> = objs
        .iter()
        .filter(|o| o.name == "course-control")
        .map(|o| (o.attrs["id"].as_str(), o))
        .collect();
    let mut out = BTreeMap::new();
    for o in &objs {
        match o.name.as_str() {
            "course" => {
                let mut next = o.first.as_deref();
                let mut i = 0;
                while let Some(cc) = next.and_then(|id| ccs.get(id)) {
                    out.insert(
                        format!("course-control {}/{i}", o.attrs["id"]),
                        cc.shape.clone(),
                    );
                    i += 1;
                    next = cc.next.as_deref();
                }
                out.insert(format!("course {}", o.attrs["id"]), o.shape.clone());
            }
            "course-control" => {}
            "leg" => {
                out.insert(
                    format!(
                        "leg {}-{}",
                        o.attrs["start-control"], o.attrs["end-control"]
                    ),
                    o.shape.clone(),
                );
            }
            other => {
                out.insert(format!("{other} {}", o.attrs["id"]), o.shape.clone());
            }
        }
    }
    out
}

/// Written from scratch, every object has the attributes and child elements Purple Pen gave it in the
/// fixture, as far as the writer owns them.
#[test]
fn shape_of_the_fixtures_written_from_scratch() {
    // What the fixtures have and the writer does not own (the model does not carry it), as
    // (object kind, child element or "" for the object's own attributes, name). Purple Pen reads every one
    // of them with a default. (`options/@description-kind` has none: the writer always writes it, see
    // `purple_pen_strict.rs`.)
    const NOT_OWNED: &[(&str, &str, &str)] = &[
        // `labels`, `print-area`: course labels and print area, written by Purple Pen with defaults.
        ("course", "", "labels"),
        ("course", "", "print-area"),
        // `hide-from-reports`, `load`.
        ("course", "options", "hide-from-reports"),
        ("course", "options", "load"),
        // `gaps`: the bit-string copy of each `circle-gaps` that Purple Pen writes for 1.x; on load
        // `circle-gaps` wins over it.
        ("control", "", "gaps"),
        // `punch-pattern`: the punch of the control; optional.
        ("control", "", "punch-pattern"),
    ];
    // The descriptions special (sampleevent8 id 3) is a box the reader does not read, so it is not written.
    const NOT_READ: &[&str] = &["special-object 3"];
    for name in FIXTURES {
        let fixture = fixture(name);
        let fixture = String::from_utf8(fixture).unwrap();
        let written = write(name, &read(name)).unwrap();
        let (want, got) = (shapes(&fixture), shapes(&written));
        for (key, (attrs, kids)) in &want {
            if NOT_READ.contains(&key.as_str()) {
                assert!(!got.contains_key(key), "{name}: {key}");
                continue;
            }
            let kind = key.split(' ').next().unwrap();
            let owned = |child: &str, n: &str| !NOT_OWNED.contains(&(kind, child, n));
            let (got_attrs, got_kids) = got
                .get(key)
                .unwrap_or_else(|| panic!("{name}: {key} not written"));
            let want_attrs: BTreeSet<&String> = attrs.iter().filter(|a| owned("", a)).collect();
            assert_eq!(
                got_attrs.iter().collect::<BTreeSet<_>>(),
                want_attrs,
                "{name}: attributes of {key}"
            );
            // A control with circle gaps in the old form only (overlapper) gets them as `circle-gaps`.
            let mut kids = kids.clone();
            if kind == "control"
                && !kids.contains_key("circle-gaps")
                && let Some(a) = kids.get("gaps").cloned()
            {
                kids.insert("circle-gaps".into(), a);
            }
            let want_kids: BTreeMap<&String, BTreeSet<&String>> = kids
                .iter()
                .filter(|(k, _)| owned("", k))
                .map(|(k, a)| (k, a.iter().filter(|a| owned(k, a)).collect()))
                .collect();
            let got_kids: BTreeMap<&String, BTreeSet<&String>> = got_kids
                .iter()
                .map(|(k, a)| (k, a.iter().collect()))
                .collect();
            assert_eq!(got_kids, want_kids, "{name}: children of {key}");
        }
        for key in got.keys() {
            assert!(
                want.contains_key(key),
                "{name}: {key} is not in the fixture"
            );
        }
    }
}

#[test]
fn unsupported_content_is_refused() {
    let base = || read("lincoln-park.ppen");
    let any = CourseFilter::All;
    let p = RawPoint {
        x_mm: 1.0,
        y_mm: 2.0,
    };
    let mut cases: Vec<(&str, RawEvent)> = Vec::new();
    let mut ev = base();
    ev.courses[0].stops[1].number = Some(NumberPlacement {
        direction: 0.0,
        gap_mm: 1.0,
    });
    cases.push(("number placement", ev));
    let mut ev = base();
    ev.courses[0].family = Some("A".into());
    cases.push(("family", ev));
    let mut ev = base();
    ev.controls[1].kind = ControlKind::EndOfMarkedRoute;
    cases.push(("end of marked route", ev));
    let mut ev = base();
    ev.specials.push(Special::Text {
        id: None,
        pt: p,
        text: "x".into(),
        size_mm: 3.0,
        courses: any.clone(),
    });
    cases.push(("text", ev));
    let mut ev = base();
    ev.specials.push(Special::MarkedRoute {
        id: None,
        path: vec![p, p],
        courses: any.clone(),
    });
    cases.push(("marked route", ev));
    let mut ev = base();
    ev.specials.push(Special::ForbiddenRoute {
        id: None,
        path: vec![p, p],
        courses: any.clone(),
    });
    cases.push(("two-point forbidden route", ev));
    let mut ev = base();
    ev.specials.push(Special::OutOfBounds {
        id: None,
        rings: vec![vec![p, p, p], vec![p, p, p]],
        courses: any.clone(),
    });
    cases.push(("two rings", ev));
    for (what, ev) in cases {
        match write("lincoln-park.ppen", &ev) {
            Err(PpenWriteError::Unsupported(_)) => {}
            other => panic!("{what}: {other:?}"),
        }
    }
}

#[test]
fn specials_and_their_course_lists() {
    let p = |x: f64| RawPoint { x_mm: x, y_mm: 1.0 };
    let mut ev = with_second_course(|_| {});
    ev.specials = vec![
        Special::FirstAid {
            id: Some(4),
            pt: p(1.0),
            courses: CourseFilter::Only(vec!["Long".into()]),
        },
        Special::Boundary {
            id: None,
            path: vec![p(1.0), p(2.0)],
            courses: CourseFilter::Only(vec!["Short".into(), "Long".into()]),
        },
        Special::WhiteOut {
            id: Some(4),
            rings: vec![vec![p(1.0), p(2.0), p(3.0)]],
            courses: CourseFilter::All,
        },
        Special::Dangerous {
            id: None,
            rings: vec![vec![p(1.0), p(2.0), p(3.0)]],
            courses: CourseFilter::Only(vec![]),
        },
    ];
    let w = write_ppen(&ev, &options_for("lincoln-park.ppen")).unwrap();
    assert_eq!(w.ids.specials, [4, 5, 6, 7]);
    assert_eq!(w.ids.courses, [1, 2]);
    assert!(w.text.contains(
        "<special-object id=\"4\" kind=\"first-aid\">\r\n    <location x=\"1\" y=\"1\" />\r\n    \
         <courses>\r\n      <course course=\"2\" />\r\n    </courses>\r\n  </special-object>"
    ));
    let back = read_ppen(w.text.as_bytes()).unwrap();
    let mut want = ev.specials.clone();
    for (s, id) in want.iter_mut().zip([4, 5, 6, 7]) {
        match s {
            Special::FirstAid { id: i, .. }
            | Special::Boundary { id: i, .. }
            | Special::WhiteOut { id: i, .. }
            | Special::Dangerous { id: i, .. } => *i = Some(id),
            _ => unreachable!(),
        }
    }
    assert_eq!(back.specials, want);
    // A name that matches no course, or two, is refused.
    for name in ["Nope", "Short"] {
        let mut ev = ev.clone();
        if name == "Short" {
            ev.courses[1].name = "Short".into();
        }
        ev.specials[0] = Special::Water {
            id: None,
            pt: p(1.0),
            courses: CourseFilter::Only(vec![name.into()]),
        };
        match write("lincoln-park.ppen", &ev) {
            Err(PpenWriteError::Invalid(m)) => assert!(m.contains(name), "{m}"),
            other => panic!("{name}: {other:?}"),
        }
    }
}

#[test]
fn flagging_written_unless_implied() {
    let mut ev = read("lincoln-park.ppen");
    ev.courses[0].legs[1].marked_route = true;
    let w = write_ppen(&ev, &options_for("lincoln-park.ppen")).unwrap();
    assert!(
        w.text.contains(
            "<leg id=\"1\" start-control=\"2\" end-control=\"3\">\r\n    <flagging kind=\"all\" />\r\n  </leg>"
        ),
        "{}",
        w.text
    );
    assert!(w.notes.is_empty());
    let back = read_ppen(w.text.as_bytes()).unwrap();
    assert!(back.courses[0].legs[1].marked_route);
    // Into a 14.1 finish Purple Pen flags the leg on its own; an unflagged leg there is noted.
    let mut ev = read("lincoln-park.ppen");
    ev.controls[3].description.as_mut().unwrap().columns[0] = Some("14.1".into());
    let w = write_ppen(&ev, &options_for("lincoln-park.ppen")).unwrap();
    assert!(!w.text.contains("<flagging"));
    assert_eq!(
        w.notes,
        [WriteNote::FlaggingImplied {
            course: "Short".into(),
            from: "32".into(),
            to: "FIN1".into()
        }]
    );
}

#[test]
fn limits_are_checked_first() {
    // Without positions the event is also invalid; the limit comes first.
    let control = |i: usize| Control {
        id: None,
        code: i.to_string(),
        kind: ControlKind::Control,
        position: None,
        geo: None,
        description: None,
    };
    let source = CourseSource {
        format: SourceFormat::PurplePen,
        event_name: None,
        map_scale: None,
        item_scaling: None,
    };
    let ev: RawEvent = CourseEvent {
        controls: (0..=MAX_CONTROLS).map(control).collect(),
        courses: vec![],
        specials: vec![],
        source: source.clone(),
        notes: vec![],
    };
    let options = options_for("overlapper.ppen");
    match write_ppen(&ev, &options) {
        Err(PpenWriteError::Limit(_)) => {}
        other => panic!("{:?}", other.map(|w| w.text.len())),
    }
    // One leg with the most bends, shared by 300 courses: 3 million points, over the total.
    let p = RawPoint::default();
    let leg = Leg {
        bends: vec![p; MAX_POINTS_PER_PATH],
        gaps: vec![],
        marked_route: false,
    };
    let stop = |code: &str| Stop {
        control: code.into(),
        kind: None,
        number: None,
        number_text_at: None,
        circle_gaps: vec![],
        map_flip: false,
    };
    let courses = (0..300)
        .map(|i| Course {
            id: None,
            name: i.to_string(),
            family: None,
            kind: CourseKind::Normal,
            legs: vec![leg.clone()],
            stops: vec![stop("1"), stop("2")],
            climb_m: None,
            length_m: None,
            print_scale_hint: None,
            course_obj_ratio: None,
            secondary_title: None,
        })
        .collect();
    let ev: RawEvent = CourseEvent {
        controls: (1..=2).map(control).collect(),
        courses,
        specials: vec![],
        source,
        notes: vec![],
    };
    match write_ppen(&ev, &options) {
        Err(PpenWriteError::Limit(m)) => assert!(m.contains("points"), "{m}"),
        other => panic!("{:?}", other.map(|w| w.text.len())),
    }
}

#[test]
fn no_clock_and_no_map_paths_beyond_the_given_one() {
    let ev = read("lincoln-park.ppen");
    let a = write("lincoln-park.ppen", &ev).unwrap();
    let b = write("lincoln-park.ppen", &ev).unwrap();
    assert_eq!(a, b);
    assert!(!a.contains("absolute-path"));
    assert!(a.starts_with(
        "\u{FEFF}<course-scribe-event>\r\n  <event id=\"1\">\r\n    <title>Lincoln Park</title>\r\n    \
         <map kind=\"bitmap\" scale=\"5000\" dpi=\"300\">LincolnNov12.gif</map>\r\n    \
         <standards map=\"2000\" description=\"2004\" />\r\n    \
         <all-controls print-scale=\"5000\" description-kind=\"symbols\" />\r\n    \
         <course-appearance />\r\n    <descriptions lang=\"en\" />\r\n  </event>\r\n  <control id=\"1\""
    ));
    assert!(a.ends_with("  </leg>\r\n</course-scribe-event>"), "{a}");
    // Store order as EventDB.Save writes it.
    let at = |s: &str| a.find(s).unwrap();
    assert!(at("<control id=\"5\"") < at("<course id=\"1\""));
    assert!(at("<course id=\"1\"") < at("<course-control id=\"1\""));
    assert!(at("<course-control id=\"4\"") < at("<leg id=\"1\""));
    // OCAD and PDF maps, the other standards, LF.
    let mut options = options_for("overlapper.ppen");
    options.map = PpenMap::Ocad {
        path: "C:\\maps\\a & b.ocd".into(),
    };
    options.map_standard = MapStandard::Issprom2019;
    options.description_lang = "de".into();
    options.line_ending = LineEnding::Lf;
    let ev = read("overlapper.ppen");
    let text = write_ppen(&ev, &options).unwrap().text;
    assert!(!text.contains('\r'));
    assert!(text.contains(
        "<map kind=\"OCAD\" scale=\"10000\" ignore-missing-fonts=\"false\">C:\\maps\\a &amp; b.ocd</map>\n"
    ));
    assert!(text.contains("<standards map=\"Spr2019\" description=\"2004\" />"));
    assert!(text.contains("<descriptions lang=\"de\" />"));
    options.map = PpenMap::Pdf {
        path: "a.pdf".into(),
    };
    options.map_standard = MapStandard::Isom2017;
    let text = write_ppen(&ev, &options).unwrap().text;
    assert!(text.contains("<map kind=\"PDF\" scale=\"10000\">a.pdf</map>"));
    assert!(text.contains("<standards map=\"2017\""));
    options.map = PpenMap::None;
    let text = write_ppen(&ev, &options).unwrap().text;
    assert!(text.contains("<map kind=\"none\" scale=\"10000\" />"));
    assert!(!text.contains("<title>") || ev.source.event_name.is_some());
    // No title without an event name; the map scale is required.
    let mut ev = ev;
    ev.source.event_name = None;
    assert!(!write_ppen(&ev, &options).unwrap().text.contains("<title>"));
    ev.source.map_scale = None;
    assert!(matches!(
        write_ppen(&ev, &options),
        Err(PpenWriteError::Invalid(_))
    ));
}

#[test]
fn invalid_content_is_refused() {
    let base = || read("lincoln-park.ppen");
    let mut cases: Vec<(&str, RawEvent)> = Vec::new();
    let mut ev = base();
    ev.controls[1].position = None;
    cases.push(("no location", ev));
    let mut ev = base();
    ev.courses[0].stops[1].control = "99".into();
    cases.push(("unknown code", ev));
    let mut ev = base();
    ev.controls[2].code = "31".into();
    cases.push(("code twice", ev));
    let mut ev = base();
    ev.controls[1].position = Some(RawPoint {
        x_mm: f64::NAN,
        y_mm: 0.0,
    });
    cases.push(("NaN", ev));
    let mut ev = base();
    ev.courses[0].name = "a\u{1}".into();
    cases.push(("control character", ev));
    for (what, ev) in cases {
        match write("lincoln-park.ppen", &ev) {
            Err(PpenWriteError::Invalid(_)) => {}
            other => panic!("{what}: {other:?}"),
        }
    }
}
