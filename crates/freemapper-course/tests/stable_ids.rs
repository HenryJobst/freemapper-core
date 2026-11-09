//! Stable ids: the `.ppen` reader keeps Purple Pen's own ids of controls and courses, so that a writer can
//! refer to the same objects again; IOF XML has none.

use std::collections::HashMap;

use freemapper_course::{read_iof_xml, read_ppen};

fn fixture(path: &str) -> Vec<u8> {
    std::fs::read(format!(
        "{}/tests/fixtures/courses/{path}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

/// The value of `id="…"` in the start tag that begins `tag`.
fn id_of(tag: &str) -> u32 {
    let rest = &tag[tag.find("id=\"").unwrap() + 4..];
    rest[..rest.find('"').unwrap()].parse().unwrap()
}

/// The ids of the `<control>` elements in file order, and the id of each `<course>` by its name, found by a
/// plain text scan independent of the reader.
fn ids_in_file(text: &str) -> (Vec<u32>, HashMap<String, u32>) {
    let controls = text
        .match_indices("<control id=")
        .map(|(i, _)| id_of(&text[i..]))
        .collect();
    let mut courses = HashMap::new();
    for (i, _) in text.match_indices("<course id=") {
        let rest = &text[i..];
        let name = &rest[rest.find("<name>").unwrap() + 6..rest.find("</name>").unwrap()];
        courses.insert(name.to_string(), id_of(rest));
    }
    (controls, courses)
}

#[test]
fn reader_fills_stable_ids() {
    for name in ["lincoln-park.ppen", "overlapper.ppen", "sampleevent8.ppen"] {
        let bytes = fixture(&format!("ppen/{name}"));
        let (controls, courses) = ids_in_file(&String::from_utf8_lossy(&bytes));
        let ev = read_ppen(&bytes).unwrap();

        // Controls stay in file order, so the n-th control carries the n-th id.
        let read: Vec<Option<u32>> = ev.controls.iter().map(|c| c.id).collect();
        let expected: Vec<Option<u32>> = controls.into_iter().map(Some).collect();
        assert_eq!(read, expected, "{name}");

        // Courses are sorted by `order` and variation courses are left out: compare by name.
        assert!(!ev.courses.is_empty(), "{name}");
        for c in &ev.courses {
            assert_eq!(c.id, Some(courses[&c.name]), "{name}: course {}", c.name);
        }
    }
}

#[test]
fn ids_need_not_be_consecutive() {
    // Lincoln Park has no control with id 4: the id is Purple Pen's, not a position in the list.
    let ev = read_ppen(&fixture("ppen/lincoln-park.ppen")).unwrap();
    let ids: Vec<Option<u32>> = ev.controls.iter().map(|c| c.id).collect();
    assert_eq!(ids, [Some(1), Some(2), Some(3), Some(5)]);
}

#[test]
fn an_id_outside_u32_reads_as_none() {
    let file = r#"<course-scribe-event>
  <event id="1"><title>T</title></event>
  <control id="-3" kind="start"><location x="0" y="0" /></control>
  <control id="4294967296" kind="normal"><code>31</code><location x="10" y="0" /></control>
  <control id="7" kind="finish"><location x="20" y="0" /></control>
  <course id="-1" kind="normal" order="1"><name>A</name><first course-control="1" /></course>
  <course id="9" kind="normal" order="2"><name>B</name><first course-control="1" /></course>
  <course-control id="1" control="-3"><next course-control="2" /></course-control>
  <course-control id="2" control="4294967296"><next course-control="3" /></course-control>
  <course-control id="3" control="7" />
</course-scribe-event>"#;
    let ev = read_ppen(file.as_bytes()).unwrap();
    let ids: Vec<Option<u32>> = ev.controls.iter().map(|c| c.id).collect();
    assert_eq!(ids, [None, None, Some(7)]);
    let ids: Vec<Option<u32>> = ev.courses.iter().map(|c| c.id).collect();
    assert_eq!(ids, [None, Some(9)]);
}

#[test]
fn iof_xml_has_no_ids() {
    for name in ["y-up.xml", "text-position.xml", "forked.xml"] {
        let ev = read_iof_xml(&fixture(name)).unwrap();
        assert!(!ev.controls.is_empty() && !ev.courses.is_empty(), "{name}");
        assert!(ev.controls.iter().all(|c| c.id.is_none()), "{name}");
        assert!(ev.courses.iter().all(|c| c.id.is_none()), "{name}");
    }
}

#[test]
fn fitting_keeps_the_ids() {
    let ev = read_ppen(&fixture("ppen/lincoln-park.ppen")).unwrap();
    let before: Vec<_> = ev.controls.iter().map(|c| c.id).collect();
    let courses: Vec<_> = ev.courses.iter().map(|c| c.id).collect();
    let moved = ev.map_points(|p| p, |o| o);
    assert_eq!(
        moved.controls.iter().map(|c| c.id).collect::<Vec<_>>(),
        before
    );
    assert_eq!(
        moved.courses.iter().map(|c| c.id).collect::<Vec<_>>(),
        courses
    );
}
