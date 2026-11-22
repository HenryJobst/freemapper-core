// SPDX-License-Identifier: AGPL-3.0-or-later

//! The font writer through the public API, as an app with its own PDF page uses it (ADR 0045): encode texts,
//! take the resources, write the subsets.

use freemapper_render::fonts::{FontFace, PageFonts, text_width};
use pdf_writer::{Content, Finish, Pdf, Ref};

#[test]
fn an_app_page_embeds_arimo_and_shows_polish_letters() {
    let mut fonts = PageFonts::new();
    let bytes = fonts.encode(FontFace::Regular, "Słubice");
    assert_eq!(bytes.len(), 2 * "Słubice".chars().count());

    let mut next = Ref::new(10);
    let objects = fonts.finish(&mut next);
    assert!(objects.missing.is_empty());

    let resources: Vec<_> = objects.resources().collect();
    assert_eq!(resources.len(), 1);
    assert_eq!(resources[0].0, PageFonts::resource_name(FontFace::Regular));

    let mut pdf = Pdf::new();
    let mut content = Content::new();
    content.begin_text();
    content.set_font(PageFonts::resource_name(FontFace::Regular), 12.0);
    content.show(pdf_writer::Str(&bytes));
    content.end_text();
    let page = Ref::new(1);
    let mut p = pdf.page(page);
    let mut res = p.resources();
    let mut font_dict = res.fonts();
    for (name, font) in objects.resources() {
        font_dict.pair(name, font);
    }
    font_dict.finish();
    res.finish();
    p.finish();
    pdf.stream(Ref::new(2), &content.finish());
    objects.write(&mut pdf).expect("the subsets are written");

    let file = pdf.finish();
    let text = String::from_utf8_lossy(&file);
    assert!(text.contains("Identity-H"));
    assert!(text.contains("Arimo"));
    assert!(text_width("Słubice", FontFace::Regular) > 0.0);
}

#[test]
fn a_character_without_a_glyph_is_reported_as_missing() {
    let mut fonts = PageFonts::new();
    let bytes = fonts.encode(FontFace::Bold, "a中");
    assert_eq!(bytes.len(), 4);
    assert_eq!(
        &bytes[2..],
        &[0, 0],
        "the missing character shows as .notdef"
    );
    let objects = fonts.finish(&mut Ref::new(1));
    assert_eq!(objects.missing, ['中'].into_iter().collect());
}
