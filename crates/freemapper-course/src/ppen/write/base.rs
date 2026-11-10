//! The lossless parse of a base file for a patch. [`parse_base`] splits the text into the top-level objects
//! and the bytes around them (byte order mark, root tags, `<event>`, comments, whitespace), which a patch
//! writes back verbatim. It takes the output style from the file and refuses a file from a Purple Pen version
//! the writer does not know (see the module header of `write.rs`). The guards are those of the reader: file
//! size, DOCTYPE and entities (shared with `xml::drive`), and `MAX_XML_DEPTH`.

use std::collections::HashSet;

use quick_xml::events::{BytesStart, Event};
use quick_xml::{Reader, XmlVersion};

use super::out::Style;
use super::{LineEnding, PpenWriteError};
use crate::CourseError;
use crate::model::MAX_XML_DEPTH;
use crate::ppen::ROOT;
use crate::xml::{check_size, guard, xml_err};

/// A range of bytes in the base text.
pub(super) type Span = std::ops::Range<usize>;

/// The object stores of a Purple Pen event, in the order `EventDB.Save` writes them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) enum Store {
    Control,
    Course,
    CourseControl,
    Leg,
    Special,
}

impl Store {
    fn of(name: &str) -> Option<Store> {
        Some(match name {
            "control" => Store::Control,
            "course" => Store::Course,
            "course-control" => Store::CourseControl,
            "leg" => Store::Leg,
            "special-object" => Store::Special,
            _ => return None,
        })
    }

    /// The element name of an object of this store.
    pub(super) fn element(self) -> &'static str {
        match self {
            Store::Control => "control",
            Store::Course => "course",
            Store::CourseControl => "course-control",
            Store::Leg => "leg",
            Store::Special => "special-object",
        }
    }
}

/// One top-level object of the base file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Object<'a> {
    pub kind: Store,
    pub id: i64,
    /// The element, from the whitespace that leads up to its start tag to the end of its end tag.
    pub span: Span,
    /// The start tag (or the whole element, if it is empty), from its `<` to its `>`.
    pub head: Span,
    /// The attributes as written: names, and values without quotes and not unescaped.
    pub attrs: Vec<(&'a str, &'a str)>,
    /// The direct child elements, each from its `<` to the end of its last tag, without the whitespace
    /// around it.
    pub children: Vec<Span>,
}

/// A base file split for a patch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Base<'a> {
    pub text: &'a str,
    pub style: Style,
    /// The `<event>` element, from its `<` to the end of its end tag. Empty, at the end of the root start tag,
    /// if the file has none.
    pub event: Span,
    /// The objects in file order.
    pub objects: Vec<Object<'a>>,
    /// The offset just past the last top-level element (the last object, else the event, else the root
    /// start tag). `text[tail..]` holds the root end tag.
    pub tail: usize,
}

/// The base file as text: UTF-8, at most `MAX_COURSE_FILE_BYTES`.
pub(super) fn base_text(bytes: &[u8]) -> Result<&str, PpenWriteError> {
    check_size(bytes.len())?;
    if bytes.starts_with(&[0xFF, 0xFE]) || bytes.starts_with(&[0xFE, 0xFF]) {
        return Err(PpenWriteError::UnknownVersion(
            "the file is UTF-16; Purple Pen writes UTF-8".into(),
        ));
    }
    let text = std::str::from_utf8(bytes)
        .map_err(|e| PpenWriteError::UnknownVersion(format!("the file is not UTF-8 ({e})")))?;
    // XML allows no NUL anywhere; ASCII as UTF-16 without a byte order mark is valid UTF-8 full of them.
    if text.contains('\0') {
        return Err(PpenWriteError::UnknownVersion(
            "the file holds NUL characters, as UTF-16 without a byte order mark does; Purple Pen writes UTF-8"
                .into(),
        ));
    }
    Ok(text)
}

/// Splits `text` into its top-level objects and the bytes around them, and checks that Purple Pen as the
/// writer knows it wrote the file.
pub(super) fn parse_base(text: &str) -> Result<Base<'_>, PpenWriteError> {
    check_size(text.len())?;
    let mut p = Parser {
        text,
        depth: 0,
        root: None,
        root_closed: false,
        event: None,
        event_start: None,
        open: None,
        objects: Vec::new(),
        ids: HashSet::new(),
        last: None,
        indent: None,
        tail: 0,
    };
    let mut reader = Reader::from_str(text);
    // quick-xml drops a byte order mark and counts its positions from the byte after it.
    let bom = if text.starts_with('\u{FEFF}') { 3 } else { 0 };
    let at =
        |position: u64| usize::try_from(position).map_or(usize::MAX, |p| p.saturating_add(bom));
    loop {
        let before = at(reader.buffer_position());
        let event = reader.read_event().map_err(xml_err)?;
        let after = at(reader.buffer_position());
        guard(&event)?;
        match event {
            Event::Decl(d) => declaration(&d)?,
            Event::Start(e) => p.begin(&e, before, after, false)?,
            Event::Empty(e) => p.begin(&e, before, after, true)?,
            Event::End(_) => p.end(after)?,
            Event::Text(t) => {
                if !t.as_ref().trim_ascii().is_empty() {
                    p.content("text")?;
                }
            }
            Event::CData(_) => p.content("a CDATA section")?,
            Event::GeneralRef(_) => p.content("text")?,
            Event::Eof => return p.finish(),
            _ => {}
        }
    }
}

/// An XML declaration is kept if it names version 1.0 and, if any, the encoding UTF-8.
fn declaration(d: &quick_xml::events::BytesDecl<'_>) -> Result<(), PpenWriteError> {
    let version = d.version().map_err(xml_err)?;
    if version.as_ref() != "1.0" {
        return Err(PpenWriteError::UnknownVersion(format!(
            "XML version {version}"
        )));
    }
    if let Some(encoding) = d.encoding() {
        let encoding = encoding.map_err(xml_err)?;
        if !encoding.eq_ignore_ascii_case("utf-8") {
            return Err(PpenWriteError::UnknownVersion(format!(
                "encoding {encoding}; Purple Pen writes UTF-8"
            )));
        }
    }
    Ok(())
}

struct Open<'a> {
    kind: Store,
    id: i64,
    start: usize,
    head: Span,
    attrs: Vec<(&'a str, &'a str)>,
    children: Vec<Span>,
    child_start: usize,
}

struct Parser<'a> {
    text: &'a str,
    /// Elements open around the current position.
    depth: usize,
    /// The end of the root start tag, once seen.
    root: Option<usize>,
    root_closed: bool,
    event: Option<Span>,
    event_start: Option<usize>,
    open: Option<Open<'a>>,
    objects: Vec<Object<'a>>,
    ids: HashSet<(Store, i64)>,
    /// The store of the last top-level element (`None` for `<event>`), and its name.
    last: Option<(Option<Store>, String)>,
    indent: Option<String>,
    tail: usize,
}

impl<'a> Parser<'a> {
    /// A start tag (or an empty element) read from `before..after`.
    fn begin(
        &mut self,
        e: &BytesStart<'_>,
        before: usize,
        after: usize,
        empty: bool,
    ) -> Result<(), PpenWriteError> {
        if self.depth >= MAX_XML_DEPTH {
            return Err(CourseError::Limit(format!(
                "nesting too deep: more than {MAX_XML_DEPTH} levels of elements"
            ))
            .into());
        }
        // The first event of a file can carry the byte order mark before its `<`.
        let lt = self
            .text
            .get(before..after)
            .and_then(|s| s.find('<'))
            .map_or(before, |i| before + i);
        let attrs = self.attributes(e)?;
        let name = e.name().into_inner().to_string();
        match self.depth {
            0 => {
                if self.root_closed {
                    return Err(CourseError::Xml("an element after the root element".into()).into());
                }
                if name != ROOT {
                    return Err(PpenWriteError::UnknownVersion(format!(
                        "root element <{name}>, expected <{ROOT}>"
                    )));
                }
                if let Some((attr, _)) = attrs.first() {
                    return Err(PpenWriteError::UnknownVersion(format!(
                        "attribute {attr} on <{ROOT}>"
                    )));
                }
                self.root = Some(after);
                self.tail = after;
                if empty {
                    self.root_closed = true;
                }
            }
            1 => {
                if self.indent.is_none() {
                    self.indent = Some(self.line_indent(lt));
                }
                // `EventDB.Load` reads each store as one run of elements, in this order, and stops a store at
                // the first element of another; an object out of order would be lost in Purple Pen.
                let store = Store::of(&name);
                if let Some((last, last_name)) = &self.last
                    && store.is_some()
                    && store < *last
                {
                    return Err(PpenWriteError::UnknownVersion(format!(
                        "<{name}> after <{last_name}>; Purple Pen writes the objects of each kind together, \
                         in the order event, control, course, course-control, leg, special-object"
                    )));
                }

                if name == "event" {
                    if self.event.is_some() || self.event_start.is_some() {
                        return Err(PpenWriteError::UnknownVersion(format!(
                            "a second <event> in <{ROOT}>"
                        )));
                    }
                    if let Some((Some(_), last_name)) = &self.last {
                        return Err(PpenWriteError::UnknownVersion(format!(
                            "<event> after <{last_name}>; Purple Pen writes the event first"
                        )));
                    }
                    if empty {
                        self.event = Some(lt..after);
                        self.tail = after;
                    } else {
                        self.event_start = Some(lt);
                    }
                } else {
                    let kind = Store::of(&name).ok_or_else(|| {
                        PpenWriteError::UnknownVersion(format!("element <{name}> in <{ROOT}>"))
                    })?;
                    let id = attrs
                        .iter()
                        .find(|(k, _)| *k == "id")
                        .and_then(|(_, v)| v.trim().parse::<i64>().ok())
                        .ok_or_else(|| CourseError::Xml(format!("<{name}> lacks a valid id")))?;
                    if !(1..=i64::from(i32::MAX)).contains(&id) {
                        return Err(CourseError::Xml(format!(
                            "<{name}> has the id {id}; Purple Pen ids run from 1 to {}",
                            i32::MAX
                        ))
                        .into());
                    }
                    if !self.ids.insert((kind, id)) {
                        return Err(
                            CourseError::Xml(format!("{name} id {id} is defined twice")).into()
                        );
                    }
                    let start = self.leading_whitespace(lt);
                    let open = Open {
                        kind,
                        id,
                        start,
                        head: lt..after,
                        attrs,
                        children: Vec::new(),
                        child_start: 0,
                    };
                    if empty {
                        self.close_object(open, after);
                    } else {
                        self.open = Some(open);
                    }
                }
                self.last = Some((store, name.clone()));
            }
            2 => {
                if let Some(o) = self.open.as_mut() {
                    if empty {
                        o.children.push(lt..after);
                    } else {
                        o.child_start = lt;
                    }
                }
            }
            _ => {}
        }
        if !empty {
            self.depth += 1;
        }
        Ok(())
    }

    /// An end tag that ends at `after`.
    fn end(&mut self, after: usize) -> Result<(), PpenWriteError> {
        self.depth = self
            .depth
            .checked_sub(1)
            .ok_or_else(|| CourseError::Xml("an end tag without a start tag".into()))?;
        match self.depth {
            0 => self.root_closed = true,
            1 => {
                if let Some(o) = self.open.take() {
                    self.close_object(o, after);
                } else if let Some(start) = self.event_start.take() {
                    self.event = Some(start..after);
                    self.tail = after;
                }
            }
            2 => {
                if let Some(o) = self.open.as_mut() {
                    o.children.push(o.child_start..after);
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Text, CDATA or a reference outside any element of an object.
    fn content(&self, what: &str) -> Result<(), PpenWriteError> {
        match self.depth {
            0 => Err(CourseError::Xml(format!("{what} outside the root element")).into()),
            1 => Err(PpenWriteError::UnknownVersion(format!(
                "{what} directly in <{ROOT}>"
            ))),
            _ => Ok(()),
        }
    }

    fn close_object(&mut self, o: Open<'a>, after: usize) {
        self.objects.push(Object {
            kind: o.kind,
            id: o.id,
            span: o.start..after,
            head: o.head,
            attrs: o.attrs,
            children: o.children,
        });
        self.tail = after;
    }

    fn finish(self) -> Result<Base<'a>, PpenWriteError> {
        let root = self
            .root
            .ok_or_else(|| CourseError::Xml(format!("no <{ROOT}> element")))?;
        if !self.root_closed {
            return Err(CourseError::Xml(format!("the file ends inside <{ROOT}>")).into());
        }
        let text = self.text;
        let newline = match text.find('\n') {
            Some(i) if i > 0 && text.as_bytes()[i - 1] == b'\r' => LineEnding::Crlf,
            Some(_) => LineEnding::Lf,
            None => LineEnding::Crlf,
        };
        Ok(Base {
            text,
            style: Style {
                bom: text.starts_with('\u{FEFF}'),
                newline,
                indent: self.indent.unwrap_or_else(|| "  ".into()),
            },
            event: self.event.unwrap_or(root..root),
            objects: self.objects,
            tail: self.tail,
        })
    }

    /// The attributes of `e` as slices of the text. Every value must hold only references `guard` accepts.
    fn attributes(&self, e: &BytesStart<'_>) -> Result<Vec<(&'a str, &'a str)>, PpenWriteError> {
        let mut out = Vec::new();
        for a in e.attributes() {
            let a = a.map_err(xml_err)?;
            a.normalized_value(XmlVersion::Explicit1_0)
                .map_err(xml_err)?;
            let key = self.slice(a.key.as_ref())?;
            let value = self.slice(a.value.as_ref())?;
            out.push((key, value));
        }
        Ok(out)
    }

    /// `part`, which quick-xml borrowed from the text, as a slice of the text with the text's lifetime.
    fn slice(&self, part: &str) -> Result<&'a str, PpenWriteError> {
        let text = self.text;
        let start = (part.as_ptr() as usize).wrapping_sub(text.as_ptr() as usize);
        start
            .checked_add(part.len())
            .and_then(|end| text.get(start..end))
            .ok_or_else(|| CourseError::Xml("an attribute outside the text".into()).into())
    }

    /// The start of the run of whitespace before `lt`.
    fn leading_whitespace(&self, lt: usize) -> usize {
        let bytes = self.text.as_bytes();
        let mut s = lt;
        while s > 0 && bytes[s - 1].is_ascii_whitespace() {
            s -= 1;
        }
        s
    }

    /// The indent of the line on which the element at `lt` starts, if that line holds only spaces and tabs
    /// before it; else Purple Pen's two spaces.
    fn line_indent(&self, lt: usize) -> String {
        let before = &self.text[..lt];
        let line = &before[before.rfind('\n').map_or(0, |i| i + 1)..];
        if !line.is_empty() && line.bytes().all(|b| b == b' ' || b == b'\t') {
            line.to_string()
        } else {
            "  ".into()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::MAX_COURSE_FILE_BYTES;

    const LINCOLN: &str = include_str!("../../../tests/fixtures/courses/ppen/lincoln-park.ppen");
    const OVERLAPPER: &str = include_str!("../../../tests/fixtures/courses/ppen/overlapper.ppen");
    const SAMPLE8: &str = include_str!("../../../tests/fixtures/courses/ppen/sampleevent8.ppen");
    const FIXTURES: [&str; 3] = [LINCOLN, OVERLAPPER, SAMPLE8];

    fn unknown(r: Result<Base<'_>, PpenWriteError>, what: &str) {
        match r {
            Err(PpenWriteError::UnknownVersion(m)) => assert!(m.contains(what), "{m}"),
            other => panic!("expected UnknownVersion naming {what}, got {other:?}"),
        }
    }

    fn base_err(r: Result<Base<'_>, PpenWriteError>) -> CourseError {
        match r {
            Err(PpenWriteError::Base(e)) => e,
            other => panic!("expected a Base error, got {other:?}"),
        }
    }

    #[test]
    fn style_detected_from_base() {
        for f in FIXTURES {
            let base = parse_base(f).unwrap();
            assert_eq!(base.style, Style::purple_pen(LineEnding::Crlf));
            let lf = f.trim_start_matches('\u{FEFF}').replace("\r\n", "\n");
            let base = parse_base(&lf).unwrap();
            assert_eq!(
                base.style,
                Style {
                    bom: false,
                    newline: LineEnding::Lf,
                    indent: "  ".into()
                }
            );
            let tabs = lf.replace("\n  <", "\n\t<");
            assert_eq!(parse_base(&tabs).unwrap().style.indent, "\t");
        }
    }

    #[test]
    fn base_spans_cover_the_file() {
        for f in FIXTURES {
            let base = parse_base(f).unwrap();
            let mut rebuilt = String::new();
            let mut at = 0;
            for o in &base.objects {
                assert!(o.span.start >= at, "objects overlap");
                let gap = &f[at..o.span.start];
                for name in [
                    "<control ",
                    "<course ",
                    "<course-control ",
                    "<leg ",
                    "<special-object ",
                ] {
                    assert!(!gap.contains(name), "object outside a span: {gap}");
                }
                rebuilt.push_str(gap);
                let s = &f[o.span.clone()];
                rebuilt.push_str(s);
                let element = s.trim_start();
                assert!(
                    s[..s.len() - element.len()]
                        .bytes()
                        .all(|b| b.is_ascii_whitespace())
                );
                assert!(element.starts_with(&format!("<{} ", o.kind.element())));
                assert!(
                    element.ends_with(&format!("</{}>", o.kind.element()))
                        || element.ends_with("/>")
                );
                for c in &o.children {
                    let c = &f[c.clone()];
                    assert!(c.starts_with('<') && c.ends_with('>'), "{c}");
                }
                at = o.span.end;
            }
            rebuilt.push_str(&f[at..]);
            assert_eq!(rebuilt, f);
            assert_eq!(base.tail, at);
            assert_eq!(f[base.tail..].trim(), format!("</{ROOT}>"));
            let event = &f[base.event.clone()];
            assert!(event.starts_with("<event id=\"1\">") && event.ends_with("</event>"));
            for (kind, marker) in [
                (Store::Control, "<control id="),
                (Store::Course, "<course id="),
                (Store::CourseControl, "<course-control id="),
                (Store::Leg, "<leg id="),
                (Store::Special, "<special-object id="),
            ] {
                let n = base.objects.iter().filter(|o| o.kind == kind).count();
                assert_eq!(n, f.matches(marker).count(), "{marker}");
            }
        }
    }

    #[test]
    fn objects_carry_ids_attributes_and_children() {
        let base = parse_base(LINCOLN).unwrap();
        let c = &base.objects[1];
        assert_eq!((c.kind, c.id), (Store::Control, 2));
        assert_eq!(c.attrs, [("id", "2"), ("kind", "normal")]);
        assert!(LINCOLN[c.span.clone()].starts_with("\r\n  <control id=\"2\""));
        assert_eq!(
            LINCOLN[c.head.clone()],
            *"<control id=\"2\" kind=\"normal\">"
        );
        let children: Vec<&str> = c.children.iter().map(|s| &LINCOLN[s.clone()]).collect();
        assert_eq!(children.len(), 5);
        assert_eq!(children[0], "<code>31</code>");
        assert_eq!(
            children[1],
            "<location x=\"127.508255\" y=\"110.638184\" />"
        );
        let leg = base.objects.iter().find(|o| o.kind == Store::Leg).unwrap();
        assert_eq!(leg.id, 1);
        assert_eq!(leg.children.len(), 2);
        assert!(LINCOLN[leg.children[0].clone()].starts_with("<bends>\r\n      <location"));
        assert!(LINCOLN[leg.children[0].clone()].ends_with("</bends>"));
        let cc = base
            .objects
            .iter()
            .find(|o| o.kind == Store::CourseControl && o.id == 6);
        assert!(cc.unwrap().children.is_empty());
        let escaped = LINCOLN
            .replace("<title>Lincoln Park</title>", "<title>A &amp; B</title>")
            .replace(
                "<course id=\"1\" kind=\"normal\" order=\"1\">",
                "<course id=\"1\" kind=\"normal\" order=\"1\" note=\"a &lt; b\">",
            );
        let base = parse_base(&escaped).unwrap();
        let course = base
            .objects
            .iter()
            .find(|o| o.kind == Store::Course)
            .unwrap();
        assert_eq!(course.attrs[3], ("note", "a &lt; b"));
    }

    #[test]
    fn gate_refuses_unknown_versions() {
        let root = "<course-scribe-event>";
        unknown(
            parse_base(&LINCOLN.replace(root, "<course-scribe-event version=\"5\">")),
            "version",
        );
        let end = "</course-scribe-event>";
        unknown(
            parse_base(&LINCOLN.replace(end, &format!("  <team id=\"1\" />\r\n{end}"))),
            "<team>",
        );
        unknown(
            parse_base(
                &LINCOLN
                    .replace(root, "<event-file>")
                    .replace(end, "</event-file>"),
            ),
            "<event-file>",
        );
        unknown(
            parse_base(&LINCOLN.replace(
                "  <control id=\"1\"",
                "  <event id=\"2\" />\r\n  <control id=\"1\"",
            )),
            "second <event>",
        );
        unknown(
            parse_base(&LINCOLN.replace(end, &format!("  stray\r\n{end}"))),
            "text",
        );
        unknown(
            parse_base(&format!(
                "<?xml version=\"1.0\" encoding=\"utf-16\"?>{}",
                LINCOLN.trim_start_matches('\u{FEFF}')
            )),
            "encoding",
        );
        let utf16: Vec<u8> = LINCOLN.encode_utf16().flat_map(u16::to_le_bytes).collect();
        assert!(matches!(
            base_text(&utf16),
            Err(PpenWriteError::UnknownVersion(_))
        ));
        let no_bom: Vec<u8> = LINCOLN
            .trim_start_matches('\u{FEFF}')
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect();
        match base_text(&no_bom) {
            Err(PpenWriteError::UnknownVersion(m)) => {
                assert!(!m.contains('\0'), "{m:?}");
                assert!(m.contains("UTF-16"), "{m}");
            }
            other => panic!("{other:?}"),
        }
        // Purple Pen reads each store in one run, in `EventDB.Load`'s order.
        unknown(
            parse_base(&LINCOLN.replace(
                "  <course-control id=\"6\" control=\"5\" />",
                "  <course-control id=\"6\" control=\"5\" />\r\n  <control id=\"9\" kind=\"normal\" />",
            )),
            "<control> after <course-control>",
        );
        unknown(
            parse_base(&LINCOLN.replace(
                "  <control id=\"1\"",
                "  <control id=\"9\" kind=\"normal\" />\r\n  <event id=\"2\" />\r\n  <control id=\"1\"",
            )),
            "second <event>",
        );
        assert!(matches!(
            base_text(b"<a>\xff</a>"),
            Err(PpenWriteError::UnknownVersion(_))
        ));
        assert_eq!(base_text(LINCOLN.as_bytes()).unwrap(), LINCOLN);
        // Content below the top level passes: Purple Pen skips it, the patch carries it.
        let foo = LINCOLN.replace(
            "    <location x=\"140.207184\"",
            "    <foo><bar a=\"1\">x</bar></foo>\r\n    <location x=\"140.207184\"",
        );
        let base = parse_base(&foo).unwrap();
        assert_eq!(
            foo[base.objects[0].children[0].clone()],
            *"<foo><bar a=\"1\">x</bar></foo>"
        );
        let empty = LINCOLN.replace("<code>31</code>", "<code>31</code><foo/>");
        let base = parse_base(&empty).unwrap();
        assert_eq!(empty[base.objects[1].children[1].clone()], *"<foo/>");
        let decl = format!(
            "<?xml version=\"1.0\" encoding=\"utf-8\"?>\r\n<!-- note -->\r\n{}",
            LINCOLN.trim_start_matches('\u{FEFF}')
        );
        assert_eq!(parse_base(&decl).unwrap().objects.len(), 10);
        let comment = LINCOLN.replace(end, &format!("  <!-- kept -->\r\n{end}"));
        let base = parse_base(&comment).unwrap();
        assert!(comment[base.tail..].contains("<!-- kept -->"));
    }

    #[test]
    fn base_parse_has_reader_guards() {
        let body = LINCOLN.trim_start_matches('\u{FEFF}');
        let e = base_err(parse_base(&format!(
            "<!DOCTYPE x [<!ENTITY e \"boom\">]>{body}"
        )));
        assert!(e.to_string().contains("DOCTYPE"), "{e}");
        let e = base_err(parse_base(
            &LINCOLN.replace("<code>31</code>", "<code>&e;</code>"),
        ));
        assert!(e.to_string().contains("entity"), "{e}");
        base_err(parse_base(
            &LINCOLN.replace("kind=\"start\"", "kind=\"&e;\""),
        ));
        assert!(parse_base(&LINCOLN.replace("<code>31</code>", "<code>&#51;&amp;</code>")).is_ok());
        let deep = format!(
            "{}x{}",
            "<a>".repeat(MAX_XML_DEPTH),
            "</a>".repeat(MAX_XML_DEPTH)
        );
        let nested = LINCOLN.replace("<code>31</code>", &deep);
        assert!(matches!(
            base_err(parse_base(&nested)),
            CourseError::Limit(_)
        ));
        let mut big = String::with_capacity(MAX_COURSE_FILE_BYTES + 1);
        big.push_str(body);
        while big.len() <= MAX_COURSE_FILE_BYTES {
            big.push(' ');
        }
        assert!(matches!(base_err(parse_base(&big)), CourseError::Limit(_)));
        assert!(matches!(
            base_text(big.as_bytes()),
            Err(PpenWriteError::Base(CourseError::Limit(_)))
        ));
        // Purple Pen ids are positive `int`s.
        for id in ["0", "-1", "2147483648"] {
            let e = base_err(parse_base(
                &LINCOLN.replace("<control id=\"2\"", &format!("<control id=\"{id}\"")),
            ));
            assert!(e.to_string().contains(id), "{e}");
        }
        // Malformed or truncated files are errors, never a panic.
        base_err(parse_base(
            &LINCOLN.replace("<control id=\"2\"", "<control id=\"1\""),
        ));
        base_err(parse_base(&LINCOLN.replace("<leg id=\"1\"", "<leg")));
        base_err(parse_base(&format!("{body}<course-scribe-event />")));
        base_err(parse_base(""));
        for cut in (0..LINCOLN.len()).filter(|i| LINCOLN.is_char_boundary(*i)) {
            assert!(parse_base(&LINCOLN[..cut]).is_err(), "cut at {cut}");
        }
    }
}
