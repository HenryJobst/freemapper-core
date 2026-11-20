// SPDX-License-Identifier: AGPL-3.0-or-later

//! Purple Pen `.ppen` patch of an existing file (M3b Task 5).

use std::collections::{BTreeMap, BTreeSet};

use freemapper_course::model::{
    ControlKind, CourseFilter, MAX_CONTROLS, MAX_COURSE_FILE_BYTES, MAX_POINTS_PER_PATH, RawPoint,
    Special, Stop,
};
use freemapper_course::{
    Control, Course, CourseError, CourseKind, Deletions, KeptObject, PatchOptions, PpenWriteError,
    RawEvent, WriteNote, Written, read_ppen, write_ppen_patch,
};

const FIXTURES: [&str; 3] = ["lincoln-park.ppen", "overlapper.ppen", "sampleevent8.ppen"];

fn fixture(name: &str) -> String {
    let path = format!(
        "{}/tests/fixtures/courses/ppen/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    String::from_utf8(std::fs::read(path).unwrap()).unwrap()
}

fn read(text: &str) -> RawEvent {
    read_ppen(text.as_bytes()).unwrap()
}

fn patch(base: &str, ev: &RawEvent) -> Written {
    write_ppen_patch(base.as_bytes(), ev, &PatchOptions::default())
        .unwrap_or_else(|e| panic!("{e}"))
}

/// The patch of `base` with an event read from `base` itself, so that its ids count.
fn own_ids() -> PatchOptions {
    PatchOptions {
        ids_from_base: true,
        ..PatchOptions::default()
    }
}

fn patch_own(base: &str, ev: &RawEvent) -> Result<Written, PpenWriteError> {
    write_ppen_patch(base.as_bytes(), ev, &own_ids())
}

fn deleting(base: &str, ev: &RawEvent, delete: Deletions) -> Result<Written, PpenWriteError> {
    write_ppen_patch(
        base.as_bytes(),
        ev,
        &PatchOptions {
            delete,
            ..PatchOptions::default()
        },
    )
}

fn stop(code: &str) -> Stop {
    Stop {
        control: code.into(),
        kind: None,
        number: None,
        number_text_at: None,
        circle_gaps: Vec::new(),
        map_flip: false,
    }
}

fn control(code: &str, x: f64, y: f64) -> Control<RawPoint> {
    Control {
        id: None,
        code: code.into(),
        kind: ControlKind::Control,
        position: Some(RawPoint { x_mm: x, y_mm: y }),
        geo: None,
        description: None,
    }
}

fn course(name: &str, stops: &[&str], print_scale: Option<u32>) -> Course<RawPoint> {
    Course {
        id: None,
        name: name.into(),
        family: None,
        kind: CourseKind::Normal,
        legs: Vec::new(),
        stops: stops.iter().map(|s| stop(s)).collect(),
        climb_m: None,
        length_m: None,
        print_scale_hint: print_scale,
        course_obj_ratio: None,
        secondary_title: None,
    }
}

/// The values of `attr="N"` after each `<tag ` in `text`.
fn ids_of(text: &str, marker: &str) -> Vec<i64> {
    text.match_indices(marker)
        .map(|(i, _)| {
            let rest = &text[i + marker.len()..];
            rest[..rest.find('"').unwrap()].parse().unwrap()
        })
        .collect()
}

/// Every course-control is reached from a `first` or a `next`, and every reference names one.
fn course_controls_consistent(text: &str) {
    let defined: BTreeSet<i64> = ids_of(text, "<course-control id=\"").into_iter().collect();
    let referenced: BTreeSet<i64> = ids_of(text, "course-control=\"").into_iter().collect();
    assert_eq!(defined, referenced, "{text}");
}

/// The text of the top-level element `<tag id="id" …>…</tag>` (or `… />`) in `text`.
fn element<'a>(text: &'a str, tag: &str, id: u32) -> &'a str {
    let start = text
        .find(&format!("<{tag} id=\"{id}\""))
        .unwrap_or_else(|| panic!("no {tag} {id}"));
    let rest = &text[start..];
    let line_end = rest.find('\n').unwrap();
    if rest[..line_end].trim_end().ends_with("/>") {
        return rest[..line_end].trim_end();
    }
    let close = format!("</{tag}>");
    &rest[..rest.find(&close).unwrap() + close.len()]
}

#[test]
fn untouched_patch_is_byte_identical() {
    for name in FIXTURES {
        let f = fixture(name);
        let ev = read(&f);
        let w = patch(&f, &ev);
        assert_eq!(w.text.as_bytes(), f.as_bytes(), "{name}");
        assert_eq!(
            w.ids.controls,
            ev.controls
                .iter()
                .map(|c| c.id.unwrap())
                .collect::<Vec<_>>(),
            "{name}"
        );
        assert_eq!(
            w.ids.courses,
            ev.courses.iter().map(|c| c.id.unwrap()).collect::<Vec<_>>(),
            "{name}"
        );
        assert_eq!(
            w.ids.specials,
            ev.specials
                .iter()
                .map(|s| s.id().unwrap())
                .collect::<Vec<_>>(),
            "{name}"
        );
        let kept = match name {
            "sampleevent8.ppen" => vec![KeptObject::Special {
                id: 3,
                kind: "descriptions".into(),
            }],
            _ => Vec::new(),
        };
        assert_eq!(w.kept, kept, "{name}");
    }
    // A variation course, which the reader leaves out, is kept as it is.
    let lincoln = fixture("lincoln-park.ppen");
    let fork = lincoln.replace(
        "  <course-control id=\"1\" control=\"1\">",
        "  <course id=\"2\" kind=\"normal\" order=\"2\">\r\n    <name>Fork</name>\r\n    \
         <first course-control=\"10\" />\r\n  </course>\r\n  \
         <course-control id=\"10\" control=\"1\" variation=\"fork\" variation-end=\"11\">\r\n    \
         <next course-control=\"11\" />\r\n  </course-control>\r\n  \
         <course-control id=\"11\" control=\"5\" />\r\n  <course-control id=\"1\" control=\"1\">",
    );
    let ev = read(&fork);
    assert_eq!(ev.courses.len(), 1);
    let w = patch(&fork, &ev);
    assert_eq!(w.text, fork);
    assert_eq!(
        w.kept,
        [KeptObject::Course {
            id: 2,
            name: "Fork".into()
        }]
    );
    // An event course matching the variation course is refused.
    let mut ev = read(&fork);
    ev.courses.push(course("Fork", &["STA1", "FIN1"], None));
    assert!(matches!(
        write_ppen_patch(fork.as_bytes(), &ev, &PatchOptions::default()),
        Err(PpenWriteError::Unsupported(_))
    ));
}

#[test]
fn one_moved_control_changes_one_line() {
    let f = fixture("lincoln-park.ppen");
    let mut ev = read(&f);
    let c = ev.controls.iter_mut().find(|c| c.code == "31").unwrap();
    c.position.as_mut().unwrap().x_mm += 1.0;
    let w = patch(&f, &ev);
    let before: Vec<&str> = f.split('\n').collect();
    let after: Vec<&str> = w.text.split('\n').collect();
    assert_eq!(before.len(), after.len());
    let changed: Vec<usize> = (0..before.len())
        .filter(|&i| before[i] != after[i])
        .collect();
    assert_eq!(changed.len(), 1, "{}", w.text);
    let i = changed[0];
    assert_eq!(
        before[i],
        "    <location x=\"127.508255\" y=\"110.638184\" />\r"
    );
    assert!(
        after[i].starts_with("    <location x=\"128.50826\""),
        "{}",
        after[i]
    );
    let back = read(&w.text);
    let c = back.controls.iter().find(|c| c.code == "31").unwrap();
    assert_eq!(
        c.position.unwrap().x_mm as f32,
        (127.508255_f64 + 1.0) as f32
    );
}

#[test]
fn unknown_content_survives() {
    let base = fixture("lincoln-park.ppen")
        .replace(
            "224.4232:330.1188</circle-gaps>",
            "224.4232:330.1188</circle-gaps>\r\n    <foo a=\"1\" />",
        )
        .replace("order=\"1\">", "order=\"1\" x-new=\"2\">")
        .replace(
            "\r\n  <course id=\"1\"",
            "\r\n  <!-- between -->\r\n  <course id=\"1\"",
        );
    let mut ev = read(&base);
    let c = ev.controls.iter_mut().find(|c| c.code == "31").unwrap();
    c.description.as_mut().unwrap().columns[1] = Some("1.11".into());
    ev.courses[0].name = "Kurz".into();
    let w = patch_own(&base, &ev).unwrap();
    let expected = base
        .replace(
            "<description box=\"D\" iof-2004-ref=\"1.10\" />",
            "<description box=\"D\" iof-2004-ref=\"1.11\" />",
        )
        .replace("<name>Short</name>", "<name>Kurz</name>");
    assert_eq!(w.text, expected);
    assert!(
        w.text
            .contains("224.4232:330.1188</circle-gaps>\r\n    <foo a=\"1\" />\r\n  </control>")
    );
    assert!(w.text.contains("order=\"1\" x-new=\"2\">"));
    assert!(w.text.contains("<!-- between -->\r\n  <course id=\"1\""));
    assert_eq!(w.ids.courses, [1]);
    let back = read(&w.text);
    assert_eq!(back.courses[0].name, "Kurz");
}

#[test]
fn partial_flagging_and_map_issue_kept() {
    // A map issue point as Purple Pen writes one, with its symbol.
    let base = fixture("lincoln-park.ppen")
        .replace(
            "kind=\"start\">\r\n    <location x=\"140.207184\" y=\"57.44655\" />",
            "kind=\"map-issue\">\r\n    <location x=\"140.207184\" y=\"57.44655\" />\r\n    \
             <description box=\"all\" iof-2004-ref=\"13.6\" />",
        )
        .replace(
            "end-control=\"5\">\r\n    <bends>",
            "end-control=\"5\">\r\n    <flagging kind=\"beginning-part\" x=\"80\" y=\"70\" />\r\n    <bends>",
        );
    let mut ev = read(&base);
    assert!(
        ev.courses[0].legs[0].marked_route,
        "flagged from a map issue"
    );
    ev.courses[0].name = "Kurz".into();
    let w = patch_own(&base, &ev).unwrap();
    assert_eq!(
        w.text,
        base.replace("<name>Short</name>", "<name>Kurz</name>")
    );
    assert!(w.text.contains("<control id=\"1\" kind=\"map-issue\">"));
    assert!(w.text.contains(element(&base, "leg", 1)));
}

#[test]
fn foreign_ids_do_not_capture_objects() {
    let overlapper = fixture("overlapper.ppen");
    let base = read(&overlapper);
    let mut ev = read(&fixture("lincoln-park.ppen"));
    // A control whose code overlapper does not have, carrying the id of overlapper's control 36.
    let mut extra = control("99", 1.0, 2.0);
    extra.id = Some(7);
    ev.controls.push(extra);
    let w = patch(&overlapper, &ev);
    let out = read(&w.text);
    let codes = |e: &RawEvent| -> BTreeMap<u32, String> {
        e.controls
            .iter()
            .map(|c| (c.id.unwrap(), c.code.clone()))
            .collect()
    };
    let names = |e: &RawEvent| -> BTreeMap<u32, String> {
        e.courses
            .iter()
            .map(|c| (c.id.unwrap(), c.name.clone()))
            .collect()
    };
    let (base_codes, out_codes) = (codes(&base), codes(&out));
    for (id, code) in &base_codes {
        assert_eq!(out_codes.get(id), Some(code), "control id {id}");
    }
    let out_names = names(&out);
    for (id, name) in names(&base) {
        assert_eq!(out_names.get(&id), Some(&name), "course id {id}");
    }
    // Codes match codes; Lincoln Park's finish (id 5 there, control 34 here) is overlapper's finish; the new
    // control and the course get fresh ids.
    assert_eq!(w.ids.controls, [1, 2, 3, 11, 19]);
    assert_eq!(w.ids.courses, [4]);
    assert_eq!(out_codes.get(&19).map(String::as_str), Some("99"));
    let short = out.courses.iter().find(|c| c.name == "Short").unwrap();
    assert_eq!(
        short
            .stops
            .iter()
            .map(|s| s.control.as_str())
            .collect::<Vec<_>>(),
        ["STA1", "31", "32", "FIN1"]
    );
    course_controls_consistent(&w.text);
}

#[test]
fn added_course_gets_fresh_ids_and_order() {
    let f = fixture("sampleevent8.ppen");
    let mut ev = read(&f);
    ev.courses
        .push(course("Neu", &["STA1", "38", "47", "FIN1"], Some(10000)));
    let w = patch(&f, &ev);
    assert_eq!(w.ids.courses, [3, 4]);
    assert!(
        w.text
            .contains("<course id=\"4\" kind=\"normal\" order=\"4\">"),
        "{}",
        w.text
    );
    assert!(w.text.contains(element(&f, "course", 3)));
    for (id, control) in [(317, 1), (318, 38), (319, 47)] {
        assert!(
            w.text.contains(&format!(
                "<course-control id=\"{id}\" control=\"{control}\">"
            )),
            "{id}"
        );
    }
    assert!(
        w.text
            .contains("<course-control id=\"320\" control=\"2\" />")
    );
    course_controls_consistent(&w.text);
    let back = read(&w.text);
    let neu = back.courses.iter().find(|c| c.name == "Neu").unwrap();
    let added = ev.courses.last().unwrap();
    assert_eq!(neu.id, Some(4));
    assert_eq!(neu.stops, added.stops);
    assert_eq!(neu.print_scale_hint, added.print_scale_hint);
    assert_eq!(back.courses.len(), 2);
    // Everything before the new objects is the base file.
    let first_new = w.text.find("<course id=\"4\"").unwrap();
    assert_eq!(w.text[..first_new], f[..first_new]);
}

#[test]
fn shortened_course_deletes_its_course_controls() {
    let f = fixture("overlapper.ppen");
    let mut ev = read(&f);
    let c = ev
        .courses
        .iter_mut()
        .find(|c| c.name == "Course 1")
        .unwrap();
    let n = c.stops.len();
    c.stops.remove(5);
    c.legs.remove(5);
    let w = patch(&f, &ev);
    course_controls_consistent(&w.text);
    assert_eq!(
        ids_of(&w.text, "<course-control id=\"").len(),
        ids_of(&f, "<course-control id=\"").len() - 1
    );
    let back = read(&w.text);
    let c = back.courses.iter().find(|c| c.name == "Course 1").unwrap();
    assert_eq!(c.stops.len(), n - 1);
    let expected = ev.courses.iter().find(|c| c.name == "Course 1").unwrap();
    assert_eq!(c.stops, expected.stops);
    // The other courses are untouched.
    for id in [2, 3] {
        assert!(w.text.contains(element(&f, "course", id)));
    }
}

#[test]
fn deletions_cascade() {
    let end = "</course-scribe-event>";
    let base = fixture("overlapper.ppen")
        .replace(
            end,
            &format!(
                "  <special-object id=\"7\" kind=\"first-aid\">\r\n    <location x=\"1\" y=\"2\" />\r\n    \
                 <courses>\r\n      <course course=\"2\" />\r\n    </courses>\r\n  </special-object>\r\n  \
                 <special-object id=\"8\" kind=\"water\">\r\n    <location x=\"3\" y=\"4\" />\r\n    \
                 <courses>\r\n      <course course=\"2\" />\r\n      <course course=\"3\" />\r\n    \
                 </courses>\r\n  </special-object>\r\n{end}"
            ),
        )
        .replace(
            "  <special-object id=\"1\"",
            "  <leg id=\"1\" start-control=\"12\" end-control=\"13\">\r\n    <bends>\r\n      \
             <location x=\"5\" y=\"6\" />\r\n    </bends>\r\n  </leg>\r\n  <special-object id=\"1\"",
        );
    let course2_ccs = [13, 41, 15, 20, 22, 16, 17, 18, 19, 42, 23, 14];
    let mut ev = read(&base);
    ev.courses.retain(|c| c.name != "Course 2");
    ev.controls.retain(|c| c.code != "40");
    ev.specials.clear();
    let w = deleting(
        &base,
        &ev,
        Deletions {
            controls: vec![12],
            courses: vec![2],
            specials: Vec::new(),
        },
    )
    .unwrap();
    let out = &w.text;
    assert!(!out.contains("<course id=\"2\""));
    for id in course2_ccs {
        assert!(
            !out.contains(&format!("<course-control id=\"{id}\"")),
            "{id}"
        );
    }
    assert!(!out.contains("<control id=\"12\""));
    assert!(!out.contains("<leg "));
    assert!(!out.contains("<special-object id=\"7\""));
    assert!(out.contains(
        "  <special-object id=\"8\" kind=\"water\">\r\n    <location x=\"3\" y=\"4\" />\r\n    \
         <courses>\r\n      <course course=\"3\" />\r\n    </courses>\r\n  </special-object>"
    ));
    course_controls_consistent(out);
    assert_eq!(
        w.kept,
        [1, 5, 6, 8]
            .iter()
            .map(|&id| KeptObject::Special {
                id,
                kind: base_kind(id).into()
            })
            .collect::<Vec<_>>()
    );
    let back = read(out);
    assert_eq!(
        back.courses
            .iter()
            .map(|c| c.name.as_str())
            .collect::<Vec<_>>(),
        ["Course 1", "Score"]
    );

    // A control still used by a course of the output is not deleted.
    let mut ev = read(&base);
    ev.controls.retain(|c| c.code != "31");
    ev.courses.clear();
    ev.specials.clear();
    let used = deleting(
        &base,
        &ev,
        Deletions {
            controls: vec![2],
            ..Deletions::default()
        },
    );
    assert!(matches!(used, Err(PpenWriteError::Invalid(_))), "{used:?}");
    // Nor is a course the event still has.
    let ev = read(&base);
    let kept = deleting(
        &base,
        &ev,
        Deletions {
            courses: vec![1],
            ..Deletions::default()
        },
    );
    assert!(matches!(kept, Err(PpenWriteError::Invalid(_))), "{kept:?}");
}

fn base_kind(id: u32) -> &'static str {
    match id {
        1 | 8 => "water",
        _ => "forbidden-route",
    }
}

#[test]
fn old_gaps_attribute_materialised() {
    let base = "\u{FEFF}<course-scribe-event>\r\n  <event id=\"1\">\r\n    <map kind=\"none\" scale=\"10000\" />\r\n  \
                </event>\r\n  <control id=\"1\" kind=\"normal\" gaps=\"01111111111111111111111111111110\">\r\n    \
                <code>31</code>\r\n    <location x=\"0\" y=\"0\" />\r\n  </control>\r\n  \
                <course id=\"1\" kind=\"normal\" order=\"1\">\r\n    <name>A</name>\r\n    \
                <first course-control=\"1\" />\r\n    <options print-scale=\"7500\" />\r\n  </course>\r\n  \
                <course id=\"2\" kind=\"normal\" order=\"2\">\r\n    <name>B</name>\r\n    \
                <first course-control=\"2\" />\r\n  </course>\r\n  \
                <course id=\"3\" kind=\"normal\" order=\"3\">\r\n    <name>C</name>\r\n    \
                <first course-control=\"3\" />\r\n    <options print-scale=\"4000\" />\r\n  </course>\r\n  \
                <course-control id=\"1\" control=\"1\" />\r\n  <course-control id=\"2\" control=\"1\" />\r\n  \
                <course-control id=\"3\" control=\"1\" />\r\n</course-scribe-event>";
    let mut ev = read(base);
    let old = vec![(-11.25, 11.25)];
    for c in &ev.courses {
        assert_eq!(c.stops[0].circle_gaps, old, "{}", c.name);
    }
    ev.courses[0].stops[0].circle_gaps = vec![(10.0, 20.0)];
    let w = patch(base, &ev);
    assert!(!w.text.contains("gaps=\""), "{}", w.text);
    for (scale, gaps) in [
        (4000, "-11.25:11.25"),
        (7500, "10:20"),
        (10000, "-11.25:11.25"),
        (15000, "-11.25:11.25"),
    ] {
        assert!(
            w.text.contains(&format!(
                "<circle-gaps scale=\"{scale}\">{gaps}</circle-gaps>"
            )),
            "{scale}: {}",
            w.text
        );
    }
    let back = read(&w.text);
    let gaps: Vec<_> = back
        .courses
        .iter()
        .map(|c| (c.name.as_str(), c.stops[0].circle_gaps.clone()))
        .collect();
    assert_eq!(
        gaps,
        [
            ("A", vec![(10.0, 20.0)]),
            ("B", old.clone()),
            ("C", old.clone())
        ]
    );
}

#[test]
fn patch_keeps_lf_and_no_bom() {
    let base = fixture("overlapper.ppen")
        .trim_start_matches('\u{FEFF}')
        .replace("\r\n", "\n");
    let mut ev = read(&base);
    ev.controls.push(control("99", 1.0, 2.0));
    let w = patch(&base, &ev);
    assert!(!w.text.contains('\r'));
    assert!(!w.text.starts_with('\u{FEFF}'));
    let new = "  <control id=\"19\" kind=\"normal\">\n    <code>99</code>\n    \
               <location x=\"1\" y=\"2\" />\n  </control>\n";
    assert_eq!(
        w.text,
        base.replace("  <course id=\"1\"", &format!("{new}  <course id=\"1\""))
    );
    assert_eq!(w.ids.controls.last(), Some(&19));
}

#[test]
fn hostile_base_is_refused_cheaply() {
    let mut xml = String::from(
        "<course-scribe-event><event id=\"1\"><map kind=\"none\" scale=\"10000\" /></event>",
    );
    for i in 1..=MAX_CONTROLS {
        xml.push_str(&format!(
            "<control id=\"{i}\" kind=\"normal\"><code>{i}</code><location x=\"0\" y=\"0\" /></control>"
        ));
    }
    for c in 1..=1000 {
        xml.push_str(&format!(
            "<course id=\"{c}\" kind=\"normal\" order=\"{c}\"><name>{c}</name><first course-control=\"{}\" />\
             </course>",
            2 * c - 1,
        ));
    }
    for c in 1..=1000 {
        xml.push_str(&format!(
            "<course-control id=\"{}\" control=\"1\"><next course-control=\"{}\" /></course-control>\
             <course-control id=\"{}\" control=\"2\" />",
            2 * c - 1,
            2 * c,
            2 * c
        ));
    }
    xml.push_str("<leg id=\"1\" start-control=\"1\" end-control=\"2\"><bends>");
    for _ in 0..MAX_POINTS_PER_PATH {
        xml.push_str("<location x=\"1\" y=\"1\" />");
    }
    xml.push_str("</bends></leg></course-scribe-event>");
    let ev = read(&fixture("lincoln-park.ppen"));
    match write_ppen_patch(xml.as_bytes(), &ev, &PatchOptions::default()) {
        Err(PpenWriteError::Base(CourseError::Limit(_))) => {}
        other => panic!("{:?}", other.map(|w| w.text.len())),
    }
    let doctype = format!(
        "<!DOCTYPE x>{}",
        fixture("lincoln-park.ppen").trim_start_matches('\u{FEFF}')
    );
    assert!(write_ppen_patch(doctype.as_bytes(), &ev, &PatchOptions::default()).is_err());
    assert!(write_ppen_patch(b"", &ev, &PatchOptions::default()).is_err());
}

#[test]
fn map_exchange_flag_at_every_map_exchange() {
    // A new course-control at a map-exchange control carries the flag, as Purple Pen requires.
    let f = fixture("sampleevent8.ppen");
    let mut ev = read(&f);
    ev.courses
        .push(course("Neu", &["STA1", "XCHG1", "FIN1"], Some(10000)));
    let w = patch(&f, &ev);
    assert!(
        w.text
            .contains("<course-control id=\"318\" control=\"81\" map-exchange=\"true\">"),
        "{}",
        w.text
    );
    // A map flip there is one Purple Pen wrote: the patch with the event read from it gives it back.
    let flip = f.replace(
        "<course-control id=\"316\" control=\"81\" map-exchange=\"true\">",
        "<course-control id=\"316\" control=\"81\" map-exchange=\"true\" map-flip=\"true\">",
    );
    let ev = read(&flip);
    assert!(
        ev.courses[0]
            .stops
            .iter()
            .any(|s| s.map_flip && s.control == "XCHG1")
    );
    assert_eq!(patch(&flip, &ev).text, flip);
}

#[test]
fn crossing_point_gets_its_orientation() {
    // The start turned into a crossing point (matched by its id), and a new crossing point.
    let f = fixture("lincoln-park.ppen");
    let mut ev = read(&f);
    let c = &mut ev.controls[0];
    c.kind = ControlKind::CrossingPoint;
    c.description = None;
    let mut cross = control("X", 5.0, 6.0);
    cross.kind = ControlKind::CrossingPoint;
    ev.controls.push(cross);
    let w = patch_own(&f, &ev).unwrap();
    assert!(
        w.text
            .contains("<control id=\"1\" kind=\"crossing-point\" orientation=\"0\">"),
        "{}",
        w.text
    );
    assert!(
        w.text
            .contains("<control id=\"6\" kind=\"crossing-point\" orientation=\"0\">"),
        "{}",
        w.text
    );
    // A crossing point of the base keeps its own orientation.
    let o = fixture("overlapper.ppen");
    let mut ev = read(&o);
    let c = ev
        .controls
        .iter_mut()
        .find(|c| c.kind == ControlKind::CrossingPoint)
        .unwrap();
    c.position.as_mut().unwrap().x_mm += 1.0;
    let w = patch(&o, &ev);
    assert!(
        w.text
            .contains("<control id=\"17\" kind=\"crossing-point\" orientation=\"83.84482\">")
    );
}

#[test]
fn options_keep_a_print_scale() {
    let f = fixture("lincoln-park.ppen");
    let mut ev = read(&f);
    ev.courses[0].climb_m = Some(20.0);
    let w = patch(&f, &ev);
    assert!(
        w.text
            .contains("<options print-scale=\"5000\" climb=\"20\" description-kind=\"symbols\" />"),
        "{}",
        w.text
    );
    // Without a hint the course has Purple Pen's default, written instead of dropping the base's value.
    ev.courses[0].print_scale_hint = None;
    let w = patch(&f, &ev);
    assert!(
        w.text.contains(
            "<options print-scale=\"15000\" climb=\"20\" description-kind=\"symbols\" />"
        ),
        "{}",
        w.text
    );
}

/// Lincoln Park with the leg 32 to FIN1 flagged from its bend to the finish.
fn partly_flagged() -> String {
    fixture("lincoln-park.ppen").replace(
        "end-control=\"5\">\r\n    <bends>",
        "end-control=\"5\">\r\n    <flagging kind=\"end-part\" x=\"81.8017349\" y=\"67.87238\" />\r\n    \
         <bends>",
    )
}

#[test]
fn partial_flagging_follows_its_bend() {
    // The flagging point is still a bend: the flagging stays.
    let base = partly_flagged();
    let mut ev = read(&base);
    let leg = &mut ev.courses[0].legs[2];
    assert!(!leg.marked_route);
    leg.bends.push(RawPoint {
        x_mm: 90.0,
        y_mm: 60.0,
    });
    let w = patch(&base, &ev);
    assert!(
        w.text
            .contains("<flagging kind=\"end-part\" x=\"81.8017349\" y=\"67.87238\" />")
    );
    assert!(w.notes.is_empty());
    // It is not: Purple Pen would refuse the leg, so the flagging goes, with a note.
    let mut ev = read(&base);
    ev.courses[0].legs[2].bends[0].x_mm += 1.0;
    let w = patch(&base, &ev);
    assert!(!w.text.contains("<flagging"), "{}", w.text);
    assert_eq!(
        w.notes,
        [WriteNote::PartialFlaggingDropped {
            course: "Short".into(),
            from: "32".into(),
            to: "FIN1".into()
        }]
    );
    let bends = |e: &RawEvent| -> Vec<(f32, f32)> {
        e.courses[0].legs[2]
            .bends
            .iter()
            .map(|b| (b.x_mm as f32, b.y_mm as f32))
            .collect()
    };
    assert_eq!(bends(&read(&w.text)), bends(&ev));
}

#[test]
fn foreign_ids_never_count_by_default() {
    // Lincoln Park with other codes and names, starts and finishes without ids: nothing coincides with
    // overlapper, so only `ids_from_base` would let its ids match.
    let a = fixture("lincoln-park.ppen");
    let mut ev = read(&a);
    for c in &mut ev.controls {
        if c.kind == ControlKind::Control {
            c.code = format!("9{}", c.code);
        } else {
            c.id = None;
        }
    }
    for c in &mut ev.courses {
        for s in &mut c.stops {
            if !s.control.starts_with("STA") && !s.control.starts_with("FIN") {
                s.control = format!("9{}", s.control);
            }
        }
        c.name = format!("X-{}", c.name);
    }
    let b = fixture("overlapper.ppen");
    let before = read(&b);
    let w = patch(&b, &ev);
    let after = read(&w.text);
    for c in &before.controls {
        let out = after.controls.iter().find(|o| o.id == c.id).unwrap();
        assert_eq!(out.code, c.code, "control id {:?}", c.id);
    }
    for c in &before.courses {
        let out = after.courses.iter().find(|o| o.id == c.id).unwrap();
        assert_eq!(out.name, c.name, "course id {:?}", c.id);
    }
    assert!(after.courses.iter().any(|c| c.name == "X-Short"));
    course_controls_consistent(&w.text);
}

#[test]
fn rename_by_id_needs_ids_from_base() {
    let f = fixture("lincoln-park.ppen");
    let mut ev = read(&f);
    ev.courses[0].name = "Kurz".into();
    // By default the renamed course is a new one beside the old.
    let w = patch(&f, &ev);
    assert_eq!(
        w.kept,
        [KeptObject::Course {
            id: 1,
            name: "Short".into()
        }]
    );
    assert_eq!(w.ids.courses, [2]);
    // With the base's ids it is the same course, and the name keeps its attributes.
    let base = f.replace(
        "<name>Short</name>",
        "<name hide-variations-on-map=\"true\">Short</name>",
    );
    let w = patch_own(&base, &ev).unwrap();
    assert_eq!(w.text, base.replace(">Short</name>", ">Kurz</name>"));
    // A special object matches by id only with the same kind.
    let o = fixture("overlapper.ppen");
    let mut ev = read(&o);
    let water = ev
        .specials
        .iter()
        .position(|s| matches!(s, Special::Water { .. }))
        .unwrap();
    let id = ev.specials[water].id();
    ev.specials[water] = Special::FirstAid {
        id,
        pt: RawPoint {
            x_mm: 1.0,
            y_mm: 2.0,
        },
        courses: CourseFilter::All,
    };
    let w = patch_own(&o, &ev).unwrap();
    assert!(w.kept.contains(&KeptObject::Special {
        id: id.unwrap(),
        kind: "water".into()
    }));
    assert!(w.text.contains("kind=\"first-aid\""));
}

#[test]
fn map_issue_point_keeps_one_description_box() {
    let base = fixture("lincoln-park.ppen").replace("kind=\"start\"", "kind=\"map-issue\"");
    let mut ev = read(&base);
    let start = &mut ev.controls[0];
    assert_eq!(start.kind, ControlKind::Start);
    // Without a symbol in the event or the base, it gets Purple Pen's symbol for a map issue point.
    let w = patch(&base, &ev);
    assert!(
        w.text
            .contains("<description box=\"all\" iof-2004-ref=\"13.6\" />"),
        "{}",
        w.text
    );
    let start = &mut ev.controls[0];
    start.description = Some(freemapper_course::model::Description {
        columns: [Some("13.5".into()), None, None, None, None, None],
        standard: "2004".into(),
        column_f_text: None,
    });
    // 13.5 is no map issue symbol: Purple Pen fails on it once the file is open.
    assert!(matches!(
        write_ppen_patch(base.as_bytes(), &ev, &PatchOptions::default()),
        Err(PpenWriteError::Invalid(_))
    ));
    ev.controls[0].description.as_mut().unwrap().columns[0] = Some("13.6".into());
    let w = patch(&base, &ev);
    assert!(
        w.text
            .contains("<description box=\"all\" iof-2004-ref=\"13.6\" />"),
        "{}",
        w.text
    );
    ev.controls[0].description.as_mut().unwrap().columns[1] = Some("1.1".into());
    assert!(matches!(
        write_ppen_patch(base.as_bytes(), &ev, &PatchOptions::default()),
        Err(PpenWriteError::Unsupported(_))
    ));
}

#[test]
fn kept_courses_keep_their_legs_and_circle_gaps() {
    let f = fixture("overlapper.ppen");
    let full = read(&f);
    let codes = |name: &str| -> Vec<String> {
        full.courses
            .iter()
            .find(|c| c.name == name)
            .unwrap()
            .stops
            .iter()
            .map(|s| s.control.clone())
            .collect()
    };
    let (one, two) = (codes("Course 1"), codes("Course 2"));
    // A leg both courses run.
    let pairs = |c: &[String]| -> Vec<(String, String)> {
        c.windows(2).map(|w| (w[0].clone(), w[1].clone())).collect()
    };
    let shared = pairs(&one)
        .into_iter()
        .find(|p| pairs(&two).contains(p))
        .expect("a shared leg");
    let mut ev = full.clone();
    ev.courses.retain(|c| c.name == "Course 1");
    let c1 = &mut ev.courses[0];
    let i = pairs(&one).iter().position(|p| *p == shared).unwrap();
    while c1.legs.len() <= i {
        c1.legs.push(freemapper_course::Leg {
            bends: Vec::new(),
            gaps: Vec::new(),
            marked_route: false,
        });
    }
    c1.legs[i].bends.push(RawPoint {
        x_mm: 50.0,
        y_mm: 50.0,
    });
    match write_ppen_patch(f.as_bytes(), &ev, &PatchOptions::default()) {
        Err(PpenWriteError::SharedLeg {
            from,
            to,
            first,
            second,
        }) => {
            assert_eq!((from, to), shared);
            assert_eq!((first.as_str(), second.as_str()), ("Course 2", "Course 1"));
        }
        other => panic!("{:?}", other.map(|w| w.text)),
    }
    // Course 2 and Score both print at 1:10000: new circle gaps for Course 2 at a control Score visits
    // would change Score too.
    let score = codes("Score");
    let (si, code) = two
        .iter()
        .enumerate()
        .find(|(_, c)| score.contains(c) && !c.starts_with("STA") && !c.starts_with("FIN"))
        .map(|(i, c)| (i, c.clone()))
        .expect("a shared control");
    let mut ev = full.clone();
    ev.courses.retain(|c| c.name == "Course 2");
    ev.courses[0].stops[si].circle_gaps = vec![(10.0, 20.0)];
    match write_ppen_patch(f.as_bytes(), &ev, &PatchOptions::default()) {
        Err(PpenWriteError::SharedCircleGaps {
            control,
            scale,
            first,
            second,
        }) => {
            assert_eq!(control, code);
            assert_eq!(scale, 10000);
            assert_eq!((first.as_str(), second.as_str()), ("Score", "Course 2"));
        }
        other => panic!("{:?}", other.map(|w| w.text)),
    }
}

/// A base file with one normal control and the courses `courses`, each visiting it once, as
/// `(order, print scale)`.
fn crafted(control_attrs: &str, courses: &[(i64, u32)]) -> String {
    let mut xml = format!(
        "<course-scribe-event>\r\n  <event id=\"1\">\r\n    <map kind=\"none\" scale=\"10000\" />\r\n  </event>\r\n  \
         <control id=\"1\" kind=\"normal\"{control_attrs}>\r\n    <code>31</code>\r\n    \
         <location x=\"0\" y=\"0\" />\r\n  </control>\r\n"
    );
    for (i, (order, scale)) in courses.iter().enumerate() {
        let id = i + 1;
        xml.push_str(&format!(
            "  <course id=\"{id}\" kind=\"normal\" order=\"{order}\">\r\n    <name>C{id}</name>\r\n    \
             <first course-control=\"{id}\" />\r\n    <options print-scale=\"{scale}\" />\r\n  </course>\r\n"
        ));
    }
    for i in 1..=courses.len() {
        xml.push_str(&format!(
            "  <course-control id=\"{i}\" control=\"1\" />\r\n"
        ));
    }
    xml.push_str("</course-scribe-event>");
    xml
}

#[test]
fn circle_gap_scales_are_capped() {
    // Old gaps at a control visited by 300 courses at 300 print scales: changing them for one course
    // would write them out for every scale.
    let courses: Vec<(i64, u32)> = (0..300).map(|i| (i + 1, 1000 + i as u32)).collect();
    let base = crafted(" gaps=\"01111111111111111111111111111110\"", &courses);
    let mut ev = read(&base);
    ev.courses.truncate(1);
    ev.courses[0].stops[0].circle_gaps = vec![(10.0, 20.0)];
    match write_ppen_patch(base.as_bytes(), &ev, &PatchOptions::default()) {
        Err(PpenWriteError::Limit(m)) => assert!(m.contains("scales"), "{m}"),
        other => panic!("{:?}", other.map(|w| w.text.len())),
    }
}

#[test]
fn course_order_cannot_overflow() {
    for order in [i64::from(i32::MAX), i64::MAX] {
        let base = crafted("", &[(order, 10000)]);
        let mut ev = read(&base);
        ev.courses.push(course("Neu", &["31"], None));
        match write_ppen_patch(base.as_bytes(), &ev, &PatchOptions::default()) {
            Err(PpenWriteError::Limit(m)) => assert!(m.contains("order"), "{m}"),
            other => panic!("{order}: {:?}", other.map(|w| w.text)),
        }
    }
}

#[test]
fn growing_past_the_size_limit_is_refused() {
    // A base just under the limit (a long comment), and an event that adds 4 MB of special objects.
    let lincoln = fixture("lincoln-park.ppen");
    let pad = MAX_COURSE_FILE_BYTES - lincoln.len() - 100;
    let base = lincoln.replace(
        "</course-scribe-event>",
        &format!("<!--{}-->\r\n</course-scribe-event>", "x".repeat(pad)),
    );
    let mut ev = read(&base);
    let path: Vec<RawPoint> = (0..MAX_POINTS_PER_PATH)
        .map(|i| RawPoint {
            x_mm: 100.0 + i as f64 / 1000.0,
            y_mm: 200.0 + i as f64 / 1000.0,
        })
        .collect();
    for _ in 0..8 {
        ev.specials.push(Special::Boundary {
            id: None,
            path: path.clone(),
            courses: CourseFilter::All,
        });
    }
    match write_ppen_patch(base.as_bytes(), &ev, &PatchOptions::default()) {
        Err(PpenWriteError::Limit(m)) => assert!(m.contains("MiB"), "{m}"),
        other => panic!("{:?}", other.map(|w| w.text.len())),
    }
}

/// `overlapper.ppen` with a first-aid post (course 2) and a water point (courses 2 and 3) as special objects 7 and 8.
fn with_two_specials() -> String {
    let end = "</course-scribe-event>";
    fixture("overlapper.ppen").replace(
        end,
        &format!(
            "  <special-object id=\"7\" kind=\"first-aid\">\r\n    <location x=\"1\" y=\"2\" />\r\n    \
             <courses>\r\n      <course course=\"2\" />\r\n    </courses>\r\n  </special-object>\r\n  \
             <special-object id=\"8\" kind=\"water\">\r\n    <location x=\"3\" y=\"4\" />\r\n    \
             <courses>\r\n      <course course=\"2\" />\r\n      <course course=\"3\" />\r\n    \
             </courses>\r\n  </special-object>\r\n{end}"
        ),
    )
}

fn special_ids(text: &str) -> Vec<i64> {
    ids_of(text, "<special-object id=\"")
}

#[test]
fn deleting_an_object_the_base_does_not_have_is_refused() {
    let base = fixture("overlapper.ppen");
    let ev = read(&base);
    for (delete, what) in [
        (
            Deletions {
                controls: vec![999],
                ..Deletions::default()
            },
            "control id 999",
        ),
        (
            Deletions {
                courses: vec![998],
                ..Deletions::default()
            },
            "course id 998",
        ),
        (
            Deletions {
                specials: vec![997],
                ..Deletions::default()
            },
            "special object id 997",
        ),
    ] {
        match deleting(&base, &ev, delete) {
            Err(PpenWriteError::Invalid(m)) => assert!(m.contains(what), "{m}"),
            other => panic!("{what}: {:?}", other.map(|w| w.text)),
        }
    }
}

#[test]
fn a_special_object_goes_only_when_asked_and_not_while_the_event_has_it() {
    let base = with_two_specials();
    let full = read(&base);
    assert_eq!(full.specials.len(), 5);
    // The event has lost special 8 but nothing asks for its deletion: the base object stays.
    let mut ev = full.clone();
    ev.specials.retain(|s| !matches!(s, Special::Water { .. }));
    let kept = patch(&base, &ev);
    assert!(special_ids(&kept.text).contains(&8));
    // Asking for it deletes exactly that object and the output reads without it.
    let w = deleting(
        &base,
        &ev,
        Deletions {
            specials: vec![8],
            ..Deletions::default()
        },
    )
    .unwrap();
    let before = special_ids(&base);
    let after = special_ids(&w.text);
    assert_eq!(
        after,
        before
            .iter()
            .copied()
            .filter(|&i| i != 8)
            .collect::<Vec<_>>()
    );
    assert!(
        !w.kept
            .iter()
            .any(|k| matches!(k, KeptObject::Special { id: 8, .. }))
    );
    // The event still has the special it is asked to delete: refused.
    let refused = deleting(
        &base,
        &full,
        Deletions {
            specials: vec![8],
            ..Deletions::default()
        },
    );
    match refused {
        Err(PpenWriteError::Invalid(m)) => assert!(m.contains("id 8"), "{m}"),
        other => panic!("{:?}", other.map(|w| w.text)),
    }
}

#[test]
fn a_changed_course_list_or_position_rewrites_only_that_special_object() {
    let base = with_two_specials();
    let mut ev = read(&base);
    for s in &mut ev.specials {
        match s {
            // Course list from "course 2" to all courses.
            Special::FirstAid { courses, .. } => *courses = CourseFilter::All,
            // Moved by 1 mm.
            Special::Water {
                id: Some(8), pt, ..
            } => pt.x_mm += 1.0,
            _ => {}
        }
    }
    let w = patch_own(&base, &ev).unwrap();
    let back = read(&w.text);
    assert_eq!(back.specials, ev.specials);
    // The other special objects keep their bytes.
    for id in [1, 5, 6] {
        assert!(
            w.text.contains(element(&base, "special-object", id)),
            "{id}"
        );
    }
    assert!(!w.text.contains(element(&base, "special-object", 7)));
    assert!(!w.text.contains(element(&base, "special-object", 8)));
    // The ids stay.
    assert_eq!(special_ids(&w.text), special_ids(&base));
}

#[test]
fn a_special_object_of_another_kind_is_a_new_object_and_the_old_one_stays() {
    let base = with_two_specials();
    let mut ev = read(&base);
    let at = ev
        .specials
        .iter()
        .position(|s| matches!(s, Special::FirstAid { .. }))
        .unwrap();
    let Special::FirstAid { id, pt, courses } = ev.specials[at].clone() else {
        unreachable!()
    };
    ev.specials[at] = Special::Water { id, pt, courses };
    let w = patch_own(&base, &ev).unwrap();
    // The id of the first-aid post does not carry over to a water point: the base object is kept as it is.
    assert!(w.text.contains(element(&base, "special-object", 7)));
    assert_eq!(special_ids(&w.text).len(), special_ids(&base).len() + 1);
    assert!(w.kept.contains(&KeptObject::Special {
        id: 7,
        kind: "first-aid".into()
    }));
}

#[test]
fn special_objects_purple_pen_lacks_cannot_be_written() {
    let base = fixture("lincoln-park.ppen");
    let p = |x: f64, y: f64| RawPoint { x_mm: x, y_mm: y };
    for (special, what) in [
        (
            Special::Text {
                id: None,
                pt: p(10.0, 20.0),
                text: "Start here".into(),
                size_mm: 3.0,
                courses: CourseFilter::All,
            },
            "text",
        ),
        (
            Special::MarkedRoute {
                id: None,
                path: vec![p(1.0, 1.0), p(5.0, 5.0)],
                courses: CourseFilter::All,
            },
            "marked route",
        ),
    ] {
        let mut ev = read(&base);
        ev.specials.push(special);
        match write_ppen_patch(base.as_bytes(), &ev, &PatchOptions::default()) {
            Err(PpenWriteError::Unsupported(m)) => assert!(m.contains(what), "{m}"),
            other => panic!("{what}: {:?}", other.map(|w| w.text)),
        }
    }
}

#[test]
fn courses_in_a_new_order_get_new_order_numbers() {
    let base = fixture("overlapper.ppen");
    let mut ev = read(&base);
    let names: Vec<String> = ev.courses.iter().map(|c| c.name.clone()).collect();
    assert_eq!(names, ["Course 1", "Course 2", "Score"]);
    ev.courses.reverse();
    let w = patch(&base, &ev);
    let back = read(&w.text);
    let order: Vec<&str> = back.courses.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(order, ["Score", "Course 2", "Course 1"]);
    assert!(element(&w.text, "course", 3).contains("order=\"1\""));
    assert!(element(&w.text, "course", 1).contains("order=\"3\""));
    // Ids stay with their courses.
    assert_eq!(back.courses[2].id, Some(1));
}

#[test]
fn a_new_course_goes_after_the_kept_ones_when_the_event_drops_one() {
    let base = fixture("overlapper.ppen");
    let mut ev = read(&base);
    // Course 2 is not in the event and not deleted: it stays, and the new course is numbered after it.
    ev.courses.retain(|c| c.name != "Course 2");
    ev.courses.push(course("Neu", &["31"], None));
    let w = patch(&base, &ev);
    let back = read(&w.text);
    let names: Vec<&str> = back.courses.iter().map(|c| c.name.as_str()).collect();
    assert!(
        names.contains(&"Course 2") && names.contains(&"Neu"),
        "{names:?}"
    );
    assert_eq!(names.last(), Some(&"Neu"));
    assert!(w.text.contains(element(&base, "course", 2)));
}

#[test]
fn bends_gaps_and_marking_of_a_leg_are_rewritten_and_read_back() {
    let base = fixture("lincoln-park.ppen");
    let mut ev = read(&base);
    let c = &mut ev.courses[0];
    // Leg 31 -> 32 has no leg object yet; leg 32 -> finish has a bend and a gap.
    c.legs[1].bends = vec![RawPoint {
        x_mm: 100.0,
        y_mm: 100.0,
    }];
    c.legs[1].gaps = vec![(1.0, 2.0)];
    c.legs[2].bends.push(RawPoint {
        x_mm: 90.0,
        y_mm: 60.0,
    });
    c.legs[2].gaps.clear();
    c.legs[0].marked_route = true;
    let w = patch(&base, &ev);
    let back = read(&w.text);
    let (legs, expected) = (&back.courses[0].legs, &ev.courses[0].legs);
    for i in 0..3 {
        // Written as f32: the seventh digit of an unchanged bend may move.
        assert_eq!(
            legs[i].bends.len(),
            expected[i].bends.len(),
            "bends of leg {i}"
        );
        for (a, b) in legs[i].bends.iter().zip(&expected[i].bends) {
            assert!((a.x_mm - b.x_mm).abs() < 1e-5 && (a.y_mm - b.y_mm).abs() < 1e-5);
        }
        assert_eq!(legs[i].gaps, expected[i].gaps, "gaps of leg {i}");
        assert_eq!(legs[i].marked_route, expected[i].marked_route, "marked {i}");
    }
    // Two legs were new, one changed: three leg objects.
    assert_eq!(ids_of(&w.text, "<leg id=\"").len(), 3);
    // Untouched parts of the file are the base's.
    assert!(w.text.starts_with(&base[..base.find("  <leg ").unwrap()]));
}

#[test]
fn two_courses_cannot_draw_one_leg_differently() {
    let base = fixture("overlapper.ppen");
    let ev = read(&base);
    // Course 1 and Course 2 both run from the same control to the same next control somewhere; find a pair.
    let pair = |c: &Course<RawPoint>| -> Vec<(String, String)> {
        c.stops
            .windows(2)
            .map(|w| (w[0].control.clone(), w[1].control.clone()))
            .collect()
    };
    let (one, two) = (pair(&ev.courses[0]), pair(&ev.courses[1]));
    let (i, j) = (0..one.len())
        .flat_map(|i| (0..two.len()).map(move |j| (i, j)))
        .find(|&(i, j)| one[i] == two[j])
        .expect("the courses of overlapper.ppen share a leg");
    // Only Course 1 is in the event and bends the shared leg: Course 2, kept as it is, would change too.
    let mut ev = ev;
    ev.courses.truncate(1);
    ev.courses[0].legs[i].bends = vec![RawPoint {
        x_mm: 3.0,
        y_mm: 4.0,
    }];
    let _ = j;
    match write_ppen_patch(base.as_bytes(), &ev, &PatchOptions::default()) {
        Err(PpenWriteError::SharedLeg { first, second, .. }) => {
            assert_eq!((first.as_str(), second.as_str()), ("Course 2", "Course 1"));
        }
        other => panic!("{:?}", other.map(|w| w.text)),
    }
}

#[test]
fn an_empty_root_element_cannot_take_new_content() {
    let base = "<course-scribe-event />";
    let mut ev = read(base);
    assert_eq!(patch(base, &ev).text, base);
    ev.controls.push(control("31", 1.0, 2.0));
    match write_ppen_patch(base.as_bytes(), &ev, &PatchOptions::default()) {
        Err(PpenWriteError::Unsupported(m)) => assert!(m.contains("empty root"), "{m}"),
        other => panic!("{:?}", other.map(|w| w.text)),
    }
}

#[test]
fn a_changed_circle_gap_is_written_for_the_course_scale_and_read_back() {
    let base = fixture("lincoln-park.ppen");
    let mut ev = read(&base);
    ev.courses[0].stops[1].circle_gaps = vec![(10.0, 40.0)];
    ev.courses[0].stops[2].circle_gaps.clear();
    let w = patch(&base, &ev);
    let back = read(&w.text);
    assert_eq!(back.courses[0].stops[1].circle_gaps, vec![(10.0, 40.0)]);
    assert!(back.courses[0].stops[2].circle_gaps.is_empty());
    // The finish keeps its gap.
    assert_eq!(
        back.courses[0].stops[3].circle_gaps,
        ev.courses[0].stops[3].circle_gaps
    );
}

#[test]
fn new_controls_of_the_kinds_purple_pen_has_are_written_and_one_it_lacks_is_refused() {
    let base = fixture("lincoln-park.ppen");
    let with = |code: &str, kind: ControlKind| {
        let mut ev = read(&base);
        let mut c = control(code, 12.0, 13.0);
        c.kind = kind;
        ev.controls.push(c);
        write_ppen_patch(base.as_bytes(), &ev, &PatchOptions::default())
    };
    for kind in [
        ControlKind::CrossingPoint,
        ControlKind::Start,
        ControlKind::Finish,
        ControlKind::Control,
    ] {
        let w = with("Q9", kind).unwrap();
        let back = read(&w.text);
        assert_eq!(back.controls.len(), 5, "{kind:?}");
        assert_eq!(back.controls.last().unwrap().kind, kind, "{kind:?}");
    }
    match with("E1", ControlKind::EndOfMarkedRoute) {
        Err(PpenWriteError::Unsupported(m)) => assert!(m.contains("E1"), "{m}"),
        other => panic!("{:?}", other.map(|w| w.text)),
    }
}

#[test]
fn a_new_course_in_front_of_kept_ones_renumbers_all_orders() {
    let base = fixture("overlapper.ppen");
    let mut ev = read(&base);
    ev.courses.insert(0, course("Neu", &["31"], None));
    let w = patch(&base, &ev);
    let back = read(&w.text);
    let names: Vec<&str> = back.courses.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["Neu", "Course 1", "Course 2", "Score"]);
    // The kept courses moved up by one, and kept their ids.
    assert!(element(&w.text, "course", 1).contains("order=\"2\""));
    assert!(element(&w.text, "course", 3).contains("order=\"4\""));
}

#[test]
fn a_control_the_event_has_is_not_deleted() {
    let base = fixture("overlapper.ppen");
    let ev = read(&base);
    let refused = deleting(
        &base,
        &ev,
        Deletions {
            controls: vec![2],
            ..Deletions::default()
        },
    );
    match refused {
        Err(PpenWriteError::Invalid(m)) => assert!(m.contains("the event has it"), "{m}"),
        other => panic!("{:?}", other.map(|w| w.text)),
    }
}

#[test]
fn a_course_with_variations_is_not_deleted() {
    let base = crafted("", &[(1, 10000), (2, 10000)]).replace(
        "<course-control id=\"2\" control=\"1\" />",
        "<course-control id=\"2\" control=\"1\" variation=\"A\" />",
    );
    let mut ev = read(&base);
    // The reader leaves the course with the variation out; the patch must not delete what it cannot see whole.
    assert_eq!(ev.courses.len(), 1);
    ev.courses.truncate(1);
    match deleting(
        &base,
        &ev,
        Deletions {
            courses: vec![2],
            ..Deletions::default()
        },
    ) {
        Err(PpenWriteError::Unsupported(m)) => assert!(m.contains("variations"), "{m}"),
        other => panic!("{:?}", other.map(|w| w.text)),
    }
}
