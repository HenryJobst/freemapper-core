use super::db::{MAX_DEPTH, MAX_POINTS_PER_SYMBOL, Pt, SYMBOLS_XML, db, parse, seg_dist};
use super::{FLATNESS, Standard, TextLang, exists, symbol, symbol_kind, symbol_text};

fn close(a: Pt, b: Pt) -> bool {
    (a.0 - b.0).abs() < 1e-9 && (a.1 - b.1).abs() < 1e-9
}

#[test]
fn all_symbols_parse() {
    let expected = SYMBOLS_XML.matches("<symbol ").count();
    assert_eq!(expected, 201);
    let db = db().expect("embedded symbol file parses");
    assert_eq!(db.entries.len(), expected);
}

#[test]
fn known_symbol_geometry() {
    // 1.1 terrace: two beziers of thickness 12.5; the first runs from (-60,-70) via (-50,-30), (-50,30) to
    // (-60,70) in Purple Pen's box (-100..100, y up).
    let g = symbol("1.1", Standard::Iof2018).expect("1.1 exists");
    assert_eq!(g.kind, 'D');
    assert_eq!(g.width, 1.0);
    assert!(g.fills.is_empty() && g.circles.is_empty());
    assert_eq!(g.strokes.len(), 2);
    let s = &g.strokes[0];
    assert!((s.width - 0.0625).abs() < 1e-12);
    assert!(!s.closed && s.round_ends && s.round_corners);
    assert!(close(s.points[0], (0.2, 0.85)), "{:?}", s.points[0]);
    assert!(close(*s.points.last().unwrap(), (0.2, 0.15)));
    // Every point of the curve lies within the flattening tolerance of the polyline.
    let curve = |t: f64| {
        let (p0, p1, p2, p3) = ((0.2, 0.85), (0.25, 0.65), (0.25, 0.35), (0.2, 0.15));
        let u = 1.0 - t;
        let b = |a: f64, b: f64, c: f64, d: f64| {
            u * u * u * a + 3.0 * u * u * t * b + 3.0 * u * t * t * c + t * t * t * d
        };
        (b(p0.0, p1.0, p2.0, p3.0), b(p0.1, p1.1, p2.1, p3.1))
    };
    for i in 0..=100 {
        let p = curve(i as f64 / 100.0);
        let d = s
            .points
            .windows(2)
            .map(|w| seg_dist(p, w[0], w[1]))
            .fold(f64::INFINITY, f64::min);
        assert!(d <= FLATNESS + 1e-9, "t={i}%: {d}");
    }
    assert!(s.points.len() <= 32, "{} points", s.points.len());

    // 0.1N, northern: a straight shaft and a sharp-cornered arrow head.
    let n = symbol("0.1N", Standard::Iof2004).expect("0.1N exists");
    assert_eq!(n.strokes[0].points, vec![(0.5, 0.825), (0.5, 0.175)]);
    assert!(!n.strokes[1].round_corners && n.strokes[1].round_ends);

    // 1.10 knoll differs by standard: a disc of radius 23 (2004) or 30 (2018).
    let r = |s| symbol("1.10", s).expect("1.10").circles[0].clone();
    assert_eq!(r(Standard::Iof2004).radius, 23.0 / 200.0);
    assert_eq!(r(Standard::Iof2018).radius, 30.0 / 200.0);
    assert_eq!(r(Standard::Iof2024).radius, 30.0 / 200.0);
    assert_eq!(r(Standard::Iof2018).stroke, None);
    assert_eq!(r(Standard::Iof2018).centre, (0.5, 0.5));
    // 11.7 bend is column G in 2004 and column F in 2018; 12.3 radio exists only in 2004, 5.25 only from
    // 2018.
    assert_eq!(symbol("11.7", Standard::Iof2004).map(|g| g.kind), Some('G'));
    assert_eq!(symbol("11.7", Standard::Iof2018).map(|g| g.kind), Some('F'));
    assert!(symbol("12.3", Standard::Iof2004).is_some());
    assert!(symbol("12.3", Standard::Iof2018).is_none());
    assert!(symbol("5.25", Standard::Iof2004).is_none());
    assert!(symbol("5.25", Standard::Iof2024).is_some());
    assert!(symbol("no-such-id", Standard::Iof2018).is_none());

    // 13.1, an instruction row, is eight cells wide; the circle is at x = -700.
    let w = symbol("13.1", Standard::Iof2018).expect("13.1");
    assert_eq!(w.width, 8.0);
    assert_eq!(w.circles[0].centre, (0.5, 0.5));
    assert_eq!(w.circles[0].stroke, Some(0.05));

    // Filled parts stay apart from strokes: 5.25 has one bezier outline, one disc and one polygon.
    let f = symbol("5.25", Standard::Iof2018).expect("5.25");
    assert_eq!(f.strokes.len(), 2);
    assert!(f.strokes[1].closed);
    assert_eq!(f.circles.len(), 1);
    assert!(f.fills.is_empty());
}

#[test]
fn every_symbol_stays_in_its_box_and_under_the_cap() {
    let db = db().expect("parses");
    for e in &db.entries {
        let g = &e.geometry;
        let pts: Vec<Pt> = g
            .strokes
            .iter()
            .flat_map(|s| s.points.iter().copied())
            .chain(g.fills.iter().flatten().copied())
            .chain(g.circles.iter().map(|c| c.centre))
            .collect();
        assert!(pts.len() <= MAX_POINTS_PER_SYMBOL, "{}", e.id);
        for p in pts {
            assert!(
                (-0.01..=g.width + 0.01).contains(&p.0) && (-0.01..=1.01).contains(&p.1),
                "{} {:?}",
                e.id,
                p
            );
        }
    }
}

#[test]
fn standards_from_the_file() {
    assert_eq!(Standard::from_description("2004"), Some(Standard::Iof2004));
    assert_eq!(
        Standard::from_description(" 2018 "),
        Some(Standard::Iof2018)
    );
    assert_eq!(Standard::from_description("2024"), Some(Standard::Iof2024));
    assert_eq!(Standard::from_description("2000"), None);
}

#[test]
fn texts_in_de_and_en() {
    assert_eq!(symbol_text("1.1", TextLang::De), Some("Terrasse"));
    assert_eq!(symbol_text("1.1", TextLang::En), Some("terrace"));
    // Placeholders stay for the description box to fill.
    assert_eq!(symbol_text("11.7", TextLang::De), Some("{0}, Biegung"));
    assert_eq!(symbol_text("11.7", TextLang::En), Some("bend in {0}"));
    // Singular before plural even when the plural comes first in the file; the first gender wins.
    assert_eq!(
        symbol_text("5.25", TextLang::De),
        Some("Blumenbeet / Verbotenes Gebiet")
    );
    assert_eq!(symbol_text("8.1", TextLang::De), Some("niedriges {0}"));
    assert_eq!(
        symbol_text("course_length_climb", TextLang::En),
        Some("Length {0}, climb {1}")
    );
    assert_eq!(symbol_text("no-such-id", TextLang::De), None);
    // Every symbol has both texts, and versions of one id share them, so the text needs no standard.
    let db = db().expect("parses");
    for e in &db.entries {
        assert!(e.text_de.is_some() && e.text_en.is_some(), "{}", e.id);
        for other in db.entries.iter().filter(|o| o.id == e.id) {
            assert_eq!(other.text_de, e.text_de, "{}", e.id);
            assert_eq!(other.text_en, e.text_en, "{}", e.id);
        }
    }
}

#[test]
fn parser_is_bounded() {
    // Only the embedded file is ever read; these feed the parser hostile variants directly.
    let doctype = "<?xml version=\"1.0\"?><!DOCTYPE symbols [<!ENTITY a \"b\">]><symbols/>";
    assert!(parse(doctype).unwrap_err().contains("DOCTYPE"));
    let deep = format!(
        "<symbols><symbol kind=\"D\" id=\"x\"><text lang=\"en\">x</text>{}{}</symbol></symbols>",
        "<a>".repeat(MAX_DEPTH),
        "</a>".repeat(MAX_DEPTH)
    );
    assert!(parse(&deep).unwrap_err().contains("nesting"));
    let entity =
        "<symbols><symbol kind=\"D\" id=\"x\"><text lang=\"en\">&e;</text></symbol></symbols>";
    assert!(parse(entity).is_err());
    let not_a_number = "<symbols><symbol kind=\"D\" id=\"x\"><text lang=\"en\">x</text>\
        <lines thickness=\"NaN\"><point x=\"0\" y=\"0\"/><point x=\"1\" y=\"1\"/></lines></symbol></symbols>";
    assert!(parse(not_a_number).is_err());
    let short_bezier = "<symbols><symbol kind=\"D\" id=\"x\"><text lang=\"en\">x</text>\
        <beziers thickness=\"1\"><point x=\"0\" y=\"0\"/><point x=\"1\" y=\"1\"/></beziers></symbol></symbols>";
    assert!(parse(short_bezier).is_err());
    let far = "<symbols><symbol kind=\"D\" id=\"x\"><text lang=\"en\">x</text>\
        <filled-circle radius=\"5\"><point x=\"1e9\" y=\"0\"/></filled-circle></symbol></symbols>";
    assert!(parse(far).is_err());
    let many_points = format!(
        "<symbols><symbol kind=\"D\" id=\"x\"><text lang=\"en\">x</text><lines thickness=\"1\">{}</lines>\
         </symbol></symbols>",
        "<point x=\"0\" y=\"0\"/>".repeat(MAX_POINTS_PER_SYMBOL + 1)
    );
    assert!(parse(&many_points).unwrap_err().contains("points"));
    let no_text = "<symbols><symbol kind=\"D\" id=\"x\"/></symbols>";
    assert!(parse(no_text).is_err());
    let wrong_root = "<course-scribe-event/>";
    assert!(parse(wrong_root).is_err());
}

#[test]
fn kinds_from_the_file() {
    assert_eq!(symbol_kind("11.7", Standard::Iof2004), Some('G'));
    assert_eq!(symbol_kind("11.7", Standard::Iof2018), Some('F'));
    assert_eq!(symbol_kind("10.1single", Standard::Iof2018), Some('F'));
    assert_eq!(symbol_kind("no-such-id", Standard::Iof2018), None);
    assert!(!exists("2.10", Standard::Iof2004));
    assert!(exists("2.10", Standard::Iof2018));
    assert!(!exists("0.1NE", Standard::Iof2018));
}
