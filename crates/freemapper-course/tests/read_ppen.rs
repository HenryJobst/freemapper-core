//! Purple Pen `.ppen` reader (M3 Task 4).

use freemapper_course::model::{
    ControlKind, CourseFilter, CourseKind, CourseNote, MAX_CODE_LEN, MAX_CONTROLS,
    MAX_COURSE_FILE_BYTES, MAX_COURSES, MAX_NAME_LEN, MAX_NOTES, MAX_POINTS_PER_PATH, MAX_SPECIALS,
    MAX_STANDARD_LEN, MAX_STOPS_PER_COURSE, MAX_TOTAL_POINTS, MAX_TOTAL_TEXT_BYTES, MAX_XML_DEPTH,
    Offset, RawPoint, SourceFormat, Special,
};
use freemapper_course::{CourseError, ItemScaling, RawEvent, course_obj_ratio, read_ppen};

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

/// A minimal event around `body`.
fn ppen(body: &str) -> Vec<u8> {
    format!(
        "<course-scribe-event><event id=\"1\"><title>T</title>\
         <map kind=\"none\" scale=\"10000\"></map></event>{body}</course-scribe-event>"
    )
    .into_bytes()
}

/// Two controls and one course from start to control 31.
const TWO: &str = "<control id=\"1\" kind=\"start\"><location x=\"0\" y=\"0\"/></control>\
     <control id=\"2\" kind=\"normal\"><code>31</code><location x=\"10\" y=\"5\"/></control>\
     <course id=\"1\" kind=\"normal\" order=\"1\"><name>A</name><first course-control=\"1\"/>\
     <options print-scale=\"10000\" description-kind=\"symbols\"/></course>\
     <course id=\"2\" kind=\"normal\" order=\"2\"><name>B</name></course>\
     <course-control id=\"1\" control=\"1\"><next course-control=\"2\"/></course-control>\
     <course-control id=\"2\" control=\"2\"/>";

fn codes(ev: &RawEvent, course: usize) -> Vec<&str> {
    ev.courses[course]
        .stops
        .iter()
        .map(|s| s.control.as_str())
        .collect()
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-4
}

fn gaps_near(got: &[(f64, f64)], want: &[(f64, f64)]) -> bool {
    got.len() == want.len()
        && got
            .iter()
            .zip(want)
            .all(|(g, w)| near(g.0, w.0) && near(g.1, w.1))
}

#[test]
fn reads_sample_event() {
    let ev = read("sampleevent8.ppen");
    assert_eq!(ev.source.format, SourceFormat::PurplePen);
    assert_eq!(ev.source.event_name.as_deref(), Some("Marymoor WIOL 2"));
    assert_eq!(ev.source.map_scale, Some(10000));
    let codes_all: Vec<_> = ev.controls.iter().map(|c| c.code.as_str()).collect();
    assert_eq!(
        codes_all,
        [
            "STA1", "FIN1", "38", "41", "43", "45", "47", "53", "59", "71", "72", "75", "77", "78",
            "80", "XCHG1"
        ]
    );
    assert_eq!(ev.controls[0].kind, ControlKind::Start);
    assert_eq!(ev.controls[1].kind, ControlKind::Finish);
    assert_eq!(ev.controls[2].kind, ControlKind::Control);
    assert_eq!(ev.controls[15].kind, ControlKind::MapExchange);
    assert_eq!(
        ev.controls[2].position,
        Some(RawPoint {
            x_mm: 50.3,
            y_mm: 2.9
        })
    );
    assert_eq!(ev.courses.len(), 1);
    let c = &ev.courses[0];
    assert_eq!(c.name, "Exchanges");
    assert_eq!(c.kind, CourseKind::Normal);
    assert_eq!(c.print_scale_hint, Some(10000));
    assert_eq!(
        codes(&ev, 0),
        [
            "STA1", "59", "53", "41", "72", "47", "XCHG1", "71", "77", "78", "43", "75", "45",
            "80", "38", "FIN1"
        ]
    );
    // Map exchanges at normal controls are a per-course kind (R8); the map-exchange control is one itself.
    let kinds: Vec<_> = c.stops.iter().map(|s| s.kind).collect();
    let mut want = vec![None; 16];
    want[3] = Some(ControlKind::MapExchange);
    want[11] = Some(ControlKind::MapExchange);
    assert_eq!(kinds, want);
    // Purple Pen flags the leg to a map-exchange control on its own.
    assert_eq!(c.legs.len(), 15);
    let marked: Vec<_> = c.legs.iter().map(|l| l.marked_route).collect();
    let mut want = vec![false; 15];
    want[5] = true;
    assert_eq!(marked, want);
    assert_eq!(
        ev.specials,
        vec![Special::Water {
            id: Some(4),
            pt: RawPoint {
                x_mm: 72.67218,
                y_mm: 10.4624271
            },
            courses: CourseFilter::All
        }]
    );
    assert!(ev.notes.contains(&CourseNote::UnknownElement(
        "special:descriptions ignored (layout decides)".into()
    )));
    assert!(!ev.notes.contains(&CourseNote::NoDescriptions));
}

#[test]
fn reads_legs_bends_gaps_and_circle_gaps() {
    let ev = read("lincoln-park.ppen");
    assert_eq!(ev.source.event_name.as_deref(), Some("Lincoln Park"));
    assert_eq!(ev.source.map_scale, Some(5000));
    assert_eq!(codes(&ev, 0), ["STA1", "31", "32", "FIN1"]);
    let c = &ev.courses[0];
    assert_eq!(c.name, "Short");
    assert_eq!(c.print_scale_hint, Some(5000));
    assert_eq!(c.legs.len(), 3);
    assert!(c.legs[0].bends.is_empty() && c.legs[0].gaps.is_empty());
    assert_eq!(
        c.legs[2].bends,
        vec![RawPoint {
            x_mm: 81.8017349,
            y_mm: 67.87238
        }]
    );
    assert!(gaps_near(&c.legs[2].gaps, &[(70.4939, 70.4939 + 15.66552)]));
    assert!(c.stops[0].circle_gaps.is_empty());
    // The angle form wins over the bit string of the same scale.
    assert!(gaps_near(&c.stops[1].circle_gaps, &[(224.4232, 330.1188)]));
    assert!(gaps_near(
        &c.stops[2].circle_gaps,
        &[(-32.6925049, 87.77142)]
    ));
    assert!(gaps_near(
        &c.stops[3].circle_gaps,
        &[(126.475586, 231.030273)]
    ));
}

#[test]
fn old_circle_gaps_follow_the_course_print_scale() {
    let ev = read("overlapper.ppen");
    let names: Vec<_> = ev.courses.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["Course 1", "Course 2", "Score"]);
    let gaps = |course: usize, code: &str| {
        let c = &ev.courses[course];
        let s = c.stops.iter().find(|s| s.control == code).unwrap();
        s.circle_gaps.clone()
    };
    // Control 31 has gaps for 1:7500 only: bits 29-31 clear, an arc ending at east.
    assert!(gaps_near(&gaps(0, "31"), &[(-33.75, 0.0)]));
    assert!(gaps(2, "31").is_empty());
    // Control 35 has gaps for 1:10000 only: bits 14-16 clear.
    assert!(gaps(0, "35").is_empty());
    assert!(gaps_near(&gaps(1, "35"), &[(157.5, 191.25)]));
}

#[test]
fn number_location_kept_as_offset() {
    let ev = read("overlapper.ppen");
    let c = &ev.courses[0];
    let s = c.stops.iter().find(|s| s.control == "38").unwrap();
    assert_eq!(
        s.number_text_at,
        Some(Offset {
            dx_mm: 4.2800827,
            dy_mm: -3.3762958
        })
    );
    assert_eq!(s.number, None);
    assert!(
        c.stops
            .iter()
            .filter(|s| s.control != "38")
            .all(|s| s.number_text_at.is_none())
    );
    // Course 2 visits 38 too, without its own number location.
    let s2 = ev.courses[1].stops.iter().find(|s| s.control == "37");
    assert!(s2.unwrap().number_text_at.is_none());
}

#[test]
fn score_course_read() {
    let ev = read("overlapper.ppen");
    assert_eq!(ev.controls.len(), 18);
    assert_eq!(ev.controls[16].code, "CROSS1");
    assert_eq!(ev.controls[16].kind, ControlKind::CrossingPoint);
    assert_eq!(ev.courses[0].kind, CourseKind::Normal);
    assert_eq!(
        codes(&ev, 0),
        [
            "STA1", "31", "44", "CROSS1", "32", "42", "33", "34", "41", "35", "36", "37", "38",
            "39", "FIN1"
        ]
    );
    assert_eq!(
        codes(&ev, 1),
        [
            "STA1", "45", "32", "33", "34", "41", "35", "36", "37", "45", "39", "FIN1"
        ]
    );
    let score = &ev.courses[2];
    assert_eq!(score.kind, CourseKind::Score);
    assert_eq!(
        codes(&ev, 2),
        [
            "STA1", "32", "42", "33", "34", "31", "37", "36", "35", "41", "43", "FIN1"
        ]
    );
    assert_eq!(score.legs.len(), score.stops.len() - 1);
    assert_eq!(score.print_scale_hint, Some(10000));
    assert_eq!(ev.courses[0].print_scale_hint, Some(7500));
    // Forbidden-route crosses are one-point paths.
    assert_eq!(ev.specials.len(), 3);
    assert!(
        matches!(&ev.specials[1], Special::ForbiddenRoute { path, courses: CourseFilter::All, .. } if path.len() == 1)
    );
}

#[test]
fn descriptions_read_per_column() {
    let ev = read("sampleevent8.ppen");
    let d = |code: &str| {
        ev.controls
            .iter()
            .find(|c| c.code == code)
            .unwrap()
            .description
            .clone()
    };
    let s = |v: &str| Some(v.to_string());
    let c47 = d("47").unwrap();
    assert_eq!(
        c47.columns,
        [
            s("0.1N"),
            s("6.2"),
            s("3.3"),
            s("10.2"),
            s("11.3E"),
            s("12.1")
        ]
    );
    assert_eq!(c47.standard, "2018");
    assert_eq!(c47.column_f_text, None);
    let c59 = d("59").unwrap();
    assert_eq!(
        c59.columns,
        [None, s("3.8"), s("1.13"), None, s("11.15"), s("12.4")]
    );
    assert_eq!(c59.column_f_text.as_deref(), Some("5x5|3"));
    // A finish has one symbol for the whole row (box "all"), kept in column C.
    assert_eq!(d("FIN1").unwrap().columns[0], s("14.3"));
    assert_eq!(d("STA1").unwrap().columns[1], s("4.4"));
    assert_eq!(d("XCHG1"), None);
    // Without a <standards> element the standard is Purple Pen's default.
    let lp = read("lincoln-park.ppen");
    assert_eq!(
        lp.controls[2].description.as_ref().unwrap().standard,
        "2004"
    );
}

#[test]
fn no_descriptions_noted() {
    let ev = read_ppen(&ppen(TWO)).unwrap();
    assert!(ev.notes.contains(&CourseNote::NoDescriptions));
    assert_eq!(codes(&ev, 0), ["STA1", "31"]);
    assert!(ev.courses[1].stops.is_empty());
}

#[test]
fn specials_read_with_course_filter() {
    let loc = |x: f64, y: f64| format!("<location x=\"{x}\" y=\"{y}\"/>");
    let sp = |id: u32, kind: &str, inner: &str, courses: &str| {
        format!("<special-object id=\"{id}\" kind=\"{kind}\">{inner}{courses}</special-object>")
    };
    let all = "<courses all=\"true\"/>";
    // Course 2 is "B"; course 0 is Purple Pen's all-controls view and part 1 means the whole course here.
    let only_b = "<courses><course course=\"2\"/><course course=\"0\"/><course course=\"2\" part=\"1\"/></courses>";
    let ring = format!("{}{}{}", loc(0.0, 0.0), loc(5.0, 0.0), loc(5.0, 5.0));
    let body = [
        sp(1, "out-of-bounds", &ring, only_b),
        sp(2, "dangerous-area", &ring, all),
        sp(3, "forbidden-route", &loc(1.0, 2.0), all),
        sp(4, "boundary", &format!("{}{}", loc(0.0, 0.0), loc(1.0, 1.0)), all),
        sp(5, "first-aid", &loc(3.0, 4.0), only_b),
        sp(6, "water", &loc(5.0, 6.0), all),
        sp(
            7,
            "text",
            &format!(
                "<text>Hello</text><font name=\"Arial\" bold=\"false\" italic=\"false\" height=\"4.5\"/>\
                 <appearance color=\"purple\"/>{}{}",
                loc(10.0, 20.0),
                loc(30.0, 12.0)
            ),
            all,
        ),
        sp(
            8,
            "text",
            &format!(
                "<text>Auto</text><font name=\"Arial\" bold=\"false\" italic=\"false\"/>{}{}",
                loc(30.0, 12.0),
                loc(10.0, 20.0)
            ),
            all,
        ),
        sp(
            9,
            "line",
            &format!("<appearance line-kind=\"single\" color=\"purple\" line-width=\"0.35\"/>{}{}", loc(0.0, 0.0), loc(2.0, 2.0)),
            all,
        ),
        sp(10, "white-out", &ring, all),
        sp(11, "descriptions", &format!("{}{}", loc(0.0, 0.0), loc(6.0, 0.0)), all),
        sp(12, "registration-mark", &loc(0.0, 0.0), all),
        sp(13, "optional-crossing-point", &loc(0.0, 0.0), all),
        sp(14, "temporary-construction", &ring, all),
        sp(15, "rectangle", &format!("{}{}", loc(0.0, 0.0), loc(2.0, 2.0)), all),
        sp(16, "ellipse", &format!("{}{}", loc(0.0, 0.0), loc(2.0, 2.0)), all),
        sp(17, "image", &format!("<image-data format=\"png\">AAAA</image-data>{}{}", loc(0.0, 0.0), loc(2.0, 2.0)), all),
        sp(18, "hologram", &loc(0.0, 0.0), all),
    ]
    .concat();
    let ev = read_ppen(&ppen(&format!("{TWO}{body}"))).unwrap();
    let p = |x_mm: f64, y_mm: f64| RawPoint { x_mm, y_mm };
    let ring_pts = vec![vec![p(0.0, 0.0), p(5.0, 0.0), p(5.0, 5.0)]];
    let b = CourseFilter::Only(vec!["B".into()]);
    assert_eq!(
        ev.specials,
        vec![
            Special::OutOfBounds {
                id: Some(1),
                rings: ring_pts.clone(),
                courses: b.clone()
            },
            Special::Dangerous {
                id: Some(2),
                rings: ring_pts.clone(),
                courses: CourseFilter::All
            },
            Special::ForbiddenRoute {
                id: Some(3),
                path: vec![p(1.0, 2.0)],
                courses: CourseFilter::All
            },
            Special::Boundary {
                id: Some(4),
                path: vec![p(0.0, 0.0), p(1.0, 1.0)],
                courses: CourseFilter::All
            },
            Special::FirstAid {
                id: Some(5),
                pt: p(3.0, 4.0),
                courses: b
            },
            Special::Water {
                id: Some(6),
                pt: p(5.0, 6.0),
                courses: CourseFilter::All
            },
            Special::Text {
                id: Some(7),
                pt: p(10.0, 20.0),
                text: "Hello".into(),
                size_mm: 4.5,
                courses: CourseFilter::All
            },
            Special::Text {
                id: Some(8),
                pt: p(10.0, 20.0),
                text: "Auto".into(),
                size_mm: 8.0,
                courses: CourseFilter::All
            },
            Special::Line {
                id: Some(9),
                path: vec![p(0.0, 0.0), p(2.0, 2.0)],
                courses: CourseFilter::All
            },
            Special::WhiteOut {
                id: Some(10),
                rings: ring_pts,
                courses: CourseFilter::All
            },
        ]
    );
    for n in [
        "special:descriptions ignored (layout decides)",
        "special:registration-mark",
        "special:optional-crossing-point",
        "special:temporary-construction",
        "special:rectangle",
        "special:ellipse",
        "special:image",
        "special:hologram",
    ] {
        assert!(
            ev.notes.contains(&CourseNote::UnknownElement(n.into())),
            "{n}"
        );
    }
    assert!(ev.notes.contains(&CourseNote::WhiteOutLaterInM4));
}

#[test]
fn unknown_elements_noted_not_fatal() {
    let base = String::from_utf8(fixture("sampleevent8.ppen")).unwrap();
    let modified = base
        .replacen(
            "<title>",
            "<hologram mode=\"3d\"><inner/></hologram><title>",
            1,
        )
        .replacen("<code>38</code>", "<code>38</code><smell>pine</smell>", 1)
        .replacen("<code>41</code>", "<code>41</code><smell>moss</smell>", 1)
        .replacen(
            "<name>Exchanges</name>",
            "<name>Exchanges</name><drone/>",
            1,
        )
        .replacen(
            "<next course-control=\"302\" />",
            "<next course-control=\"302\" /><beacon/>",
            1,
        )
        .replacen(
            "</course-scribe-event>",
            "<team id=\"1\"/><team id=\"2\"/></course-scribe-event>",
            1,
        );
    let ev = read_ppen(modified.as_bytes()).unwrap();
    let plain = read("sampleevent8.ppen");
    assert_eq!(ev.courses, plain.courses);
    assert_eq!(ev.controls, plain.controls);
    let unknown: Vec<_> = ev
        .notes
        .iter()
        .filter_map(|n| match n {
            CourseNote::UnknownElement(s) if !s.starts_with("special:") => Some(s.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(unknown, ["hologram", "smell", "drone", "beacon", "team"]);
}

#[test]
fn unknown_kinds_noted_not_fatal() {
    let body = "<control id=\"1\" kind=\"start\"><location x=\"0\" y=\"0\"/></control>\
         <control id=\"2\" kind=\"teleporter\"><location x=\"1\" y=\"1\"/></control>\
         <course id=\"1\" kind=\"relay-ish\" order=\"1\"><name>A</name><first course-control=\"1\"/></course>\
         <course-control id=\"1\" control=\"1\"><next course-control=\"2\"/></course-control>\
         <course-control id=\"2\" control=\"2\"/>\
         <leg id=\"1\" start-control=\"1\" end-control=\"2\"><flagging kind=\"zigzag\"/></leg>";
    let ev = read_ppen(&ppen(body)).unwrap();
    assert_eq!(codes(&ev, 0), ["STA1", "CTL1"]);
    assert_eq!(ev.controls[1].kind, ControlKind::Control);
    assert_eq!(ev.courses[0].kind, CourseKind::Normal);
    for n in [
        "control kind teleporter",
        "course kind relay-ish",
        "leg flagging zigzag",
    ] {
        assert!(
            ev.notes.contains(&CourseNote::UnknownElement(n.into())),
            "{n}"
        );
    }
}

#[test]
fn map_issue_reads_as_start_and_flagging() {
    let body = "<control id=\"1\" kind=\"map-issue\" map-issue-location=\"beginning\"><location x=\"0\" y=\"0\"/>\
           <description box=\"all\" iof-2004-ref=\"13.6\"/></control>\
         <control id=\"2\" kind=\"start\"><location x=\"0\" y=\"10\"/></control>\
         <control id=\"3\" kind=\"normal\"><code>31</code><location x=\"10\" y=\"10\"/></control>\
         <control id=\"4\" kind=\"finish\"><location x=\"20\" y=\"10\"/><description box=\"all\" iof-2004-ref=\"14.1\"/></control>\
         <control id=\"5\" kind=\"normal\"><code>32</code><location x=\"30\" y=\"10\"/></control>\
         <course id=\"1\" kind=\"normal\" order=\"1\"><name>A</name><first course-control=\"1\"/></course>\
         <course-control id=\"1\" control=\"1\"><next course-control=\"2\"/></course-control>\
         <course-control id=\"2\" control=\"2\"><next course-control=\"3\"/></course-control>\
         <course-control id=\"3\" control=\"3\"><next course-control=\"5\"/></course-control>\
         <course-control id=\"5\" control=\"5\"><next course-control=\"4\"/></course-control>\
         <course-control id=\"4\" control=\"4\"/>\
         <leg id=\"1\" start-control=\"3\" end-control=\"5\"><flagging kind=\"end-part\" x=\"25\" y=\"10\"/>\
           <bends><location x=\"25\" y=\"10\"/></bends></leg>";
    let ev = read_ppen(&ppen(body)).unwrap();
    assert_eq!(ev.controls[0].code, "ISSUE1");
    assert_eq!(ev.controls[0].kind, ControlKind::Start);
    assert!(
        ev.notes
            .contains(&CourseNote::MapIssueAsStart("ISSUE1".into()))
    );
    assert_eq!(codes(&ev, 0), ["ISSUE1", "STA1", "31", "32", "FIN1"]);
    let marked: Vec<_> = ev.courses[0].legs.iter().map(|l| l.marked_route).collect();
    // From the map issue point and into a 14.1 finish Purple Pen flags the whole leg; a partly flagged leg
    // is drawn as a normal leg and noted.
    assert_eq!(marked, [true, false, false, true]);
    assert!(
        ev.notes
            .contains(&CourseNote::UnknownElement("leg flagging end-part".into()))
    );
}

#[test]
fn variations_left_out_and_noted() {
    let body = "<control id=\"1\" kind=\"start\"><location x=\"0\" y=\"0\"/></control>\
         <control id=\"2\" kind=\"normal\"><code>31</code><location x=\"1\" y=\"0\"/></control>\
         <control id=\"3\" kind=\"normal\"><code>32</code><location x=\"2\" y=\"0\"/></control>\
         <control id=\"4\" kind=\"normal\"><code>33</code><location x=\"2\" y=\"2\"/></control>\
         <control id=\"5\" kind=\"finish\"><location x=\"3\" y=\"0\"/></control>\
         <course id=\"1\" kind=\"normal\" order=\"1\"><name>Forked</name><first course-control=\"1\"/></course>\
         <course id=\"2\" kind=\"normal\" order=\"2\"><name>Plain</name><first course-control=\"10\"/></course>\
         <course-control id=\"1\" control=\"1\"><next course-control=\"2\"/></course-control>\
         <course-control id=\"2\" control=\"2\" variation=\"fork\" variation-end=\"5\"><next course-control=\"3\"/>\
           <variation course-control=\"2\"/><variation course-control=\"6\"/></course-control>\
         <course-control id=\"6\" control=\"2\" variation=\"fork\" variation-end=\"5\"><next course-control=\"4\"/>\
           <variation course-control=\"2\"/><variation course-control=\"6\"/></course-control>\
         <course-control id=\"3\" control=\"3\"><next course-control=\"5\"/></course-control>\
         <course-control id=\"4\" control=\"4\"><next course-control=\"5\"/></course-control>\
         <course-control id=\"5\" control=\"5\"/>\
         <course-control id=\"10\" control=\"1\"><next course-control=\"11\"/></course-control>\
         <course-control id=\"11\" control=\"5\"/>";
    let ev = read_ppen(&ppen(body)).unwrap();
    let names: Vec<_> = ev.courses.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["Plain"]);
    assert!(ev.notes.contains(&CourseNote::Variations("Forked".into())));
    // Even after the notes were cut short, a course left out is named.
    let noise: String = (0..1500).map(|i| format!("<gadget{i}/>")).collect();
    let ev = read_ppen(&ppen(&format!("{noise}{body}"))).unwrap();
    assert!(ev.notes.contains(&CourseNote::NotesTruncated));
    assert!(ev.notes.contains(&CourseNote::Variations("Forked".into())));
}

#[test]
fn courses_sorted_by_order() {
    let body = "<control id=\"1\" kind=\"start\"><location x=\"0\" y=\"0\"/></control>\
         <course id=\"1\" kind=\"normal\" order=\"3\"><name>C</name></course>\
         <course id=\"2\" kind=\"normal\" order=\"1\"><name>A</name></course>\
         <course id=\"3\" kind=\"normal\" order=\"2\"><name>B</name></course>";
    let ev = read_ppen(&ppen(body)).unwrap();
    let names: Vec<_> = ev.courses.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["A", "B", "C"]);
}

#[test]
fn refuses_dtd_and_entities() {
    let dtd = b"<?xml version=\"1.0\"?><!DOCTYPE x [<!ENTITY e \"boom\">]><course-scribe-event/>";
    assert!(matches!(read_ppen(dtd), Err(CourseError::Xml(_))));
    let ent = ppen(
        "<control id=\"1\" kind=\"normal\"><code>&e;</code><location x=\"0\" y=\"0\"/></control>",
    );
    assert!(matches!(read_ppen(&ent), Err(CourseError::Xml(_))));
    // The predefined entities and character references are fine.
    let ok = ppen(
        "<control id=\"1\" kind=\"normal\"><code>A&amp;B&#33;</code><location x=\"0\" y=\"0\"/></control>",
    );
    assert_eq!(read_ppen(&ok).unwrap().controls[0].code, "A&B!");
}

#[test]
fn bad_input_is_an_error_not_a_panic() {
    assert!(read_ppen(b"<CourseData/>").is_err());
    assert!(read_ppen(b"<course-scribe-event>").is_err());
    assert!(read_ppen(b"").is_err());
    // A course that uses an undefined course control or control.
    let bad_next = TWO.replace(
        "<next course-control=\"2\"/>",
        "<next course-control=\"9\"/>",
    );
    assert!(read_ppen(&ppen(&bad_next)).is_err());
    let bad_control = TWO.replace("control=\"2\"/>", "control=\"7\"/>");
    assert!(read_ppen(&ppen(&bad_control)).is_err());
    // A chain that loops.
    let looped = TWO.replace(
        "<course-control id=\"2\" control=\"2\"/>",
        "<course-control id=\"2\" control=\"2\"><next course-control=\"1\"/></course-control>",
    );
    assert!(matches!(
        read_ppen(&ppen(&looped)),
        Err(CourseError::Xml(_))
    ));
    // Duplicate ids and codes, bad numbers, a used control without a location.
    let dup_code = format!(
        "{TWO}<control id=\"3\" kind=\"normal\"><code>31</code><location x=\"0\" y=\"0\"/></control>"
    );
    assert!(read_ppen(&ppen(&dup_code)).is_err());
    let dup_id = format!(
        "{TWO}<control id=\"2\" kind=\"normal\"><code>99</code><location x=\"0\" y=\"0\"/></control>"
    );
    assert!(read_ppen(&ppen(&dup_id)).is_err());
    assert!(read_ppen(&ppen(&TWO.replace("x=\"10\"", "x=\"ten\""))).is_err());
    assert!(read_ppen(&ppen(&TWO.replace("x=\"10\"", "x=\"NaN\""))).is_err());
    let no_loc = TWO.replace("<location x=\"10\" y=\"5\"/>", "");
    assert!(read_ppen(&ppen(&no_loc)).is_err());
    // Nested elements that should not nest are noted and skipped.
    for (body, name) in [
        (
            "<control id=\"1\" kind=\"normal\"><code>1</code><control id=\"2\" kind=\"normal\"/></control>",
            "control",
        ),
        (
            "<course id=\"1\" kind=\"normal\"><course id=\"2\" kind=\"normal\"/></course>",
            "course",
        ),
        (
            "<special-object id=\"1\" kind=\"water\"><location x=\"0\" y=\"0\"/>\
             <special-object id=\"2\" kind=\"water\"/></special-object>",
            "special-object",
        ),
    ] {
        let ev = read_ppen(&ppen(body));
        assert!(ev.is_ok(), "{name}");
        let ev = ev.unwrap();
        assert!(
            ev.notes.contains(&CourseNote::UnknownElement(name.into())),
            "{name}"
        );
    }
    // A special without a location.
    let no_pt = "<special-object id=\"1\" kind=\"water\"><courses all=\"true\"/></special-object>";
    assert!(read_ppen(&ppen(no_pt)).is_err());
}

#[test]
fn limits_enforced() {
    // File size.
    let big = vec![b' '; MAX_COURSE_FILE_BYTES + 1];
    assert!(matches!(read_ppen(&big), Err(CourseError::Limit(_))));
    // Depth.
    let deep = format!(
        "{}{}",
        "<a>".repeat(MAX_XML_DEPTH + 1),
        "</a>".repeat(MAX_XML_DEPTH + 1)
    );
    assert!(matches!(
        read_ppen(&ppen(&deep)),
        Err(CourseError::Limit(_))
    ));
    // Points per path, on a special and on a leg.
    let pts = |n: usize| "<location x=\"1\" y=\"1\"/>".repeat(n);
    let special = |n: usize| {
        format!(
            "<special-object id=\"1\" kind=\"boundary\">{}<courses all=\"true\"/></special-object>",
            pts(n)
        )
    };
    assert!(read_ppen(&ppen(&special(MAX_POINTS_PER_PATH))).is_ok());
    assert!(matches!(
        read_ppen(&ppen(&special(MAX_POINTS_PER_PATH + 1))),
        Err(CourseError::Limit(_))
    ));
    let leg = |n: usize| {
        format!(
            "{TWO}<leg id=\"1\" start-control=\"1\" end-control=\"2\"><bends>{}</bends></leg>",
            pts(n)
        )
    };
    assert_eq!(
        read_ppen(&ppen(&leg(MAX_POINTS_PER_PATH))).unwrap().courses[0].legs[0]
            .bends
            .len(),
        MAX_POINTS_PER_PATH
    );
    assert!(matches!(
        read_ppen(&ppen(&leg(MAX_POINTS_PER_PATH + 1))),
        Err(CourseError::Limit(_))
    ));
    // Specials.
    let water = |n: usize| {
        (0..n)
            .map(|i| {
                format!(
                    "<special-object id=\"{i}\" kind=\"water\"><location x=\"1\" y=\"1\"/><courses all=\"true\"/></special-object>"
                )
            })
            .collect::<String>()
    };
    assert_eq!(
        read_ppen(&ppen(&water(MAX_SPECIALS)))
            .unwrap()
            .specials
            .len(),
        MAX_SPECIALS
    );
    assert!(matches!(
        read_ppen(&ppen(&water(MAX_SPECIALS + 1))),
        Err(CourseError::Limit(_))
    ));
    // Controls.
    let controls = |n: usize| {
        (0..n)
            .map(|i| {
                format!("<control id=\"{i}\" kind=\"normal\"><code>{i}</code><location x=\"1\" y=\"1\"/></control>")
            })
            .collect::<String>()
    };
    assert_eq!(
        read_ppen(&ppen(&controls(MAX_CONTROLS)))
            .unwrap()
            .controls
            .len(),
        MAX_CONTROLS
    );
    assert!(matches!(
        read_ppen(&ppen(&controls(MAX_CONTROLS + 1))),
        Err(CourseError::Limit(_))
    ));
    // Courses.
    let courses = |n: usize| {
        (0..n)
            .map(|i| {
                format!(
                    "<course id=\"{i}\" kind=\"normal\" order=\"{}\"><name>{i}</name></course>",
                    i + 1
                )
            })
            .collect::<String>()
    };
    assert_eq!(
        read_ppen(&ppen(&courses(MAX_COURSES)))
            .unwrap()
            .courses
            .len(),
        MAX_COURSES
    );
    assert!(matches!(
        read_ppen(&ppen(&courses(MAX_COURSES + 1))),
        Err(CourseError::Limit(_))
    ));
    // Stops in one course.
    let chain = |n: usize| {
        let mut s = String::from(
            "<control id=\"1\" kind=\"normal\"><code>31</code><location x=\"1\" y=\"1\"/></control>\
             <course id=\"1\" kind=\"normal\" order=\"1\"><name>A</name><first course-control=\"1\"/></course>",
        );
        for i in 1..=n {
            let next = if i < n {
                format!("<next course-control=\"{}\"/>", i + 1)
            } else {
                String::new()
            };
            s.push_str(&format!(
                "<course-control id=\"{i}\" control=\"1\">{next}</course-control>"
            ));
        }
        s
    };
    assert_eq!(
        read_ppen(&ppen(&chain(MAX_STOPS_PER_COURSE)))
            .unwrap()
            .courses[0]
            .stops
            .len(),
        MAX_STOPS_PER_COURSE
    );
    assert!(matches!(
        read_ppen(&ppen(&chain(MAX_STOPS_PER_COURSE + 1))),
        Err(CourseError::Limit(_))
    ));
    // Notes: deduplicated and capped.
    let one = ppen(&"<gadget/>".repeat(5000));
    assert_eq!(read_ppen(&one).unwrap().notes.len(), 2);
    let many: String = (0..MAX_NOTES + 500)
        .map(|i| format!("<gadget{i}/>"))
        .collect();
    let ev = read_ppen(&ppen(&many)).unwrap();
    // The closing note about the whole file still comes after the cut.
    assert_eq!(ev.notes.len(), MAX_NOTES + 2);
    assert_eq!(ev.notes[MAX_NOTES], CourseNote::NotesTruncated);
    assert_eq!(ev.notes.last(), Some(&CourseNote::NoDescriptions));
}

/// `n` courses over the same two controls, each with its own course-controls, at print scale 1:10000.
fn shared(n: usize, control_extra: &str, extra: &str) -> Vec<u8> {
    let mut body = format!(
        "<control id=\"1\" kind=\"start\"><location x=\"0\" y=\"0\"/></control>\
         <control id=\"2\" kind=\"normal\"><code>31</code><location x=\"10\" y=\"5\"/>{control_extra}</control>{extra}"
    );
    for i in 0..n {
        let (a, b) = (2 * i + 1, 2 * i + 2);
        body.push_str(&format!(
            "<course id=\"{i}\" kind=\"normal\" order=\"{}\"><name>{i}</name><first course-control=\"{a}\"/>\
             <options print-scale=\"10000\"/></course>\
             <course-control id=\"{a}\" control=\"1\"><next course-control=\"{b}\"/></course-control>\
             <course-control id=\"{b}\" control=\"2\"/>",
            i + 1
        ));
    }
    ppen(&body)
}

#[test]
fn shared_legs_and_gaps_cannot_multiply_points() {
    let n = MAX_TOTAL_POINTS / MAX_POINTS_PER_PATH;
    assert!(n < MAX_COURSES);
    // One leg with the most bends a path may have, used by every course.
    let bends = "<location x=\"1\" y=\"1\"/>".repeat(MAX_POINTS_PER_PATH);
    let leg =
        format!("<leg id=\"1\" start-control=\"1\" end-control=\"2\"><bends>{bends}</bends></leg>");
    let ev = read_ppen(&shared(n, "", &leg)).unwrap();
    let total: usize = ev.courses.iter().map(|c| c.legs[0].bends.len()).sum();
    assert_eq!(total, MAX_TOTAL_POINTS);
    assert!(matches!(
        read_ppen(&shared(n + 1, "", &leg)),
        Err(CourseError::Limit(_))
    ));
    // One control with the most circle gaps, used by every course.
    let gaps = format!(
        "<circle-gaps scale=\"10000\">{}</circle-gaps>",
        vec!["1:2"; MAX_POINTS_PER_PATH].join(",")
    );
    let ev = read_ppen(&shared(n, &gaps, "")).unwrap();
    let total: usize = ev
        .courses
        .iter()
        .map(|c| c.stops[1].circle_gaps.len())
        .sum();
    assert_eq!(total, MAX_TOTAL_POINTS);
    assert!(matches!(
        read_ppen(&shared(n + 1, &gaps, "")),
        Err(CourseError::Limit(_))
    ));
}

#[test]
fn variation_loop_reaches_variations_without_loop_error() {
    // As in Purple Pen's variations.ppen: course-control 4 is the loop exit and continues to 5; the loop
    // branches 25 and 26 lead back to 4 (variation-end).
    let body = "<control id=\"1\" kind=\"start\"><location x=\"0\" y=\"0\"/></control>\
         <control id=\"4\" kind=\"normal\"><code>34</code><location x=\"1\" y=\"0\"/></control>\
         <control id=\"6\" kind=\"normal\"><code>36</code><location x=\"1\" y=\"1\"/></control>\
         <control id=\"7\" kind=\"normal\"><code>37</code><location x=\"2\" y=\"1\"/></control>\
         <control id=\"9\" kind=\"finish\"><location x=\"3\" y=\"0\"/></control>\
         <course id=\"1\" kind=\"normal\" order=\"1\"><name>Loop</name><first course-control=\"1\"/></course>\
         <course-control id=\"1\" control=\"1\"><next course-control=\"4\"/></course-control>\
         <course-control id=\"4\" control=\"4\" variation=\"loop\" variation-end=\"4\"><next course-control=\"5\"/>\
           <variation course-control=\"4\"/><variation course-control=\"25\"/><variation course-control=\"26\"/></course-control>\
         <course-control id=\"25\" control=\"4\" variation=\"loop\" variation-end=\"4\"><next course-control=\"15\"/>\
           <variation course-control=\"4\"/><variation course-control=\"25\"/><variation course-control=\"26\"/></course-control>\
         <course-control id=\"26\" control=\"4\" variation=\"loop\" variation-end=\"4\"><next course-control=\"17\"/>\
           <variation course-control=\"4\"/><variation course-control=\"25\"/><variation course-control=\"26\"/></course-control>\
         <course-control id=\"15\" control=\"6\"><next course-control=\"4\"/></course-control>\
         <course-control id=\"17\" control=\"7\"><next course-control=\"4\"/></course-control>\
         <course-control id=\"5\" control=\"9\"/>";
    let ev = read_ppen(&ppen(body)).unwrap();
    assert!(ev.courses.is_empty());
    assert!(ev.notes.contains(&CourseNote::Variations("Loop".into())));
    // Entered through a branch, the chain comes back to the loop control: still a variation, no loop error.
    let via_branch = body.replace(
        "<first course-control=\"1\"/>",
        "<first course-control=\"15\"/>",
    );
    let ev = read_ppen(&ppen(&via_branch)).unwrap();
    assert!(ev.notes.contains(&CourseNote::Variations("Loop".into())));
}

#[test]
fn numbering_labels_and_points_noted() {
    let body = TWO
        .replace(
            "<first course-control=\"1\"/>",
            "<first course-control=\"1\" control-number=\"5\"/><labels label-kind=\"sequence-and-code\"/>",
        )
        .replace(
            "<course-control id=\"2\" control=\"2\"/>",
            "<course-control id=\"2\" control=\"2\" points=\"30\"/>",
        );
    let ev = read_ppen(&ppen(&body)).unwrap();
    for n in [
        "course:control-number",
        "course:label-kind",
        "course-control:points",
    ] {
        assert!(
            ev.notes.contains(&CourseNote::UnknownElement(n.into())),
            "{n}"
        );
    }
    // The defaults give no note: sequence for a normal course, code for a score course.
    let plain = TWO.replace(
        "<first course-control=\"1\"/>",
        "<first course-control=\"1\" control-number=\"1\"/><labels label-kind=\"sequence\"/>",
    );
    let score = format!(
        "{TWO}<course id=\"3\" kind=\"score\" order=\"3\"><name>S</name><labels label-kind=\"code\"/></course>"
    );
    for b in [plain, score] {
        let ev = read_ppen(&ppen(&b)).unwrap();
        assert!(
            !ev.notes
                .iter()
                .any(|n| matches!(n, CourseNote::UnknownElement(s) if s.starts_with("course"))),
            "{:?}",
            ev.notes
        );
    }
}

#[test]
fn code_and_name_lengths_limited() {
    let with_code = |code: &str| TWO.replace("<code>31</code>", &format!("<code>{code}</code>"));
    // Characters, not bytes: 64 umlauts are 128 bytes.
    let ev = read_ppen(&ppen(&with_code(&"ü".repeat(MAX_CODE_LEN)))).unwrap();
    assert_eq!(ev.courses[0].stops[1].control.chars().count(), MAX_CODE_LEN);
    assert!(matches!(
        read_ppen(&ppen(&with_code(&"ü".repeat(MAX_CODE_LEN + 1)))),
        Err(CourseError::Limit(_))
    ));
    let with_name = |name: &str| TWO.replace("<name>A</name>", &format!("<name>{name}</name>"));
    assert!(read_ppen(&ppen(&with_name(&"ä".repeat(MAX_NAME_LEN)))).is_ok());
    assert!(matches!(
        read_ppen(&ppen(&with_name(&"ä".repeat(MAX_NAME_LEN + 1)))),
        Err(CourseError::Limit(_))
    ));
}

/// `n` courses that all start at the same chain of `MAX_STOPS_PER_COURSE` visits of one control whose code
/// has `MAX_CODE_LEN` bytes. Course names have 4 bytes.
fn shared_chain(n: usize) -> Vec<u8> {
    let mut body = format!(
        "<control id=\"1\" kind=\"normal\"><code>{}</code><location x=\"1\" y=\"1\"/></control>",
        "c".repeat(MAX_CODE_LEN)
    );
    for i in 1..=MAX_STOPS_PER_COURSE {
        let next = if i < MAX_STOPS_PER_COURSE {
            format!("<next course-control=\"{}\"/>", i + 1)
        } else {
            String::new()
        };
        body.push_str(&format!(
            "<course-control id=\"{i}\" control=\"1\">{next}</course-control>"
        ));
    }
    for i in 0..n {
        body.push_str(&format!(
            "<course id=\"{i}\" kind=\"normal\" order=\"{}\"><name>{i:04}</name><first course-control=\"1\"/></course>",
            i + 1
        ));
    }
    ppen(&body)
}

#[test]
fn shared_chain_with_long_code_is_bounded() {
    // Copied text: the control's code once, and per course 500 stop codes plus a 4-byte name.
    let per_course = MAX_STOPS_PER_COURSE * MAX_CODE_LEN + 4;
    let n = (MAX_TOTAL_TEXT_BYTES - MAX_CODE_LEN) / per_course;
    assert!(n < MAX_COURSES);
    let ev = read_ppen(&shared_chain(n)).unwrap();
    let bytes: usize = ev.controls.iter().map(|c| c.code.len()).sum::<usize>()
        + ev.courses
            .iter()
            .map(|c| c.name.len() + c.stops.iter().map(|s| s.control.len()).sum::<usize>())
            .sum::<usize>();
    assert_eq!(bytes, MAX_CODE_LEN + n * per_course);
    assert!(bytes <= MAX_TOTAL_TEXT_BYTES);
    assert!(matches!(
        read_ppen(&shared_chain(n + 1)),
        Err(CourseError::Limit(_))
    ));
}

/// `MAX_COURSES` courses with names of `MAX_NAME_LEN` bytes and `m` specials that list every course twice.
fn named_specials(m: usize) -> Vec<u8> {
    let mut body = String::new();
    for i in 0..MAX_COURSES {
        body.push_str(&format!(
            "<course id=\"{i}\" kind=\"normal\" order=\"{}\"><name>{i:0>width$}</name></course>",
            i + 1,
            width = MAX_NAME_LEN
        ));
    }
    let list: String = (0..2 * MAX_COURSES)
        .map(|i| format!("<course course=\"{}\"/>", i % MAX_COURSES))
        .collect();
    for j in 0..m {
        body.push_str(&format!(
            "<special-object id=\"{j}\" kind=\"water\"><location x=\"1\" y=\"1\"/><courses>{list}</courses></special-object>"
        ));
    }
    ppen(&body)
}

#[test]
fn specials_with_long_names_are_bounded() {
    // Copied text: every course name once, and per special every name once (duplicates dropped).
    let names = MAX_COURSES * MAX_NAME_LEN;
    let m = (MAX_TOTAL_TEXT_BYTES - names) / names;
    let ev = read_ppen(&named_specials(m)).unwrap();
    assert_eq!(ev.specials.len(), m);
    let bytes: usize = ev.courses.iter().map(|c| c.name.len()).sum::<usize>()
        + ev.specials
            .iter()
            .map(|s| match s {
                Special::Water {
                    courses: CourseFilter::Only(v),
                    ..
                } => {
                    assert_eq!(v.len(), MAX_COURSES);
                    v.iter().map(String::len).sum::<usize>()
                }
                other => panic!("{other:?}"),
            })
            .sum::<usize>();
    assert_eq!(bytes, names * (m + 1));
    assert!(bytes <= MAX_TOTAL_TEXT_BYTES);
    assert!(matches!(
        read_ppen(&named_specials(m + 1)),
        Err(CourseError::Limit(_))
    ));
}

#[test]
fn description_standard_length_limited() {
    let with_standard = |std: &str| {
        let body = TWO.replace(
            "<code>31</code>",
            "<code>31</code><description box=\"D\" iof-2004-ref=\"1.1\"/>",
        );
        format!(
            "<course-scribe-event><event id=\"1\"><standards map=\"2017\" description=\"{std}\"/>\
             </event>{body}</course-scribe-event>"
        )
        .into_bytes()
    };
    let at = "2".repeat(MAX_STANDARD_LEN);
    let ev = read_ppen(&with_standard(&at)).unwrap();
    assert_eq!(ev.controls[1].description.as_ref().unwrap().standard, at);
    assert!(matches!(
        read_ppen(&with_standard(&"2".repeat(MAX_STANDARD_LEN + 1))),
        Err(CourseError::Limit(_))
    ));
}

#[test]
fn map_flip_and_secondary_title_read() {
    let ev = read("sampleevent8.ppen");
    let course = &ev.courses[0];
    // Control 75 is a map exchange that flips the map; 41 is a plain map exchange.
    let flip = |code: &str| {
        course
            .stops
            .iter()
            .find(|s| s.control == code)
            .unwrap()
            .map_flip
    };
    assert!(flip("75"));
    assert!(!flip("41"));
    assert!(!flip("59"));
    assert_eq!(course.secondary_title, None);
    // A secondary title is kept as written, `|` and all.
    let text = String::from_utf8(fixture("sampleevent8.ppen")).unwrap();
    let with = text.replacen(
        "<name>Exchanges</name>",
        "<name>Exchanges</name>\n    <secondary-title>Damen|Lang</secondary-title>",
        1,
    );
    let ev = read_ppen(with.as_bytes()).unwrap();
    assert_eq!(ev.courses[0].secondary_title.as_deref(), Some("Damen|Lang"));
}

#[test]
fn item_scaling_is_read() {
    assert_eq!(
        read("sampleevent8.ppen").source.item_scaling,
        Some(ItemScaling::None)
    );
    let with = |scale: &str| {
        let file = format!(
            "<course-scribe-event><event id=\"1\"><title>T</title>\
             <map kind=\"none\" scale=\"10000\"></map><course-appearance scale-sizes=\"{scale}\"/>\
             </event>{TWO}</course-scribe-event>"
        );
        read_ppen(file.as_bytes()).unwrap().source.item_scaling
    };
    assert_eq!(with("RelativeToMap"), Some(ItemScaling::RelativeToMap));
    assert_eq!(with("RelativeTo15000"), Some(ItemScaling::RelativeTo15000));
    let iof = std::fs::read(format!(
        "{}/tests/fixtures/courses/y-up.xml",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap();
    assert_eq!(
        freemapper_course::read_iof_xml(&iof)
            .unwrap()
            .source
            .item_scaling,
        None
    );
}

#[test]
fn course_obj_ratio_is_public() {
    assert_eq!(
        course_obj_ratio(ItemScaling::None, Some(10000.0), Some(7500.0)),
        0.75
    );
    assert_eq!(
        course_obj_ratio(ItemScaling::RelativeTo15000, Some(10000.0), None),
        1.5
    );
    assert_eq!(
        course_obj_ratio(ItemScaling::RelativeToMap, Some(10000.0), Some(7500.0)),
        1.0
    );
    assert_eq!(course_obj_ratio(ItemScaling::None, None, Some(7500.0)), 1.0);
}
