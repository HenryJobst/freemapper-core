//! The embedded fonts: Arimo, metric-compatible with Arial, stands in for every OOM font family
//! (freemapper-print ADR 0017). Texts are measured with Arimo's advance widths, and each PDF page embeds a subset of
//! the faces it uses as Type0 / CIDFontType2 fonts with Identity-H encoding.

use std::collections::BTreeSet;
use std::sync::OnceLock;

use crate::pdf::PdfError;
use pdf_writer::types::{CidFontType, FontFlags, SystemInfo, UnicodeCmap};
use pdf_writer::{Filter, Finish, Name, Pdf, Rect, Ref, Str};
use subsetter::GlyphRemapper;
use ttf_parser::GlyphId;

/// One of the four Arimo styles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FontFace {
    Regular,
    Bold,
    Italic,
    BoldItalic,
}

const FACES: [FontFace; 4] = [
    FontFace::Regular,
    FontFace::Bold,
    FontFace::Italic,
    FontFace::BoldItalic,
];

impl FontFace {
    fn index(self) -> usize {
        self as usize
    }

    fn data(self) -> &'static [u8] {
        match self {
            FontFace::Regular => include_bytes!("../assets/fonts/Arimo-Regular.ttf"),
            FontFace::Bold => include_bytes!("../assets/fonts/Arimo-Bold.ttf"),
            FontFace::Italic => include_bytes!("../assets/fonts/Arimo-Italic.ttf"),
            FontFace::BoldItalic => include_bytes!("../assets/fonts/Arimo-BoldItalic.ttf"),
        }
    }

    /// The PostScript name (name table ID 6).
    fn postscript_name(self) -> &'static str {
        match self {
            FontFace::Regular => "Arimo-Regular",
            FontFace::Bold => "Arimo-Bold",
            FontFace::Italic => "Arimo-Italic",
            FontFace::BoldItalic => "Arimo-BoldItalic",
        }
    }

    /// The parsed font; the embedded files are known to parse (test `embedded_faces_parse`).
    fn parsed(self) -> &'static ttf_parser::Face<'static> {
        static PARSED: OnceLock<Vec<ttf_parser::Face<'static>>> = OnceLock::new();
        &PARSED.get_or_init(|| {
            FACES
                .iter()
                .map(|f| ttf_parser::Face::parse(f.data(), 0).expect("embedded Arimo parses"))
                .collect()
        })[self.index()]
    }

    /// The glyph of `c`, `None` if Arimo has none.
    fn glyph(self, c: char) -> Option<GlyphId> {
        self.parsed().glyph_index(c)
    }

    /// Advance width of a glyph in font units.
    fn advance(self, glyph: GlyphId) -> u16 {
        self.parsed().glyph_hor_advance(glyph).unwrap_or(0)
    }

    fn units_per_em(self) -> f64 {
        f64::from(self.parsed().units_per_em())
    }
}

/// The face for a text symbol's bold and italic flags.
pub fn face(bold: bool, italic: bool) -> FontFace {
    match (bold, italic) {
        (false, false) => FontFace::Regular,
        (true, false) => FontFace::Bold,
        (false, true) => FontFace::Italic,
        (true, true) => FontFace::BoldItalic,
    }
}

/// Advance width of `text` in em. A character without a glyph counts with the width of `.notdef`.
pub fn text_width(text: &str, face: FontFace) -> f64 {
    text.chars().map(|c| char_width(c, face)).sum()
}

/// Advance width of one character in em ([`text_width`]).
pub fn char_width(c: char, face: FontFace) -> f64 {
    f64::from(face.advance(face.glyph(c).unwrap_or(GlyphId(0)))) / face.units_per_em()
}

/// The cap height of `face` in em, from its OS/2 table. Arimo's digits are as tall as its capitals
/// (test `digits_are_as_tall_as_capitals`), so a control number of height h is set at h / cap height.
pub fn cap_height(face: FontFace) -> f64 {
    let parsed = face.parsed();
    // Every Arimo face has an OS/2 table version 2 or later; the fallback is the ascent.
    parsed
        .capital_height()
        .map_or(ASCENT, |h| f64::from(h) / face.units_per_em())
}

/// Arimo's ascent and line spacing in em, from its hhea table: ascender 1854, descender −434,
/// line gap 67 at 2048 units per em, the same as Arial (reference §6; test
/// `metrics_follow_the_hhea_table`).
pub const ASCENT: f64 = 1854.0 / 2048.0;
pub const DESCENT: f64 = 434.0 / 2048.0;
pub const LINE_SPACING: f64 = (1854.0 + 434.0 + 67.0) / 2048.0;

/// Families Arimo matches metrically, compared without regard to case.
const METRIC_TWINS: [&str; 3] = ["arial", "arimo", "liberation sans"];

/// True for a font family that Arimo replaces although it is not metric-compatible, so the text
/// may look different from the map author's intent.
pub fn substituted(family: &str) -> bool {
    let family = family.trim();
    !METRIC_TWINS.iter().any(|t| t.eq_ignore_ascii_case(family))
}

/// The glyphs one face shows on a page.
struct FaceUse {
    remapper: GlyphRemapper,
    /// The character of each new glyph id (CID), `None` for `.notdef`.
    unicode: Vec<Option<char>>,
}

impl FaceUse {
    fn new() -> Self {
        FaceUse {
            remapper: GlyphRemapper::new(),
            unicode: vec![None],
        }
    }
}

/// The fonts of one PDF page: which glyphs of which faces its texts use.
pub(crate) struct PageFonts {
    used: [Option<FaceUse>; 4],
    missing: BTreeSet<char>,
}

impl PageFonts {
    pub(crate) fn new() -> Self {
        PageFonts {
            used: [None, None, None, None],
            missing: BTreeSet::new(),
        }
    }

    /// The resource name of `face` on the page.
    pub(crate) fn resource_name(face: FontFace) -> Name<'static> {
        Name(match face {
            FontFace::Regular => b"F1",
            FontFace::Bold => b"F2",
            FontFace::Italic => b"F3",
            FontFace::BoldItalic => b"F4",
        })
    }

    /// `text` as two-byte CIDs of `face` for a show-text operator. The glyphs join the face's
    /// subset; a character without a glyph is shown as `.notdef` (CID 0) and reported.
    pub(crate) fn encode(&mut self, face: FontFace, text: &str) -> Vec<u8> {
        let usage = self.used[face.index()].get_or_insert_with(FaceUse::new);
        let mut cids = Vec::with_capacity(2 * text.len());
        for c in text.chars() {
            let cid = match face.glyph(c) {
                Some(glyph) => {
                    let cid = usage.remapper.remap(glyph.0);
                    if usize::from(cid) == usage.unicode.len() {
                        usage.unicode.push(Some(c));
                    }
                    cid
                }
                None => {
                    self.missing.insert(c);
                    0
                }
            };
            cids.extend(cid.to_be_bytes());
        }
        cids
    }

    /// Allocates the PDF objects of the used faces from `next`.
    pub(crate) fn finish(self, next: &mut Ref) -> FontObjects {
        let mut fonts = Vec::new();
        for (face, usage) in FACES.into_iter().zip(self.used) {
            if let Some(usage) = usage {
                fonts.push(FontObject {
                    face,
                    usage,
                    type0: next.bump(),
                    cid_font: next.bump(),
                    descriptor: next.bump(),
                    font_file: next.bump(),
                    to_unicode: next.bump(),
                });
            }
        }
        FontObjects {
            fonts,
            missing: self.missing,
        }
    }
}

struct FontObject {
    face: FontFace,
    usage: FaceUse,
    type0: Ref,
    cid_font: Ref,
    descriptor: Ref,
    font_file: Ref,
    to_unicode: Ref,
}

/// The font objects of a page, with their references allocated, ready to be written.
pub(crate) struct FontObjects {
    fonts: Vec<FontObject>,
    /// Characters without a glyph in the face that was to show them.
    pub(crate) missing: BTreeSet<char>,
}

impl FontObjects {
    /// (resource name, Type0 font) of each embedded face, for the page's resources.
    pub(crate) fn resources(&self) -> impl Iterator<Item = (Name<'static>, Ref)> + '_ {
        self.fonts
            .iter()
            .map(|f| (PageFonts::resource_name(f.face), f.type0))
    }

    /// Writes every used face as a Type0 font over a CIDFontType2 with the subset embedded.
    pub(crate) fn write(&self, pdf: &mut Pdf) -> Result<(), PdfError> {
        for font in &self.fonts {
            write_font(pdf, font)?;
        }
        Ok(())
    }
}

/// A subset tag (six capital letters) derived from the glyph set, so equal subsets get equal
/// names and different subsets of one face different names.
fn subset_tag(glyphs: impl Iterator<Item = u16>) -> String {
    // FNV-1a.
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for g in glyphs {
        for b in g.to_be_bytes() {
            hash ^= u64::from(b);
            hash = hash.wrapping_mul(0x0100_0000_01b3);
        }
    }
    (0..6)
        // `% 26` keeps the value below 26, so the cast is exact.
        .map(|i| char::from(b'A' + ((hash >> (5 * i)) % 26) as u8))
        .collect()
}

fn write_font(pdf: &mut Pdf, font: &FontObject) -> Result<(), PdfError> {
    let face = font.face;
    let parsed = face.parsed();
    let scale = 1000.0 / face.units_per_em();
    let to_pdf = |v: f64| (v * scale) as f32;
    let base_font = format!(
        "{}+{}",
        subset_tag(font.usage.remapper.remapped_gids()),
        face.postscript_name()
    );
    let system_info = SystemInfo {
        registry: Str(b"Adobe"),
        ordering: Str(b"Identity"),
        supplement: 0,
    };

    pdf.type0_font(font.type0)
        .base_font(Name(base_font.as_bytes()))
        .encoding_predefined(Name(b"Identity-H"))
        .descendant_font(font.cid_font)
        .to_unicode(font.to_unicode);

    // The subset numbers its glyphs as the remapper does, and the CIDs shown are those new glyph
    // ids, so CID = GID in the embedded font.
    let mut cid_font = pdf.cid_font(font.cid_font);
    cid_font
        .subtype(CidFontType::Type2)
        .base_font(Name(base_font.as_bytes()))
        .system_info(system_info)
        .font_descriptor(font.descriptor)
        .cid_to_gid_map_predefined(Name(b"Identity"));
    let widths: Vec<f32> = font
        .usage
        .remapper
        .remapped_gids()
        .map(|g| to_pdf(f64::from(face.advance(GlyphId(g)))))
        .collect();
    cid_font.widths().consecutive(0, widths);
    cid_font.finish();

    let bbox = parsed.global_bounding_box();
    let mut flags = FontFlags::NON_SYMBOLIC;
    if matches!(face, FontFace::Italic | FontFace::BoldItalic) {
        flags |= FontFlags::ITALIC;
    }
    // Stem width estimated from the weight class, as is common when the font does not state it.
    let weight = f32::from(parsed.weight().to_number());
    pdf.font_descriptor(font.descriptor)
        .name(Name(base_font.as_bytes()))
        .flags(flags)
        .bbox(Rect::new(
            to_pdf(f64::from(bbox.x_min)),
            to_pdf(f64::from(bbox.y_min)),
            to_pdf(f64::from(bbox.x_max)),
            to_pdf(f64::from(bbox.y_max)),
        ))
        .italic_angle(parsed.italic_angle())
        .ascent(to_pdf(f64::from(parsed.ascender())))
        .descent(to_pdf(f64::from(parsed.descender())))
        .cap_height(to_pdf(f64::from(
            parsed.capital_height().unwrap_or(parsed.ascender()),
        )))
        .stem_v(10.0 + 0.244 * (weight - 50.0))
        .font_file2(font.font_file);

    // Arimo is a plain TrueType font, which `subsetter` handles; the test
    // `every_face_subsets` covers all four files.
    let font_error = |message: String| PdfError::Font {
        face: face.postscript_name(),
        message,
    };
    let subset = subsetter::subset(face.data(), 0, &font.usage.remapper)
        .map_err(|e| font_error(format!("{e:?}")))?;
    let length = i32::try_from(subset.len())
        .map_err(|_| font_error(format!("subset of {} bytes", subset.len())))?;
    let compressed = miniz_oxide::deflate::compress_to_vec_zlib(&subset, 6);
    pdf.stream(font.font_file, &compressed)
        .filter(Filter::FlateDecode)
        .pair(Name(b"Length1"), length);

    let mut cmap = UnicodeCmap::new(Name(b"Custom"), system_info);
    for (cid, c) in font.usage.unicode.iter().enumerate() {
        if let Some(c) = c {
            // The remapper hands out u16 glyph ids, one per entry.
            let cid = u16::try_from(cid).map_err(|_| font_error(format!("glyph id {cid}")))?;
            cmap.pair(cid, *c);
        }
    }
    let compressed = miniz_oxide::deflate::compress_to_vec_zlib(&cmap.finish(), 6);
    pdf.stream(font.to_unicode, &compressed)
        .filter(Filter::FlateDecode);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arimo_width_matches_arial() {
        let arial = 0.722 + 0.556 + 0.278 + 0.278 + 0.556;
        let w = text_width("Hütte", FontFace::Regular);
        assert!((w - arial).abs() / arial < 0.01, "{w} vs {arial}");
    }

    #[test]
    fn unknown_family_is_substituted() {
        assert!(substituted("Frutiger"));
        assert!(!substituted("Arial"));
        assert!(!substituted("arimo"));
        assert!(!substituted("Liberation Sans"));
        assert!(!substituted(" ARIAL "));
    }

    #[test]
    fn embedded_faces_parse() {
        for face in FACES {
            let parsed = face.parsed();
            assert_eq!(
                parsed.is_bold(),
                matches!(face, FontFace::Bold | FontFace::BoldItalic)
            );
            assert_eq!(
                parsed.is_italic(),
                matches!(face, FontFace::Italic | FontFace::BoldItalic)
            );
        }
    }

    #[test]
    fn metrics_follow_the_hhea_table() {
        for face in FACES {
            let f = face.parsed();
            let em = face.units_per_em();
            assert!((f64::from(f.ascender()) / em - ASCENT).abs() < 1e-9);
            assert!((-f64::from(f.descender()) / em - DESCENT).abs() < 1e-9);
            let spacing = f64::from(f.ascender() - f.descender() + f.line_gap()) / em;
            assert!((spacing - LINE_SPACING).abs() < 1e-9);
        }
    }

    #[test]
    fn digits_are_as_tall_as_capitals() {
        let face = FontFace::Regular;
        let f = face.parsed();
        let em = face.units_per_em();
        let cap = cap_height(face);
        assert!(cap > 0.6 && cap < 0.8, "{cap}");
        for d in '0'..='9' {
            let bbox = f.glyph_bounding_box(face.glyph(d).unwrap()).unwrap();
            // Round digits overshoot the cap height by a few units.
            let top = f64::from(bbox.y_max) / em;
            assert!((top - cap).abs() < 0.02, "{d}: {top} vs {cap}");
        }
    }

    #[test]
    fn bold_is_wider_and_missing_characters_count_as_notdef() {
        assert!(text_width("Wald", FontFace::Bold) > text_width("Wald", FontFace::Regular));
        assert_eq!(
            text_width("中", FontFace::Regular),
            f64::from(FontFace::Regular.advance(GlyphId(0))) / 2048.0
        );
    }

    #[test]
    fn every_face_subsets() {
        for face in FACES {
            let mut fonts = PageFonts::new();
            fonts.encode(face, "Řeka");
            let mut next = Ref::new(1);
            let objects = fonts.finish(&mut next);
            let mut pdf = Pdf::new();
            objects.write(&mut pdf).unwrap();
            assert_eq!(objects.resources().count(), 1);
        }
    }

    #[test]
    fn encoding_reuses_cids_and_reports_missing_characters() {
        let mut fonts = PageFonts::new();
        assert_eq!(
            fonts.encode(FontFace::Regular, "abca"),
            [0, 1, 0, 2, 0, 3, 0, 1]
        );
        assert_eq!(fonts.encode(FontFace::Bold, "b✈"), [0, 1, 0, 0]);
        assert_eq!(fonts.missing, BTreeSet::from(['✈']));
    }
}
