//! IOF XML 3.0 `CourseData` reader (M3 Task 2).

use freemapper_course::model::{
    ControlKind, CourseKind, CourseNote, MAX_CODE_LEN, MAX_CONTROLS, MAX_COURSE_FILE_BYTES,
    MAX_COURSES, MAX_NAME_LEN, MAX_NOTES, MAX_STOPS_PER_COURSE, MAX_XML_DEPTH,
};
use freemapper_course::{CourseError, read_iof_xml};

fn fixture(name: &str) -> Vec<u8> {
    let path = format!(
        "{}/tests/fixtures/courses/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read(path).unwrap()
}

fn wrap(body: &str) -> Vec<u8> {
    format!("<CourseData><RaceCourseData>{body}</RaceCourseData></CourseData>").into_bytes()
}

#[test]
fn reads_controls_courses_and_order() {
    let ev = read_iof_xml(&fixture("y-up.xml")).unwrap();
    let codes: Vec<_> = ev.controls.iter().map(|c| c.code.as_str()).collect();
    assert_eq!(codes, ["S1", "31", "32", "X1", "F1"]);
    let kinds: Vec<_> = ev.controls.iter().map(|c| c.kind).collect();
    assert_eq!(
        kinds,
        [
            ControlKind::Start,
            ControlKind::Control,
            ControlKind::Control,
            ControlKind::CrossingPoint,
            ControlKind::Finish
        ]
    );
    assert_eq!(ev.courses.len(), 2);
    assert_eq!(ev.courses[0].name, "A");
    let stops: Vec<_> = ev.courses[0]
        .stops
        .iter()
        .map(|s| s.control.as_str())
        .collect();
    assert_eq!(stops, ["S1", "31", "32", "F1"]);
    assert_eq!(ev.courses[0].legs.len(), 3);
    assert_eq!(ev.courses[0].kind, CourseKind::Normal);
    assert_eq!(ev.courses[1].name, "B");
    assert_eq!(ev.source.event_name.as_deref(), Some("Test Event"));
    assert_eq!(ev.source.map_scale, Some(10000));
}

#[test]
fn reads_map_position_mm_and_text_position() {
    let ev = read_iof_xml(&fixture("y-up.xml")).unwrap();
    let c = &ev.controls[0];
    let p = c.position.unwrap();
    assert_eq!((p.x_mm, p.y_mm), (50.5, 40.25));
    assert_eq!(c.geo, Some((53.8655, 10.6866)));
    assert_eq!(ev.controls[2].geo, None);

    let ev = read_iof_xml(&fixture("text-position.xml")).unwrap();
    let course = &ev.courses[0];
    assert_eq!(course.kind, CourseKind::Score);
    let p = course.stops[1].number_text_at.unwrap();
    assert_eq!((p.dx_mm, p.dy_mm), (6.5, 3.0));
    assert!(course.stops[1].number.is_none());
    assert!(course.stops[0].number_text_at.is_none());
}

#[test]
fn length_and_climb_taken_when_present() {
    let ev = read_iof_xml(&fixture("y-up.xml")).unwrap();
    assert_eq!(ev.courses[0].length_m, Some(4500.0));
    assert_eq!(ev.courses[0].climb_m, Some(120.0));
    assert_eq!(ev.courses[1].length_m, None);
    assert_eq!(ev.courses[1].climb_m, None);
}

#[test]
fn forked_family_kept_and_alternatives_skipped() {
    let ev = read_iof_xml(&fixture("forked.xml")).unwrap();
    let names: Vec<_> = ev.courses.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["Relay 1", "Relay 2", "Plain"]);
    assert_eq!(ev.courses[0].family.as_deref(), Some("Relay"));
    assert_eq!(ev.courses[2].family, None);
    assert!(ev.notes.contains(&CourseNote::Forked {
        family: "Relay".into(),
        variants: vec!["Relay 1".into(), "Relay 2".into()]
    }));
    assert!(
        ev.notes
            .contains(&CourseNote::AlternativeControls("Alternatives".into()))
    );
}

#[test]
fn duplicate_control_id_refused() {
    let c = "<Control type=\"Control\"><Id>31</Id><MapPosition x=\"1\" y=\"1\"/></Control>";
    let err = read_iof_xml(&wrap(&format!("{c}{c}"))).unwrap_err();
    assert!(err.to_string().contains("31"), "{err}");
}

#[test]
fn geo_only_control_ok_when_used_missing_position_error() {
    let geo = "<Control type=\"Start\"><Id>S</Id><Position lat=\"53.1\" lng=\"10.2\"/></Control>";
    let course = "<Course><Name>A</Name><CourseControl type=\"Start\"><Control>S</Control></CourseControl></Course>";
    let ev = read_iof_xml(&wrap(&format!("{geo}{course}"))).unwrap();
    assert_eq!(ev.controls[0].position, None);
    assert_eq!(ev.controls[0].geo, Some((53.1, 10.2)));
    // Unused control with neither: fine. Used: error.
    let bare = "<Control type=\"Control\"><Id>Z</Id></Control>";
    assert!(read_iof_xml(&wrap(bare)).is_ok());
    let used = "<Course><Name>A</Name><CourseControl><Control>Z</Control></CourseControl></Course>";
    assert!(read_iof_xml(&wrap(&format!("{bare}{used}"))).is_err());
}

#[test]
fn malformed_nesting_is_an_error_not_a_panic() {
    let ctl = "<Control type=\"Start\"><Id>S</Id><MapPosition x=\"1\" y=\"1\"/></Control>";
    // RaceCourseData inside RaceCourseData is ignored and noted.
    let xml = format!(
        "<CourseData><RaceCourseData><RaceCourseData>{ctl}</RaceCourseData></RaceCourseData></CourseData>"
    );
    let ev = read_iof_xml(xml.as_bytes()).unwrap();
    assert!(ev.controls.is_empty());
    // Course inside Course, CourseControl inside CourseControl, Control inside Control: errors that name it.
    for (body, what) in [
        ("<Course><Course><Name>x</Name></Course></Course>", "Course"),
        (
            "<Course><CourseControl><CourseControl/></CourseControl></Course>",
            "CourseControl",
        ),
        ("<Control><Control/></Control>", "Control"),
    ] {
        let err = read_iof_xml(&wrap(body)).unwrap_err().to_string();
        assert!(
            err.contains(&format!("<{what}> opened inside another <{what}>")),
            "{err}"
        );
    }
    // Wrong root and RaceCourseData outside the root.
    assert!(read_iof_xml(b"<Other/>").is_err());
    let _ = read_iof_xml(b"<CourseData><X><RaceCourseData/></X></CourseData>");
}

#[test]
fn notes_are_deduplicated_and_capped() {
    let one = wrap(&"<Gadget/>".repeat(5000));
    assert_eq!(read_iof_xml(&one).unwrap().notes.len(), 2);
    let mut body = String::new();
    for i in 0..(MAX_NOTES + 500) {
        body.push_str(&format!("<Gadget{i}/>"));
    }
    // A course left out and the notes about the whole file still come after the cut.
    body.push_str(
        "<Control><Id>A</Id><MapPosition x=\"1\" y=\"1\"/></Control>\
         <Control><Id>B</Id><MapPosition x=\"2\" y=\"2\"/></Control>\
         <Course><Name>Alt</Name><CourseControl><Control>A</Control><Control>B</Control></CourseControl></Course>",
    );
    let ev = read_iof_xml(&wrap(&body)).unwrap();
    assert_eq!(ev.notes.len(), MAX_NOTES + 3);
    assert_eq!(ev.notes[MAX_NOTES], CourseNote::NotesTruncated);
    assert_eq!(
        ev.notes[MAX_NOTES + 1..],
        [
            CourseNote::AlternativeControls("Alt".into()),
            CourseNote::NoDescriptions
        ]
    );
}

#[test]
fn unknown_course_control_type_is_noted() {
    let xml = wrap(
        "<Control><Id>A</Id><MapPosition x=\"1\" y=\"1\"/></Control>\
         <Course><Name>c</Name><CourseControl type=\"Teleport\"><Control>A</Control></CourseControl></Course>",
    );
    let ev = read_iof_xml(&xml).unwrap();
    assert_eq!(ev.courses[0].stops[0].kind, None);
    assert!(
        ev.notes.contains(&CourseNote::UnknownElement(
            "CourseControl type Teleport".into()
        )),
        "{:?}",
        ev.notes
    );
}

#[test]
fn course_control_type_overrides_and_end_of_marked_route() {
    let xml = wrap(
        "<Control type=\"Control\"><Id>A</Id><MapPosition x=\"1\" y=\"1\"/></Control>\
         <Control type=\"EndOfMarkedRoute\"><Id>E</Id><MapPosition x=\"2\" y=\"2\"/></Control>\
         <Course><Name>c</Name><CourseControl type=\"Finish\"><Control>A</Control></CourseControl></Course>\
         <Course><Name>d</Name><CourseControl><Control>A</Control></CourseControl></Course>",
    );
    let ev = read_iof_xml(&xml).unwrap();
    // The override belongs to the stop: A is the finish of course c and a plain control of course d.
    assert_eq!(ev.controls[0].kind, ControlKind::Control);
    assert_eq!(ev.courses[0].stops[0].kind, Some(ControlKind::Finish));
    assert_eq!(ev.courses[1].stops[0].kind, None);
    assert_eq!(ev.controls[1].kind, ControlKind::EndOfMarkedRoute);
}

#[test]
fn negative_length_and_climb_refused() {
    assert!(read_iof_xml(&wrap("<Course><Name>x</Name><Length>-1</Length></Course>")).is_err());
    assert!(read_iof_xml(&wrap("<Course><Name>x</Name><Climb>-5</Climb></Course>")).is_err());
}

#[test]
fn bom_and_namespace_prefix() {
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(
        b"<iof:CourseData xmlns:iof=\"http://www.orienteering.org/datastandard/3.0\"><iof:RaceCourseData>\
          <iof:Control type=\"Start\"><iof:Id>S</iof:Id><iof:MapPosition x=\"1\" y=\"2\"/></iof:Control>\
          </iof:RaceCourseData></iof:CourseData>",
    );
    let ev = read_iof_xml(&bytes).unwrap();
    assert_eq!(ev.controls[0].code, "S");
    assert_eq!(ev.controls[0].position.unwrap().y_mm, 2.0);
}

#[test]
fn refuses_dtd_and_entities() {
    let xml = br#"<?xml version="1.0"?><!DOCTYPE CourseData [<!ENTITY x "y">]><CourseData/>"#;
    assert!(matches!(read_iof_xml(xml), Err(CourseError::Xml(_))));
    let xml = br#"<CourseData><Event><Name>&boom;</Name></Event></CourseData>"#;
    assert!(read_iof_xml(xml).is_err());
}

#[test]
fn limits_enforced() {
    // File size.
    let big = vec![b' '; MAX_COURSE_FILE_BYTES + 1];
    assert!(matches!(read_iof_xml(&big), Err(CourseError::Limit(_))));

    // Controls.
    let mut body = String::new();
    for i in 0..=MAX_CONTROLS {
        body.push_str(&format!(
            "<Control type=\"Control\"><Id>c{i}</Id><MapPosition x=\"1\" y=\"1\"/></Control>"
        ));
    }
    assert!(matches!(
        read_iof_xml(&wrap(&body)),
        Err(CourseError::Limit(_))
    ));
    // Exactly the limit passes.
    let ok = body.rsplit_once("<Control type").unwrap().0.to_string();
    assert_eq!(
        read_iof_xml(&wrap(&ok)).unwrap().controls.len(),
        MAX_CONTROLS
    );

    // Courses.
    let ctl = "<Control type=\"Start\"><Id>S</Id><MapPosition x=\"1\" y=\"1\"/></Control>";
    let mut body = ctl.to_string();
    for i in 0..=MAX_COURSES {
        body.push_str(&format!("<Course><Name>c{i}</Name></Course>"));
    }
    assert!(matches!(
        read_iof_xml(&wrap(&body)),
        Err(CourseError::Limit(_))
    ));

    // Stops per course.
    let mut body = ctl.to_string();
    body.push_str("<Course><Name>x</Name>");
    for _ in 0..=MAX_STOPS_PER_COURSE {
        body.push_str("<CourseControl type=\"Control\"><Control>S</Control></CourseControl>");
    }
    body.push_str("</Course>");
    assert!(matches!(
        read_iof_xml(&wrap(&body)),
        Err(CourseError::Limit(_))
    ));

    // Depth.
    let n = MAX_XML_DEPTH + 1;
    let xml = format!(
        "<CourseData>{}{}</CourseData>",
        "<a>".repeat(n),
        "</a>".repeat(n)
    );
    assert!(matches!(
        read_iof_xml(xml.as_bytes()),
        Err(CourseError::Limit(_))
    ));
    let n = MAX_XML_DEPTH - 1;
    let xml = format!(
        "<CourseData>{}{}</CourseData>",
        "<a>".repeat(n),
        "</a>".repeat(n)
    );
    assert!(read_iof_xml(xml.as_bytes()).is_ok());
}

#[test]
fn non_mm_unit_refused() {
    let xml = wrap(
        "<Control type=\"Start\"><Id>S</Id><MapPosition x=\"1\" y=\"1\" unit=\"px\"/></Control>",
    );
    assert!(matches!(
        read_iof_xml(&xml),
        Err(CourseError::Unsupported(_))
    ));
}

#[test]
fn bad_input_is_an_error() {
    let nan = wrap("<Control type=\"Start\"><Id>S</Id><MapPosition x=\"NaN\" y=\"1\"/></Control>");
    assert!(read_iof_xml(&nan).is_err());
    let dangling = wrap(
        "<Course><Name>x</Name><CourseControl type=\"Control\"><Control>nope</Control></CourseControl></Course>",
    );
    assert!(read_iof_xml(&dangling).is_err());
}

#[test]
fn unknown_element_noted() {
    let xml = wrap("<Gadget/>");
    let ev = read_iof_xml(&xml).unwrap();
    assert!(
        ev.notes
            .contains(&CourseNote::UnknownElement("Gadget".into()))
    );
    assert!(ev.notes.contains(&CourseNote::NoDescriptions));
}

#[test]
fn code_and_name_lengths_limited() {
    let body = |code: &str, name: &str, family: &str| {
        wrap(&format!(
            "<Control><Id>{code}</Id><MapPosition x=\"1\" y=\"1\"/></Control>\
             <Course><Name>{name}</Name><CourseFamily>{family}</CourseFamily>\
             <CourseControl><Control>{code}</Control></CourseControl></Course>"
        ))
    };
    let (code, name) = ("ü".repeat(MAX_CODE_LEN), "ä".repeat(MAX_NAME_LEN));
    let ev = read_iof_xml(&body(&code, &name, &name)).unwrap();
    assert_eq!(ev.courses[0].stops[0].control, code);
    for bad in [
        body(&"ü".repeat(MAX_CODE_LEN + 1), &name, &name),
        body(&code, &"ä".repeat(MAX_NAME_LEN + 1), &name),
        body(&code, &name, &"ä".repeat(MAX_NAME_LEN + 1)),
    ] {
        assert!(matches!(read_iof_xml(&bad), Err(CourseError::Limit(_))));
    }
}
