//! The SVG backend against hand-built pages: golden output, determinism, the view, and parity with the PDF.
//!
//! `UPDATE_GOLDEN=1 cargo test -p freemapper-render --test svg` rewrites the golden files in `tests/golden/`.

use std::sync::Arc;

use freemapper_render::page::{
    Cap, DirectColour, FillRule, Join, MapClip, Overprint, PaperBox, Primitive, Pt, RasterImage,
    RenderColour, RenderedPage, Shape, SheetItem,
};
use freemapper_render::pdf::{PdfMeta, write_pdf};
use freemapper_render::svg::{SvgError, SvgView, write_svg};

fn colour(priority: i32, name: &str, cmyk: [f64; 4]) -> RenderColour {
    RenderColour {
        priority,
        name: name.into(),
        cmyk,
        rgb: [0.0; 3],
        opacity: 1.0,
    }
}

fn square(min: Pt, max: Pt) -> Vec<Pt> {
    vec![min, (max.0, min.1), max, (min.0, max.1)]
}

fn stroke(path: Vec<Pt>, width: f64) -> Shape {
    Shape::Stroke {
        path,
        closed: false,
        width,
        cap: Cap::Round,
        join: Join::Round,
    }
}

fn at(object: u32, priority: i32, shape: Shape) -> Primitive {
    Primitive {
        object,
        symbol: 100 + object,
        ..Primitive::new(priority, shape)
    }
}

/// A page of 60 × 40 mm with a frame of 5 mm: an area with a hole, a line, a hatched area (two lines sharing
/// one clip), a text and a framed text turned a quarter, one mask with a window, both overprint layers, and a
/// sheet title, rectangle and image.
fn tiny_page() -> RenderedPage {
    let hatch: Arc<[Vec<Pt>]> = vec![square((30.0, 20.0), (40.0, 30.0))].into();
    let hatched = |y: f64| Shape::Clipped {
        rings: Arc::clone(&hatch),
        inner: Box::new(Shape::Stroke {
            path: vec![(28.0, y), (42.0, y)],
            closed: false,
            width: 0.5,
            cap: Cap::Flat,
            join: Join::Miter,
        }),
    };
    let frame = PaperBox {
        min: (5.0, 5.0),
        max: (55.0, 35.0),
    };
    RenderedPage {
        primitives: vec![
            at(
                0,
                5,
                Shape::Fill {
                    rings: vec![
                        square((8.0, 8.0), (28.0, 18.0)),
                        square((12.0, 10.0), (16.0, 14.0)),
                    ],
                    rule: FillRule::EvenOdd,
                },
            ),
            at(
                1,
                3,
                stroke(vec![(8.0, 22.0), (20.0, 26.0), (26.0, 22.0)], 0.35),
            ),
            at(2, 3, hatched(23.0)),
            at(2, 3, hatched(27.0)),
            at(
                3,
                0,
                Shape::Text {
                    origin: (8.0, 33.0),
                    size: 3.0,
                    bold: false,
                    italic: false,
                    text: "Ab".into(),
                    rotation: 0.0,
                    stroke: None,
                },
            ),
            at(
                4,
                0,
                Shape::Text {
                    origin: (50.0, 30.0),
                    size: 2.0,
                    bold: true,
                    italic: false,
                    text: "1".into(),
                    rotation: std::f64::consts::FRAC_PI_2,
                    stroke: Some(0.1),
                },
            ),
        ],
        clip: frame,
        page_mm: (60.0, 40.0),
        centre_mm: (0.0, 0.0),
        colours: vec![
            colour(0, "Black", [0.0, 0.0, 0.0, 1.0]),
            colour(3, "Blue", [1.0, 0.0, 0.0, 0.0]),
            colour(5, "Green", [0.5, 0.0, 1.0, 0.0]),
        ],
        sheet: vec![
            SheetItem::Shape {
                colour: DirectColour::BLACK,
                shape: Shape::Text {
                    origin: (6.0, 4.0),
                    size: 2.5,
                    bold: false,
                    italic: true,
                    text: "T".into(),
                    rotation: 0.0,
                    stroke: None,
                },
            },
            SheetItem::Shape {
                colour: DirectColour {
                    cmyk: [0.0, 0.0, 0.0, 0.5],
                },
                shape: Shape::Fill {
                    rings: vec![square((45.0, 36.0), (55.0, 38.0))],
                    rule: FillRule::NonZero,
                },
            },
            SheetItem::Image {
                rect: PaperBox {
                    min: (1.0, 36.0),
                    max: (3.0, 37.0),
                },
                image: Arc::new(
                    RasterImage::new(2, 1, vec![255, 0, 0, 0, 0, 255], Some(vec![255, 128]))
                        .unwrap(),
                ),
            },
        ],
        overprint: Overprint {
            lower: vec![Shape::Stroke {
                path: square((20.0, 6.0), (30.0, 12.0)),
                closed: true,
                width: 0.35,
                cap: Cap::Flat,
                join: Join::Miter,
            }],
            upper: vec![Shape::Fill {
                rings: vec![vec![(46.0, 20.0), (52.0, 20.0), (49.0, 25.0)]],
                rule: FillRule::NonZero,
            }],
            below_priority: Some(3),
            colour: DirectColour {
                cmyk: [0.0, 1.0, 0.0, 0.0],
            },
            cut: false,
        },
        map_clips: vec![MapClip::new(
            frame,
            [square((10.0, 7.0), (20.0, 12.0))],
            [square((13.0, 8.0), (15.0, 9.0))],
        )],
    }
}

/// Compares `svg` with `tests/golden/<name>`, or writes it there under `UPDATE_GOLDEN`.
fn golden(name: &str, svg: &str) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(name);
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::write(&path, svg).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
        .replace("\r\n", "\n");
    assert!(
        svg == expected,
        "{name} differs; rerun with UPDATE_GOLDEN=1 and review the diff\n{svg}"
    );
}

/// The `<path` elements outside clip paths: what paints.
fn drawn_paths(svg: &str) -> usize {
    let mut count = 0;
    let mut rest = svg;
    while let Some(start) = rest.find("<clipPath") {
        count += rest[..start].matches("<path").count();
        let end = rest[start..].find("</clipPath>").unwrap() + start;
        rest = &rest[end..];
    }
    count + rest.matches("<path").count()
}

#[test]
fn tiny_page_matches_the_golden_svg() {
    let svg = write_svg(&tiny_page(), &SvgView::default()).unwrap();
    golden("tiny-page.svg", &svg);
}

#[test]
fn a_view_matches_the_golden_svg() {
    let view = SvgView {
        rect: Some(PaperBox {
            min: (7.0, 5.0),
            max: (31.0, 19.0),
        }),
        zoom: 10.0,
    };
    let svg = write_svg(&tiny_page(), &view).unwrap();
    golden("tiny-page-view.svg", &svg);
}

#[test]
fn the_same_page_gives_the_same_text() {
    // Fresh pages hold their clips at other addresses; the text must not depend on them.
    let first = write_svg(&tiny_page(), &SvgView::default()).unwrap();
    let page = tiny_page();
    let second = write_svg(&page, &SvgView::default()).unwrap();
    let third = write_svg(&page, &SvgView::default()).unwrap();
    assert_eq!(first, second);
    assert_eq!(second, third);
}

#[test]
fn shapes_outside_the_view_are_left_out() {
    let page = tiny_page();
    let whole = write_svg(&page, &SvgView::default()).unwrap();
    // The left half of the area with the hole, nothing else of the map.
    let view = SvgView {
        rect: Some(PaperBox {
            min: (6.0, 6.0),
            max: (11.0, 11.0),
        }),
        zoom: 4.0,
    };
    let part = write_svg(&page, &view).unwrap();
    assert!(part.contains("viewBox=\"6 6 5 5\">"), "{part}");
    assert!(part.contains("width=\"20\" height=\"20\""), "{part}");
    // The area is green; the line, the hatch, the texts, the overprint and the sheet are gone.
    assert_eq!(drawn_paths(&part), 1, "{part}");
    assert!(part.contains("fill=\"#80ff00\""));
    for gone in ["#00ffff", "#ff00ff", "<image", "url(#c0)", "#808080"] {
        assert!(whole.contains(gone), "{gone}");
        assert!(!part.contains(gone), "{gone}");
    }
    // A view beside the frame shows the paper and the sheet only.
    let beside = SvgView {
        rect: Some(PaperBox {
            min: (0.0, 35.5),
            max: (60.0, 40.0),
        }),
        zoom: 1.0,
    };
    let sheet = write_svg(&page, &beside).unwrap();
    assert!(!sheet.contains("url(#frame)"));
    assert_eq!(drawn_paths(&sheet), 1, "{sheet}");
    assert!(sheet.contains("<image"));
}

#[test]
fn every_primitive_on_the_page_becomes_one_path_and_the_pdf_draws_the_same_page() {
    let mut page = tiny_page();
    page.map_clips.clear();
    let mut far = at(9, 3, stroke(vec![(500.0, 500.0), (600.0, 500.0)], 1.0));
    page.primitives.push(far.clone());
    // On the page, but outside the frame: clipped away, so left out too.
    far.shape = stroke(vec![(1.0, 1.0), (3.0, 1.0)], 0.2);
    page.primitives.push(far);
    write_pdf(&page, &PdfMeta::new("parity")).unwrap();
    let svg = write_svg(&page, &SvgView::default()).unwrap();
    let primitives = page.primitives.len() - 2;
    let overprint = page.overprint.lower.len() + page.overprint.upper.len();
    let sheet_shapes = 2;
    assert_eq!(
        drawn_paths(&svg),
        primitives + overprint + sheet_shapes,
        "{svg}"
    );
    assert_eq!(svg.matches("<image").count(), 1);
}

#[test]
fn texts_are_outlines_and_need_no_font() {
    let svg = write_svg(&tiny_page(), &SvgView::default()).unwrap();
    assert!(!svg.contains("<text"));
    assert!(!svg.contains("font"));
}

#[test]
fn empty_and_huge_pages_are_written() {
    let mut page = tiny_page();
    page.primitives.clear();
    page.sheet.clear();
    page.overprint = Overprint::default();
    page.map_clips.clear();
    let empty = write_svg(&page, &SvgView::default()).unwrap();
    assert_eq!(
        empty,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"240\" height=\"160\" viewBox=\"0 0 60 40\">\n\
         <rect x=\"0\" y=\"0\" width=\"60\" height=\"40\" fill=\"#ffffff\"/>\n</svg>\n"
    );
    page.page_mm = (0.0, 0.0);
    assert!(write_svg(&page, &SvgView::default()).is_ok());
    page.page_mm = (1e9, 1e9);
    page.clip = PaperBox {
        min: (0.0, 0.0),
        max: (1e9, 1e9),
    };
    page.primitives = tiny_page().primitives;
    assert!(write_svg(&page, &SvgView::default()).is_ok());
}

#[test]
fn bad_views_and_numbers_are_refused() {
    let page = tiny_page();
    for view in [
        SvgView {
            rect: None,
            zoom: 0.0,
        },
        SvgView {
            rect: None,
            zoom: f64::NAN,
        },
        SvgView {
            rect: Some(PaperBox {
                min: (10.0, 0.0),
                max: (5.0, 10.0),
            }),
            zoom: 1.0,
        },
        SvgView {
            rect: Some(PaperBox {
                min: (0.0, 0.0),
                max: (f64::INFINITY, 10.0),
            }),
            zoom: 1.0,
        },
    ] {
        assert!(matches!(
            write_svg(&page, &view),
            Err(SvgError::InvalidView)
        ));
    }
    let mut broken = tiny_page();
    broken.primitives.push(at(
        8,
        0,
        Shape::Fill {
            rings: vec![vec![(10.0, 10.0), (f64::NAN, 12.0), (12.0, 12.0)]],
            rule: FillRule::EvenOdd,
        },
    ));
    assert!(matches!(
        write_svg(&broken, &SvgView::default()),
        Err(SvgError::NotFinite)
    ));
    let mut no_size = tiny_page();
    no_size.page_mm = (f64::NAN, 40.0);
    assert!(matches!(
        write_svg(&no_size, &SvgView::default()),
        Err(SvgError::NotFinite)
    ));
}
