//! The output style of the writer: elements laid out as .NET's `XmlTextWriter` with `Formatting.Indented`
//! writes them, escaping, and the text of numbers.

use super::{LineEnding, PpenWriteError};

/// How the output looks: byte order mark, line end and the indent of one level.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Style {
    pub bom: bool,
    pub newline: LineEnding,
    pub indent: String,
}

impl Style {
    /// Purple Pen's own style with the line end `newline`: a byte order mark and two spaces of indent.
    pub(super) fn purple_pen(newline: LineEnding) -> Style {
        Style {
            bom: true,
            newline,
            indent: "  ".into(),
        }
    }
}

/// One element to write. Attributes are written in the order given, with their values escaped here.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Element {
    pub name: &'static str,
    pub attrs: Vec<(String, String)>,
    pub body: Body,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) enum Body {
    Empty,
    /// Text content, escaped here; the element stays on one line.
    Text(String),
    /// Child elements, one per line. An empty list is written as an empty element.
    Children(Vec<Child>),
}

#[derive(Debug, Clone, PartialEq)]
pub(super) enum Child {
    Element(Element),
    /// Bytes carried from a base file, written as they are: they bring their own indent and line end.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "carried bytes come with the patch still to come")
    )]
    Raw(String),
}

/// The text of `v` as Purple Pen stores it: an `f32` in its shortest round-trip form, no exponent, `-0` as `0`.
/// A value that is not finite or lies outside the range of `f32` is `Invalid`, naming `what`.
pub(super) fn num(v: f64, what: &str) -> Result<String, PpenWriteError> {
    if !v.is_finite() {
        return Err(PpenWriteError::Invalid(format!(
            "{what} is not a finite number: {v}"
        )));
    }
    let f = v as f32;
    if !f.is_finite() {
        return Err(PpenWriteError::Invalid(format!(
            "{what} is outside the range of a .ppen number: {v}"
        )));
    }
    if f == 0.0 {
        return Ok("0".into());
    }
    // `Display` of a float gives the shortest text that reads back as the same value, never with an exponent.
    Ok(f.to_string())
}

/// Refuses `s` if it holds a character XML 1.0 does not allow (control characters other than tab, line feed
/// and carriage return, U+FFFE, U+FFFF), naming `what`.
pub(super) fn text_ok(s: &str, what: &str) -> Result<(), PpenWriteError> {
    match s.chars().find(|c| !xml_char(*c)) {
        Some(c) => Err(PpenWriteError::Invalid(format!(
            "{what} holds the character U+{:04X}, which XML 1.0 does not allow",
            u32::from(c)
        ))),
        None => Ok(()),
    }
}

fn xml_char(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\r' | '\u{20}'..='\u{D7FF}' | '\u{E000}'..='\u{FFFD}' | '\u{10000}'..)
}

/// Appends `e` at nesting `depth` (0 for the root), its line end included.
pub(super) fn write_element(out: &mut String, e: &Element, depth: usize, style: &Style) {
    let newline = style.newline.as_str();
    push_indent(out, depth, style);
    out.push('<');
    out.push_str(e.name);
    for (name, value) in &e.attrs {
        out.push(' ');
        out.push_str(name);
        out.push_str("=\"");
        escape_attr(out, value);
        out.push('"');
    }
    match &e.body {
        Body::Text(t) => {
            out.push('>');
            escape_text(out, t);
            out.push_str("</");
            out.push_str(e.name);
            out.push('>');
        }
        Body::Children(children) if !children.is_empty() => {
            out.push('>');
            out.push_str(newline);
            for c in children {
                match c {
                    Child::Element(child) => write_element(out, child, depth + 1, style),
                    Child::Raw(raw) => out.push_str(raw),
                }
            }
            push_indent(out, depth, style);
            out.push_str("</");
            out.push_str(e.name);
            out.push('>');
        }
        Body::Empty | Body::Children(_) => out.push_str(" />"),
    }
    out.push_str(newline);
}

fn push_indent(out: &mut String, depth: usize, style: &Style) {
    for _ in 0..depth {
        out.push_str(&style.indent);
    }
}

fn escape_text(out: &mut String, s: &str) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            c => out.push(c),
        }
    }
}

pub(super) fn escape_attr(out: &mut String, s: &str) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\n' => out.push_str("&#xA;"),
            '\r' => out.push_str("&#xD;"),
            '\t' => out.push_str("&#x9;"),
            c => out.push(c),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn el(name: &'static str, attrs: &[(&str, &str)], body: Body) -> Element {
        Element {
            name,
            attrs: attrs
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            body,
        }
    }

    #[test]
    fn num_is_shortest_f32() {
        assert_eq!(num(140.207184, "x").unwrap(), "140.20718");
        assert_eq!(num(69.6024857, "x").unwrap(), "69.602486");
        assert_eq!(num(-6.789, "x").unwrap(), "-6.789");
        assert_eq!(num(-0.0, "x").unwrap(), "0");
        assert_eq!(num(1e-7, "x").unwrap(), "0.0000001");
        assert_eq!(num(5000.0, "x").unwrap(), "5000");
        for bad in [f64::NAN, f64::INFINITY, 1e40, -1e40] {
            match num(bad, "control 31 x") {
                Err(PpenWriteError::Invalid(m)) => assert!(m.contains("control 31 x"), "{m}"),
                other => panic!("{bad}: {other:?}"),
            }
        }
    }

    #[test]
    fn escapes_text_and_attributes() {
        let style = Style::purple_pen(LineEnding::Lf);
        let mut out = String::new();
        write_element(
            &mut out,
            &el("name", &[], Body::Text("A<&\"B>".into())),
            0,
            &style,
        );
        assert_eq!(out, "<name>A&lt;&amp;\"B&gt;</name>\n");
        let mut out = String::new();
        write_element(
            &mut out,
            &el("x", &[("a", "A<&\"B"), ("b", "1\n2\r3\t4'>")], Body::Empty),
            0,
            &style,
        );
        assert_eq!(
            out,
            "<x a=\"A&lt;&amp;&quot;B\" b=\"1&#xA;2&#xD;3&#x9;4'&gt;\" />\n"
        );
        assert!(text_ok("a\tb\nc\r\u{E000}\u{10FFFF}", "name").is_ok());
        for bad in ["a\u{1}", "\u{0}", "x\u{FFFE}", "\u{FFFF}"] {
            match text_ok(bad, "course name") {
                Err(PpenWriteError::Invalid(m)) => assert!(m.contains("course name"), "{m}"),
                other => panic!("{bad:?}: {other:?}"),
            }
        }
    }

    #[test]
    fn writes_purple_pen_layout() {
        let style = Style::purple_pen(LineEnding::Crlf);
        let location = el(
            "location",
            &[
                ("x", &num(140.207184, "x").unwrap()),
                ("y", &num(57.44655, "y").unwrap()),
            ],
            Body::Empty,
        );
        let control = el(
            "control",
            &[("id", "1"), ("kind", "start")],
            Body::Children(vec![Child::Element(location)]),
        );
        let mut out = String::new();
        write_element(&mut out, &control, 1, &style);
        assert_eq!(
            out,
            "  <control id=\"1\" kind=\"start\">\r\n    <location x=\"140.20718\" y=\"57.44655\" />\r\n  </control>\r\n"
        );
    }

    #[test]
    fn layout_of_text_raw_and_empty_children() {
        let style = Style {
            bom: false,
            newline: LineEnding::Lf,
            indent: "\t".into(),
        };
        let course = el(
            "course",
            &[("id", "2")],
            Body::Children(vec![
                Child::Element(el("name", &[], Body::Text("Long".into()))),
                Child::Raw("\t\t<labels label-kind=\"sequence\" />\n".into()),
                Child::Element(el("bends", &[], Body::Children(Vec::new()))),
            ]),
        );
        let mut out = String::new();
        write_element(&mut out, &course, 1, &style);
        assert_eq!(
            out,
            "\t<course id=\"2\">\n\t\t<name>Long</name>\n\t\t<labels label-kind=\"sequence\" />\n\t\t<bends />\n\t</course>\n"
        );
    }
}
