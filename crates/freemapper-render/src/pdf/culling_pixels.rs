//! Leaving out what cannot show changes no pixel (ADR 0031): each page is written with and without culling,
//! rasterised by poppler's `pdftoppm`, and the two images are compared byte for byte. The test needs `pdftoppm`
//! (or the program `PDFTOPPM` names), so it is ignored by default:
//!
//! ```text
//! cargo test -p freemapper-render --lib culling_keeps_every_pixel -- --ignored --nocapture
//! ```
//!
//! Compact paths keep the picture (ADR 0032): `compact_paths_keep_the_picture` writes each page with compact paths and
//! with every point as before, and rasterises both at 150 dpi, and at 600 dpi averaged in 4 × 4 blocks to 150 dpi. It
//! fails if more than one pixel in 10 000 differs by more than 32 levels in a channel either way, or a pixel of the
//! averaged images by more than 64. poppler snaps thin lines to the device pixels, and a line whose coordinates move
//! by 1/1000 point can land on the next pixel row, a quarter of a 4 × 4 block. The test also prints the write times of
//! both ways (meaningful with `--release`).
//!
//! It draws the forest sample on cropped pages: at 1:2000 on A4, at 1:5000 and 1:10000 on small free pages, at 1:7500
//! off centre with the information block, and at 1:2000 with a mask and an overprint grid that crosses the frame. More maps
//! come from `FREEMAPPER_CULLING_MAPS`, paths joined as in `PATH`; each is drawn at its own scale (both factors 1)
//! on a cropped A4 page, the way freemapper-print's `render` does. Maps of other people stay out of the repository
//! this way. `FREEMAPPER_CULLING_DPI` sets the resolutions (comma-separated, default 150).
//! `FREEMAPPER_CULLING_OUT` names a folder that keeps the PDFs and images; without it they go to a temporary
//! folder that is removed.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use freemapper_omap::{MapCoord, MapDocument};

use super::{PdfMeta, Writing, write_pdf_with};
use crate::build::{Orientation, PageSpec, Paper, build_page};
use crate::draw::{FillRule, Shape};
use crate::page::{DirectColour, MapClip, Overprint, RenderedPage};
use crate::symbols::{Cap, Join};

/// One page to compare.
struct Case {
    name: String,
    page: RenderedPage,
}

fn forest_sample() -> MapDocument {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/maps/forest-sample.omap"
    );
    freemapper_omap::read::read_map_file(Path::new(path)).unwrap()
}

/// `page` with purple circles every 15 mm over the whole page and beyond it, half of them under the map, and a mask
/// with a round window: overprint and mask runs that cross the frame.
fn with_overprint_and_mask(mut page: RenderedPage) -> RenderedPage {
    let circle = |c: (f64, f64), r: f64| -> Vec<(f64, f64)> {
        (0..48)
            .map(|i| {
                let a = f64::from(i) * std::f64::consts::TAU / 48.0;
                (c.0 + r * a.cos(), c.1 + r * a.sin())
            })
            .collect()
    };
    let (w, h) = page.page_mm;
    let (mut lower, mut upper) = (Vec::new(), Vec::new());
    for i in -3..=((w / 15.0) as i32 + 3) {
        for j in -3..=((h / 15.0) as i32 + 3) {
            let c = (f64::from(i) * 15.0, f64::from(j) * 15.0);
            let ring = Shape::Stroke {
                path: circle(c, 6.0),
                closed: true,
                width: 0.35,
                cap: Cap::Flat,
                join: Join::Miter,
            };
            if (i + j) % 2 == 0 {
                lower.push(ring);
            } else {
                upper.push(ring);
            }
        }
    }
    page.overprint = Overprint {
        lower,
        upper,
        below_priority: page
            .primitives
            .get(page.primitives.len() / 2)
            .map(|p| p.priority),
        colour: DirectColour {
            cmyk: [0.0, 1.0, 0.0, 0.0],
        },
        cut: false,
    };
    let frame = page.clip;
    let middle = (
        (frame.min.0 + frame.max.0) / 2.0,
        (frame.min.1 + frame.max.1) / 2.0,
    );
    page.map_clips = vec![MapClip::new(
        frame,
        [vec![
            (frame.min.0 - 20.0, frame.min.1 - 20.0),
            (frame.max.0 + 20.0, frame.min.1 - 20.0),
            (frame.max.0 + 20.0, middle.1),
            (frame.min.0 - 20.0, middle.1),
        ]],
        [circle((middle.0, frame.min.1 + 10.0), 25.0)],
    )];
    // A filled overprint shape across the frame edge, too.
    page.overprint.upper.push(Shape::Fill {
        rings: vec![circle((frame.max.0, middle.1), 8.0)],
        rule: FillRule::NonZero,
    });
    page
}

/// The page of `map` as `spec` sets it; it must be cropped: the map reaches beyond the frame.
fn cropped(map: &MapDocument, spec: &PageSpec, name: &str) -> RenderedPage {
    let (page, report) = build_page(map, spec).unwrap();
    assert!(
        report.overflow_mm.is_some(),
        "{name} is not cropped: {:?} on {:?}",
        crate::bounds::painted_bounds(&page.primitives),
        page.clip
    );
    page
}

fn cases() -> Vec<Case> {
    let forest = forest_sample();
    // The forest sample is drawn at 1:10000 and paints about 58 x 54 mm at that scale.
    let a4_at_2000 = PageSpec {
        orientation: Orientation::Portrait,
        ..PageSpec::new(2.5, 5.0, Paper::A4)
    };
    let mut cases = vec![
        ("forest-2000-a4", a4_at_2000.clone()),
        (
            "forest-5000-free",
            PageSpec {
                page_mm: Some((80.0, 100.0)),
                ..PageSpec::new(2.0, 2.0, Paper::A4)
            },
        ),
        (
            "forest-10000-small",
            PageSpec {
                page_mm: Some((30.0, 40.0)),
                frame_margin_mm: Some(4.0),
                ..PageSpec::new(1.0, 1.0, Paper::A4)
            },
        ),
        (
            "forest-7500-off-centre",
            PageSpec {
                orientation: Orientation::Landscape,
                centre: Some(MapCoord::new(-40_000, 25_000)),
                show_sheet_objects: true,
                ..PageSpec::new(2.0 / 1.5, 10.0 / 7.5, Paper::A4)
            },
        ),
    ]
    .into_iter()
    .map(|(name, spec)| Case {
        name: name.into(),
        page: cropped(&forest, &spec, name),
    })
    .collect::<Vec<_>>();
    cases.push(Case {
        name: "forest-2000-mask-overprint".into(),
        page: with_overprint_and_mask(cropped(&forest, &a4_at_2000, "mask")),
    });
    if let Some(paths) = std::env::var_os("FREEMAPPER_CULLING_MAPS") {
        for path in std::env::split_paths(&paths) {
            let map = freemapper_omap::read::read_map_file(&path)
                .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            let name = format!("{}-a4", path.file_stem().unwrap().to_string_lossy());
            let page = cropped(&map, &PageSpec::new(1.0, 1.0, Paper::A4), &name);
            cases.push(Case { name, page });
        }
    }
    cases
}

/// `pdf` rasterised at `dpi` by `pdftoppm` into `<stem>.ppm` next to it.
fn rasterise(pdf: &Path, dpi: u32) -> PathBuf {
    let program = std::env::var_os("PDFTOPPM").unwrap_or_else(|| "pdftoppm".into());
    let stem = pdf.with_extension("").with_file_name(format!(
        "{}-{dpi}",
        pdf.file_stem().unwrap().to_string_lossy()
    ));
    let status = Command::new(&program)
        .args(["-r", &dpi.to_string(), "-singlefile"])
        .arg(pdf)
        .arg(&stem)
        .status()
        .unwrap_or_else(|e| panic!("{}: {e}; set PDFTOPPM", program.to_string_lossy()));
    assert!(status.success(), "pdftoppm failed on {}", pdf.display());
    stem.with_extension("ppm")
}

/// The header of a binary PPM (`P6`): width, height and where the pixels start.
fn ppm_header(data: &[u8]) -> (usize, usize, usize) {
    let mut fields = Vec::new();
    let mut i = 0;
    while fields.len() < 4 {
        while data[i].is_ascii_whitespace() {
            i += 1;
        }
        let start = i;
        while !data[i].is_ascii_whitespace() {
            i += 1;
        }
        fields.push(std::str::from_utf8(&data[start..i]).unwrap().to_string());
    }
    assert_eq!(fields[0], "P6");
    (
        fields[1].parse().unwrap(),
        fields[2].parse().unwrap(),
        i + 1,
    )
}

/// A box of pixels: the first and the last column and row.
type PixelBox = ((usize, usize), (usize, usize));

/// How many pixels differ, and the box they lie in.
fn differing_pixels(a: &[u8], b: &[u8]) -> (usize, Option<PixelBox>) {
    let (w, h, start) = ppm_header(a);
    assert_eq!((w, h, start), ppm_header(b), "image sizes differ");
    let (mut count, mut bbox) = (0, None::<PixelBox>);
    for (n, (p, q)) in a[start..].chunks(3).zip(b[start..].chunks(3)).enumerate() {
        if p != q {
            count += 1;
            let (x, y) = (n % w, n / w);
            bbox = Some(match bbox {
                None => ((x, y), (x, y)),
                Some((min, max)) => ((min.0.min(x), min.1.min(y)), (max.0.max(x), max.1.max(y))),
            });
        }
    }
    (count, bbox)
}

#[test]
#[ignore = "needs pdftoppm; run with --ignored"]
fn culling_keeps_every_pixel() {
    let dpis: Vec<u32> = std::env::var("FREEMAPPER_CULLING_DPI")
        .unwrap_or_else(|_| "150".into())
        .split(',')
        .map(|d| d.trim().parse().unwrap())
        .collect();
    let (dir, keep) = match std::env::var_os("FREEMAPPER_CULLING_OUT") {
        Some(dir) => (PathBuf::from(dir), true),
        None => (
            std::env::temp_dir().join(format!("freemapper-culling-{}", std::process::id())),
            false,
        ),
    };
    std::fs::create_dir_all(&dir).unwrap();
    let mut failures = Vec::new();
    for case in cases() {
        let meta = PdfMeta::new(&case.name);
        let uncut = Writing {
            cull: false,
            ..Writing::FILE
        };
        let (all, all_info) = write_pdf_with(&case.page, &meta, uncut).unwrap();
        let (culled, culled_info) = write_pdf_with(&case.page, &meta, Writing::FILE).unwrap();
        assert_eq!(
            all_info, culled_info,
            "{}: what the page reports",
            case.name
        );
        let old = dir.join(format!("{}-all.pdf", case.name));
        let new = dir.join(format!("{}-culled.pdf", case.name));
        std::fs::write(&old, &all).unwrap();
        std::fs::write(&new, &culled).unwrap();
        eprintln!(
            "{}: {} bytes without culling, {} with ({:.1} %)",
            case.name,
            all.len(),
            culled.len(),
            100.0 * culled.len() as f64 / all.len() as f64
        );
        for &dpi in &dpis {
            let (a, b) = (rasterise(&old, dpi), rasterise(&new, dpi));
            let (a, b) = (std::fs::read(a).unwrap(), std::fs::read(b).unwrap());
            let (count, bbox) = differing_pixels(&a, &b);
            eprintln!("  {dpi} dpi: {count} pixels differ");
            if count > 0 {
                failures.push(format!(
                    "{} at {dpi} dpi: {count} pixels in {bbox:?}",
                    case.name
                ));
            }
        }
    }
    if !keep {
        let _ = std::fs::remove_dir_all(&dir);
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

/// The pixels of a binary PPM with each `factor` × `factor` block averaged into one: width, height and RGB bytes.
fn box_filtered(data: &[u8], factor: usize) -> (usize, usize, Vec<u8>) {
    let (w, h, start) = ppm_header(data);
    let pixels = &data[start..];
    let (bw, bh) = (w / factor, h / factor);
    let mut out = Vec::with_capacity(bw * bh * 3);
    for by in 0..bh {
        for bx in 0..bw {
            for c in 0..3 {
                let mut sum = 0usize;
                for y in by * factor..(by + 1) * factor {
                    for x in bx * factor..(bx + 1) * factor {
                        sum += usize::from(pixels[(y * w + x) * 3 + c]);
                    }
                }
                let n = factor * factor;
                out.push(((sum + n / 2) / n) as u8);
            }
        }
    }
    (bw, bh, out)
}

/// How two images of the same size differ: pixels that differ at all, the largest difference of a channel, and the
/// pixels with a channel more than 16 and more than 32 levels apart.
fn channel_differences(a: &[u8], b: &[u8]) -> (usize, u8, usize, usize) {
    assert_eq!(a.len(), b.len(), "image sizes differ");
    let (mut any, mut max, mut over16, mut over32) = (0, 0u8, 0, 0);
    for (p, q) in a.chunks(3).zip(b.chunks(3)) {
        let d = p
            .iter()
            .zip(q)
            .map(|(x, y)| x.abs_diff(*y))
            .max()
            .unwrap_or(0);
        any += usize::from(d > 0);
        over16 += usize::from(d > 16);
        over32 += usize::from(d > 32);
        max = max.max(d);
    }
    (any, max, over16, over32)
}

#[test]
#[ignore = "needs pdftoppm; run with --ignored"]
fn compact_paths_keep_the_picture() {
    let (dir, keep) = match std::env::var_os("FREEMAPPER_CULLING_OUT") {
        Some(dir) => (PathBuf::from(dir), true),
        None => (
            std::env::temp_dir().join(format!("freemapper-compact-{}", std::process::id())),
            false,
        ),
    };
    std::fs::create_dir_all(&dir).unwrap();
    let polylines = Writing {
        compact: false,
        ..Writing::FILE
    };
    let mut failures = Vec::new();
    for case in cases() {
        let meta = PdfMeta::new(&case.name);
        let started = Instant::now();
        let (old, old_info) = write_pdf_with(&case.page, &meta, polylines).unwrap();
        let old_time = started.elapsed();
        let started = Instant::now();
        let (new, new_info) = write_pdf_with(&case.page, &meta, Writing::FILE).unwrap();
        let new_time = started.elapsed();
        assert_eq!(old_info, new_info, "{}: what the page reports", case.name);
        let old_pdf = dir.join(format!("{}-polylines.pdf", case.name));
        let new_pdf = dir.join(format!("{}-compact.pdf", case.name));
        std::fs::write(&old_pdf, &old).unwrap();
        std::fs::write(&new_pdf, &new).unwrap();
        eprintln!(
            "{}: {} bytes as before in {old_time:?}, {} compact in {new_time:?} ({:.1} %)",
            case.name,
            old.len(),
            new.len(),
            100.0 * new.len() as f64 / old.len() as f64
        );
        let read = |pdf: &Path, dpi: u32| {
            let ppm = rasterise(pdf, dpi);
            let data = std::fs::read(&ppm).unwrap();
            std::fs::remove_file(ppm).unwrap();
            data
        };
        let (a, b) = (read(&old_pdf, 150), read(&new_pdf, 150));
        let start = ppm_header(&a).2;
        let (any, max, over16, over32) = channel_differences(&a[start..], &b[start..]);
        eprintln!(
            "  150 dpi: {any} pixels differ, by up to {max}; {over16} by more than 16, {over32} by more than 32"
        );
        if over32 * 10_000 > (a.len() - start) / 3 {
            failures.push(format!(
                "{} at 150 dpi: {over32} pixels differ by more than 32 levels",
                case.name
            ));
        }
        let (a, b) = (read(&old_pdf, 600), read(&new_pdf, 600));
        let ((wa, ha, a), (wb, hb, b)) = (box_filtered(&a, 4), box_filtered(&b, 4));
        assert_eq!((wa, ha), (wb, hb));
        let (any, max, over16, over32) = channel_differences(&a, &b);
        eprintln!(
            "  600 dpi averaged to 150: {any} pixels differ, by up to {max}; {over16} by more than 16, {over32} by more \
             than 32"
        );
        if max > 64 || over32 * 10_000 > a.len() / 3 {
            failures.push(format!(
                "{} averaged from 600 dpi: {over32} pixels differ by more than 32 levels, by up to {max}",
                case.name
            ));
        }
    }
    if !keep {
        let _ = std::fs::remove_dir_all(&dir);
    }
    assert!(failures.is_empty(), "{failures:#?}");
}
