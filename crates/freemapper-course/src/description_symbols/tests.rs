use super::db::{MAX_DEPTH, MAX_POINTS_PER_SYMBOL, Pt, SYMBOLS_XML, db, parse, seg_dist};
use super::iof_2024::{BEND_2024, DIRECTIONS, Mapped, iof_2024};
use super::ppen::{ColumnNote, Inverse, Refusal, ppen_columns, ppen_symbol};
use super::{FLATNESS, Standard, TextLang, exists, ids, symbol, symbol_kind, symbol_text};

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

// ---- the mapping between Purple Pen ids and IOF 2024 ----

/// Every Purple Pen description symbol id of the database (not the instruction texts such as `basic_combo`).
fn purple_pen_ids() -> Vec<&'static str> {
    let mut ids: Vec<&str> = db()
        .expect("embedded symbol file parses")
        .entries
        .iter()
        .map(|e| e.id.as_str())
        .filter(|id| id.starts_with(|c: char| c.is_ascii_digit()))
        .collect();
    ids.sort_unstable();
    ids.dedup();
    ids
}

fn some(s: &str) -> Option<String> {
    Some(s.to_owned())
}

fn columns(c: [Option<&str>; 6]) -> [Option<String>; 6] {
    c.map(|x| x.map(str::to_owned))
}

/// The symbol kinds Purple Pen's description popup offers per column (`purple_pen_strict.rs`
/// `COLUMN_KINDS`). Column C also takes the instruction rows (V to Z), which a finish, a crossing point or a
/// map issue point keeps in its one box.
fn accepts(index: usize, kind: char) -> bool {
    match index {
        0 => kind == 'C' || ('V'..='Z').contains(&kind),
        1 => kind == 'D',
        2 => kind == 'E' || kind == 'D',
        3 => kind == 'F',
        4 => kind == 'G',
        _ => kind == 'H',
    }
}

const STANDARDS: [Standard; 3] = [Standard::Iof2004, Standard::Iof2018, Standard::Iof2024];

#[test]
fn forward_covers_every_purple_pen_id_but_12_3_and_13_6() {
    let ids = purple_pen_ids();
    assert!(ids.len() > 150, "{} ids", ids.len());
    let unmapped: Vec<&str> = ids
        .iter()
        .copied()
        .filter(|id| iof_2024(id).is_none())
        .collect();
    assert_eq!(unmapped, ["12.3", "13.6"]);
    let to = |id: &str| iof_2024(id).map(|m| m.id);
    for (from, want) in [
        ("1.10", "1.10"),
        ("2.10", "2.10"),
        ("5.20", "5.20"),
        ("5.23", "5.21"),
        ("5.26", "5.24"),
        ("9.3deep", "9.3"),
        ("10.2single", "10.2"),
        ("11.7", BEND_2024),
        ("11.1N", "12.1N"),
        ("11.6SW", "12.6SW"),
        ("11.8W", "12.7W"),
        ("11.9", "12.8"),
        ("11.10", "12.9"),
        ("11.11", "12.10"),
        ("11.13", "12.11"),
        ("11.14NE", "12.12NE"),
        ("11.12", "12.13"),
        ("11.15single", "12.14"),
        ("12.2", "13.2"),
        ("12.4", "13.3"),
        ("13.3", "15.3"),
        ("13.5control", "15.5"),
        ("15.6", "15.6"),
        ("14.3", "16.3"),
    ] {
        assert_eq!(to(from).as_deref(), Some(want), "{from}");
    }
    for nonsense in ["", "1", "1.17", "11.1", "1.1N", "x.1", "11.7NE", "9.1wide"] {
        assert_eq!(to(nonsense), None, "{nonsense}");
    }
    assert_eq!(
        iof_2024("10.1single"),
        Some(Mapped {
            id: "10.1".to_owned(),
            single: true
        })
    );
    assert_eq!(DIRECTIONS.len(), 8);
}

#[test]
fn every_purple_pen_id_round_trips() {
    // Ids that come back as another Purple Pen id: the two readings of "height or depth" (9.1 and 9.3 of 2024
    // draw one symbol; the writer says "high", which is `chosen`), and the variants the writer builds from the
    // columns (`single`, `control`).
    fn back(id: &str) -> (&str, bool) {
        match id {
            "9.1deep" => ("9.1high", true),
            "9.3deep" => ("9.3high", true),
            "9.1high" => ("9.1high", true),
            "9.3high" => ("9.3high", true),
            "10.1single" => ("10.1", false),
            "10.2single" => ("10.2", false),
            "11.15single" => ("11.15", false),
            "13.5control" => ("13.5", false),
            other => (other, false),
        }
    }
    for id in purple_pen_ids() {
        let Some(mapped) = iof_2024(id) else { continue };
        for standard in STANDARDS {
            if !exists(id, standard) {
                continue;
            }
            // The written id may be missing in the standard (a 2004 file has no 5.25): that is a refusal.
            let want = back(id);
            let got = ppen_symbol(&mapped.id, standard);
            if exists(want.0, standard) {
                assert_eq!(
                    got,
                    Ok(Inverse {
                        id: want.0.to_owned(),
                        chosen: want.1
                    }),
                    "{id} in {standard:?}"
                );
            } else {
                assert_eq!(got, Err(Refusal::NotInStandard(standard)), "{id}");
            }
        }
    }
}

/// The 2024 ids the forward mapping produces, and the ids the rulings talk about.
fn reachable_ids() -> Vec<String> {
    let mut ids: Vec<String> = purple_pen_ids()
        .into_iter()
        .filter_map(iof_2024)
        .map(|m| m.id)
        .collect();
    for extra in [
        "0.1NE", "0.1NW", "0.1SE", "0.1SW", "0.2N", "0.2E", "0.2S", "0.2W", "9.1", "9.3", "2.10",
        "15.6", "5.21", "5.22", "5.23", "5.24", "5.25", "5.26", "11.1", "12.14", "10.1", "10.2",
        "99.9",
    ] {
        ids.push(extra.to_owned());
    }
    ids.sort();
    ids.dedup();
    ids
}

#[test]
fn every_written_id_exists_in_its_standard_and_box() {
    let ids = reachable_ids();
    for standard in STANDARDS {
        for id in &ids {
            for index in 0..6 {
                let mut given: [Option<String>; 6] = Default::default();
                given[index] = Some(id.clone());
                // Second case: next to a bend, a combination and a feature in D and E.
                let mut crowded = given.clone();
                for (i, x) in [(1, "5.2"), (2, "5.2"), (3, "10.1")] {
                    crowded[i].get_or_insert_with(|| x.to_owned());
                }
                for given in [given, crowded] {
                    let out = ppen_columns(&given, standard);
                    for (i, written) in out.columns.iter().enumerate() {
                        let Some(w) = written else { continue };
                        assert!(exists(w, standard), "{w} in {standard:?} ({given:?})");
                        let kind = symbol_kind(w, standard).unwrap();
                        assert!(
                            accepts(i, kind),
                            "{w} (kind {kind}) in box {i}, {standard:?}, {given:?}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn cardinal_and_diagonal_directions_are_normalised() {
    let ok = |id: &str| {
        Ok(Inverse {
            id: id.to_owned(),
            chosen: false,
        })
    };
    assert_eq!(ppen_symbol("0.1NE", Standard::Iof2018), ok("0.2NE"));
    assert_eq!(ppen_symbol("0.1SW", Standard::Iof2004), ok("0.2SW"));
    assert_eq!(ppen_symbol("0.2W", Standard::Iof2018), ok("0.1W"));
    assert_eq!(ppen_symbol("0.2N", Standard::Iof2024), ok("0.1N"));
    assert_eq!(ppen_symbol("0.1N", Standard::Iof2018), ok("0.1N"));
    assert_eq!(ppen_symbol("0.2SE", Standard::Iof2018), ok("0.2SE"));
}

#[test]
fn plain_height_is_written_as_high() {
    let high = |id: &str| {
        Ok(Inverse {
            id: id.to_owned(),
            chosen: true,
        })
    };
    assert_eq!(ppen_symbol("9.1", Standard::Iof2018), high("9.1high"));
    assert_eq!(ppen_symbol("9.3", Standard::Iof2004), high("9.3high"));
    let out = ppen_columns(
        &columns([None, None, None, Some("9.1"), None, None]),
        Standard::Iof2018,
    );
    assert_eq!(out.columns[3], some("9.1high"));
    assert_eq!(
        out.notes,
        vec![ColumnNote::VariantChosen {
            column: 'F',
            id: "9.1".to_owned(),
            written: "9.1high".to_owned()
        }]
    );
}

#[test]
fn symbols_of_2018_are_refused_in_2004() {
    for id in ["2.10", "15.6", "5.23", "5.24"] {
        assert_eq!(
            ppen_symbol(id, Standard::Iof2004),
            Err(Refusal::NotInStandard(Standard::Iof2004)),
            "{id}"
        );
        assert!(ppen_symbol(id, Standard::Iof2018).is_ok(), "{id}");
    }
    // 2024 5.21 and 5.22 are Purple Pen's 5.23 and 5.24, which both standards have.
    assert!(ppen_symbol("5.21", Standard::Iof2004).is_ok());
    // The bend cannot stand in box F of a 2004 file when G is taken: 11.7 is a G symbol there.
    let out = ppen_columns(
        &columns([None, None, None, Some("11.1"), Some("12.1N"), None]),
        Standard::Iof2004,
    );
    assert_eq!((&out.columns[3], &out.columns[4]), (&None, &some("11.1N")));
    assert_eq!(
        out.notes,
        vec![ColumnNote::Refused {
            column: 'F',
            id: "11.1".to_owned(),
            reason: Refusal::WrongColumn
        }]
    );
}

#[test]
fn ids_without_a_purple_pen_symbol_are_refused() {
    for id in ["5.25", "5.26", "99.9", "14.1", ""] {
        assert_eq!(
            ppen_symbol(id, Standard::Iof2018),
            Err(Refusal::NoPurplePenId),
            "{id:?}"
        );
    }
    let out = ppen_columns(
        &columns([None, Some("5.2"), Some("99.9"), None, None, Some("13.9")]),
        Standard::Iof2018,
    );
    assert_eq!(out.columns[1], some("5.2"));
    assert_eq!((&out.columns[2], &out.columns[5]), (&None, &None));
    assert_eq!(
        out.notes,
        vec![
            ColumnNote::Refused {
                column: 'E',
                id: "99.9".to_owned(),
                reason: Refusal::NoPurplePenId
            },
            ColumnNote::Refused {
                column: 'H',
                id: "13.9".to_owned(),
                reason: Refusal::NoPurplePenId
            },
        ]
    );
}

#[test]
fn a_symbol_in_a_box_purple_pen_does_not_offer_is_refused() {
    // 8.1 is an E symbol: not in D, fine in E.
    let d = ppen_columns(
        &columns([None, Some("8.1"), None, None, None, None]),
        Standard::Iof2018,
    );
    assert_eq!(d.columns[1], None);
    assert!(matches!(
        d.notes[..],
        [ColumnNote::Refused {
            column: 'D',
            reason: Refusal::WrongColumn,
            ..
        }]
    ));
    let e = ppen_columns(
        &columns([None, None, Some("8.1"), None, None, None]),
        Standard::Iof2018,
    );
    assert_eq!(e.columns[2], some("8.1"));
    // A D symbol is fine in E.
    let e = ppen_columns(
        &columns([None, None, Some("5.2"), None, None, None]),
        Standard::Iof2018,
    );
    assert_eq!(e.columns[2], some("5.2"));
}

#[test]
fn single_variant_goes_to_the_combination_column() {
    for (combination, written) in [("10.1", "10.1single"), ("10.2", "10.2single")] {
        let out = ppen_columns(
            &columns([
                None,
                Some("5.2"),
                Some("5.2"),
                Some(combination),
                None,
                None,
            ]),
            Standard::Iof2018,
        );
        assert_eq!(out.columns[1], some("5.2"));
        assert_eq!(out.columns[2], None);
        assert_eq!(out.columns[3], some(written));
        assert!(out.notes.is_empty());
        // Back: the reader copies D into E when the id carries `single`.
        let mapped = iof_2024(written).unwrap();
        assert!(mapped.single);
        assert_eq!(mapped.id, combination);
    }
    // Different features stay in both columns, with the plain combination.
    let out = ppen_columns(
        &columns([None, Some("5.2"), Some("3.4"), Some("10.1"), None, None]),
        Standard::Iof2018,
    );
    assert_eq!(
        (&out.columns[1], &out.columns[2], &out.columns[3]),
        (&some("5.2"), &some("3.4"), &some("10.1"))
    );
    // Equal features without a combination stay in both columns.
    let out = ppen_columns(
        &columns([None, Some("5.2"), Some("5.2"), None, None, None]),
        Standard::Iof2018,
    );
    assert_eq!(
        (&out.columns[1], &out.columns[2]),
        (&some("5.2"), &some("5.2"))
    );
    // "Between" (2024 12.14, Purple Pen 11.15) keeps both columns: 11.15single in column G is a symbol Purple
    // Pen throws on, and 11.15 with the feature twice reads the same.
    let out = ppen_columns(
        &columns([None, Some("4.5"), Some("4.5"), None, Some("12.14"), None]),
        Standard::Iof2018,
    );
    assert_eq!(
        (&out.columns[1], &out.columns[2], &out.columns[4]),
        (&some("4.5"), &some("4.5"), &some("11.15"))
    );
}

#[test]
fn bend_moves_to_g_in_2004() {
    let col = |f: Option<&str>, g: Option<&str>, standard| {
        ppen_columns(&columns([None, None, None, f, g, None]), standard).columns
    };
    let bend = col(Some("11.1"), None, Standard::Iof2004);
    assert_eq!((&bend[3], &bend[4]), (&None, &some("11.7")));
    // From 2018 on the bend is in box F.
    for standard in [Standard::Iof2018, Standard::Iof2024] {
        let bend = col(Some("11.1"), None, standard);
        assert_eq!((&bend[3], &bend[4]), (&some("11.7"), &None));
    }
    // A bend in G is a 2004 symbol.
    let g = col(None, Some("11.1"), Standard::Iof2004);
    assert_eq!(g[4], some("11.7"));
    let g = col(None, Some("11.1"), Standard::Iof2018);
    assert_eq!(g[4], None);
}

#[test]
fn explicit_rows() {
    let one = |index: usize, id: &str, standard| {
        let mut given: [Option<String>; 6] = Default::default();
        given[index] = some(id);
        ppen_columns(&given, standard).columns[index].clone()
    };
    let s = Standard::Iof2004;
    assert_eq!(one(1, "5.21", s), some("5.23"));
    assert_eq!(one(4, "12.1NE", s), some("11.1NE"));
    assert_eq!(one(5, "13.1", s), some("12.1"));
    assert_eq!(one(0, "15.3", s), some("13.3"));
    assert_eq!(one(0, "16.3", s), some("14.3"));
}

#[test]
fn plain_id_wins_over_variants() {
    let s = Standard::Iof2018;
    let id = |x: &str| ppen_symbol(x, s).unwrap().id;
    assert_eq!(id("10.1"), "10.1");
    assert_eq!(id("15.5"), "13.5");
    assert_eq!(id("12.14"), "11.15");
    assert_eq!(id("9.1"), "9.1high");
}

#[test]
fn ids_lists_every_symbol_of_a_standard_once() {
    for standard in [Standard::Iof2004, Standard::Iof2018, Standard::Iof2024] {
        let ids: Vec<&str> = ids(standard).collect();
        assert!(!ids.is_empty());
        let unique: std::collections::BTreeSet<&str> = ids.iter().copied().collect();
        assert_eq!(unique.len(), ids.len(), "an id comes twice in {standard:?}");
        for id in &ids {
            assert!(
                exists(id, standard),
                "{id} listed but not defined in {standard:?}"
            );
            assert!(symbol_kind(id, standard).is_some());
        }
    }
    assert!(ids(Standard::Iof2018).any(|id| id == "5.11"));
    assert!(ids(Standard::Iof2004).count() != ids(Standard::Iof2018).count());
}
