//! Copied from freemapper 743ce4b (freemapper-print ADR 0014); moved here from freemapper-print
//! 918cec2 (ADR 0012).
//! Symbol enlargement with the same rules as OOM's `Symbol::scale`.

use super::{MAX_XML_DEPTH, OmapError, too_deep};

const LINE_SYMBOL_ATTRS: &[&str] = &[
    "line_width",
    "minimum_length",
    "start_offset",
    "end_offset",
    // Before file version 8 the length of both pointed caps; OOM turns it into the offsets when
    // loading and then scales those.
    "pointed_cap_length",
    "segment_length",
    "end_length",
    // OOM scales these two counts as well; keep its behaviour.
    "minimum_mid_symbol_count",
    "minimum_mid_symbol_count_when_closed",
    "dash_length",
    "break_length",
    "in_group_break_length",
    "mid_symbol_distance",
];
const BORDER_ATTRS: &[&str] = &["width", "shift", "dash_length", "break_length"];
const POINT_SYMBOL_ATTRS: &[&str] = &["inner_radius", "outer_width"];
const PATTERN_ATTRS: &[&str] = &[
    "line_spacing",
    "line_width",
    "line_offset",
    "offset_along_line",
    "point_distance",
];
const FRAMING_ATTRS: &[&str] = &["line_half_width", "shadow_x_offset", "shadow_y_offset"];
// OOM's `TextSymbol::scale` leaves these two alone. freemapper-print scales them, so a print at
// another scale keeps the proportions of a text (freemapper-print ADR 0018: everything grows by
// the same ratio).
const TEXT_ATTRS: &[&str] = &["paragraph_spacing"];
const LINE_BELOW_ATTRS: &[&str] = &["width", "distance"];

/// How a numeric attribute value is scaled.
#[derive(Clone, Copy)]
enum Scaling {
    Linear(f64),
    Quadratic(f64),
    FontSize(f64),
}

impl Scaling {
    fn apply(self, value: f64) -> f64 {
        match self {
            Self::Linear(f) => (value * f).round(),
            Self::Quadratic(f) => (value * f * f).round(),
            // OOM keeps a minimum font size of 0.01 mm.
            Self::FontSize(f) => (value * f).round().max(10.0),
        }
    }
}

/// Enlarges every symbol dimension in an OOM `<symbols>` fragment by `factor`.
///
/// Follows OOM's `Symbol::scale` implementations; everything else is copied byte for byte.
pub fn enlarge_symbols(symbols_xml: &str, factor: f64) -> Result<String, OmapError> {
    use quick_xml::Reader;
    use quick_xml::events::Event;

    use super::symbol_set::position;

    let mut reader = Reader::from_str(symbols_xml);
    let mut stack: Vec<String> = Vec::new();
    let mut out = String::with_capacity(symbols_xml.len());

    loop {
        let before = position(&reader);
        let event = reader
            .read_event()
            .map_err(|e| OmapError::Xml(e.to_string()))?;
        let raw = &symbols_xml[before..position(&reader)];
        match event {
            Event::Start(e) | Event::Empty(e) => {
                let is_start = raw.ends_with('>') && !raw.ends_with("/>");
                let tag = e.name().as_ref().to_string();
                let parent = stack.last().map(String::as_str);
                let rules: &[(&[&str], Scaling)] = match (tag.as_str(), parent) {
                    ("line_symbol", _) => &[(LINE_SYMBOL_ATTRS, Scaling::Linear(factor))],
                    ("border", _) => &[(BORDER_ATTRS, Scaling::Linear(factor))],
                    ("point_symbol", _) => &[(POINT_SYMBOL_ATTRS, Scaling::Linear(factor))],
                    ("area_symbol", _) => &[(&["min_area"], Scaling::Quadratic(factor))],
                    ("pattern", Some("area_symbol")) => &[(PATTERN_ATTRS, Scaling::Linear(factor))],
                    ("font", Some("text_symbol")) => &[(&["size"], Scaling::FontSize(factor))],
                    ("framing", Some("text_symbol")) => &[(FRAMING_ATTRS, Scaling::Linear(factor))],
                    ("text", Some("text_symbol")) => &[(TEXT_ATTRS, Scaling::Linear(factor))],
                    ("line_below", Some("text_symbol")) => {
                        &[(LINE_BELOW_ATTRS, Scaling::Linear(factor))]
                    }
                    ("coord", Some("coords")) if in_element_object(&stack) => {
                        &[(&["x", "y"], Scaling::Linear(factor))]
                    }
                    _ => &[],
                };
                let mut tag_text = raw.to_string();
                for (attrs, scaling) in rules {
                    for attr in *attrs {
                        tag_text = scale_attribute(&tag_text, attr, *scaling);
                    }
                }
                out.push_str(&tag_text);
                if is_start {
                    if stack.len() >= MAX_XML_DEPTH {
                        return Err(too_deep());
                    }
                    stack.push(tag);
                }
            }
            Event::End(_) => {
                out.push_str(raw);
                stack.pop();
            }
            Event::Text(_)
                if stack.last().map(String::as_str) == Some("coords")
                    && in_element_object(&stack[..stack.len() - 1]) =>
            {
                out.push_str(&scale_coords_text(raw, factor));
            }
            Event::Eof => break,
            _ => out.push_str(raw),
        }
    }
    Ok(out)
}

/// True if the innermost elements are `element > object` (a point symbol element's geometry).
fn in_element_object(stack: &[String]) -> bool {
    let n = stack.len();
    let ends_with = |names: &[&str]| {
        n >= names.len()
            && stack[n - names.len()..]
                .iter()
                .map(String::as_str)
                .eq(names.iter().copied())
    };
    ends_with(&["element", "object"]) || ends_with(&["element", "object", "coords"])
}

/// Replaces the value of numeric attribute `name` in a raw start tag.
fn scale_attribute(tag: &str, name: &str, scaling: Scaling) -> String {
    let needle = format!(" {name}=\"");
    let Some(start) = tag.find(&needle).map(|i| i + needle.len()) else {
        return tag.to_string();
    };
    let Some(len) = tag[start..].find('"') else {
        return tag.to_string();
    };
    let Ok(value) = tag[start..start + len].parse::<f64>() else {
        return tag.to_string();
    };
    format!(
        "{}{}{}",
        &tag[..start],
        scaling.apply(value),
        &tag[start + len..]
    )
}

/// Scales compact coordinate text `x y [flags];…`, keeping flags.
fn scale_coords_text(text: &str, factor: f64) -> String {
    let mut out = String::with_capacity(text.len());
    for (i, coord) in text.split(';').enumerate() {
        if i > 0 {
            out.push(';');
        }
        let mut parts = coord.split_whitespace();
        match (parts.next(), parts.next()) {
            (Some(x), Some(y)) => match (x.parse::<f64>(), y.parse::<f64>()) {
                (Ok(x), Ok(y)) => {
                    out.push_str(&format!(
                        "{} {}",
                        (x * factor).round(),
                        (y * factor).round()
                    ));
                    for flag in parts {
                        out.push(' ');
                        out.push_str(flag);
                    }
                }
                _ => out.push_str(coord),
            },
            _ => out.push_str(coord),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SymbolSet;

    fn enlarge(xml: &str) -> String {
        enlarge_symbols(xml, 1.25).unwrap()
    }

    #[test]
    fn line_attributes_scaled() {
        let xml = r#"<symbols><symbol type="2" id="0" code="101"><line_symbol color="8" line_width="210" dash_length="4000" dashes_in_group="1" pointed_cap_length="750"/></symbol></symbols>"#;
        let out = enlarge(xml);
        assert!(out.contains(r#"line_width="263""#), "{out}");
        assert!(out.contains(r#"dash_length="5000""#));
        assert!(out.contains(r#"color="8""#));
        assert!(out.contains(r#"dashes_in_group="1""#));
        assert!(out.contains(r#"pointed_cap_length="938""#), "{out}");
    }

    #[test]
    fn task_5_lengths_scaled() {
        // Offsets, the dash symbol's dot and element coordinates, and text framing.
        let xml = r#"<symbols><symbol type="2" id="0"><line_symbol line_width="200" start_offset="800" end_offset="400"><dash_symbol><symbol type="1"><point_symbol inner_radius="400" inner_color="1" outer_width="80" outer_color="1" elements="1"><element><symbol type="2"><line_symbol line_width="100"/></symbol><object type="1"><coords count="2">0 -400;0 400;</coords></object></element></point_symbol></symbol></dash_symbol></line_symbol></symbol><symbol type="8" id="1"><text_symbol><font size="4000"/><text color="1"/><framing color="2" mode="1" line_half_width="200" shadow_x_offset="80" shadow_y_offset="-40"/></text_symbol></symbol></symbols>"#;
        let out = enlarge(xml);
        for scaled in [
            r#"start_offset="1000""#,
            r#"end_offset="500""#,
            r#"inner_radius="500""#,
            r#"outer_width="100""#,
            "0 -500;0 500;",
            r#"line_half_width="250""#,
            r#"shadow_x_offset="100""#,
            r#"shadow_y_offset="-50""#,
        ] {
            assert!(out.contains(scaled), "{scaled}: {out}");
        }
    }

    #[test]
    fn border_scaled() {
        let xml = r#"<symbols><symbol type="2" id="0"><line_symbol line_width="0"><borders><border color="15" width="100" shift="50"/></borders></line_symbol></symbol></symbols>"#;
        let out = enlarge(xml);
        assert!(
            out.contains(r#"<border color="15" width="125" shift="63"/>"#),
            "{out}"
        );
    }

    #[test]
    fn point_element_coords_scaled() {
        let xml = r#"<symbols><symbol type="1" id="0"><point_symbol inner_radius="1000" inner_color="-1" outer_width="0" outer_color="-1" elements="1"><element><symbol type="2" code=""><line_symbol color="2" line_width="220"/></symbol><object type="1"><coords count="3">0 0;0 -750;-12797 -557 1;</coords><pattern rotation="0"><coord x="10" y="10"/></pattern></object></element></point_symbol></symbol></symbols>"#;
        let out = enlarge(xml);
        assert!(out.contains(r#"inner_radius="1250""#), "{out}");
        assert!(out.contains(r#"line_width="275""#));
        assert!(out.contains("0 0;0 -938;-15996 -696 1;"), "{out}");
        assert!(out.contains(r#"<pattern rotation="0"><coord x="10" y="10"/></pattern>"#));
    }

    #[test]
    fn area_min_area_quadratic() {
        let xml = r#"<symbols><symbol type="4" id="0"><area_symbol inner_color="14" min_area="500" patterns="1"><pattern type="2" angle="0.785398" line_spacing="1000" line_offset="0" offset_along_line="0" color="16" line_width="250"/></area_symbol></symbol></symbols>"#;
        let out = enlarge(xml);
        assert!(out.contains(r#"min_area="781""#), "{out}");
        assert!(out.contains(r#"inner_color="14""#));
        assert!(out.contains(r#"angle="0.785398""#));
        assert!(out.contains(r#"line_spacing="1250""#));
        assert!(out.contains(r#"line_width="313""#));
        assert!(out.contains(r#"color="16""#));
    }

    #[test]
    fn enlarge_scales_patterns() {
        let xml = r#"<symbols><symbol type="4" id="0"><area_symbol inner_color="-1" min_area="0" patterns="2"><pattern type="1" angle="0.5" rotatable="true" line_spacing="1000" line_offset="100" offset_along_line="200" color="3" line_width="250"/><pattern type="2" angle="0.785398" line_spacing="2000" line_offset="500" offset_along_line="250" point_distance="1500"><symbol type="1" code=""><point_symbol inner_radius="100" inner_color="5" outer_width="40" outer_color="6" elements="1"><element><symbol type="2" code=""><line_symbol color="2" line_width="50"/></symbol><object type="1"><coords count="2">0 0;200 0;</coords></object></element></point_symbol></symbol></pattern></area_symbol></symbol></symbols>"#;
        let out = enlarge_symbols(xml, 1.5).unwrap();
        // Line pattern: spacing, offsets and width; angle and colour unchanged.
        assert!(
            out.contains(r#"<pattern type="1" angle="0.5" rotatable="true" line_spacing="1500" line_offset="150" offset_along_line="300" color="3" line_width="375"/>"#),
            "{out}"
        );
        // Point pattern and its nested point symbol: dot, ring, element line and coordinates.
        assert!(
            out.contains(r#"<pattern type="2" angle="0.785398" line_spacing="3000" line_offset="750" offset_along_line="375" point_distance="2250">"#),
            "{out}"
        );
        assert!(
            out.contains(r#"<point_symbol inner_radius="150" inner_color="5" outer_width="60" outer_color="6" elements="1">"#),
            "{out}"
        );
        assert!(
            out.contains(r#"<line_symbol color="2" line_width="75"/>"#),
            "{out}"
        );
        assert!(out.contains("0 0;300 0;"), "{out}");
    }

    #[test]
    fn font_size_min_10() {
        let xml = r#"<symbols><symbol type="8" id="0"><text_symbol icon_text="375"><font family="Arial" size="4"/><text color="8"/><framing color="1" mode="1" line_half_width="100" shadow_x_offset="200" shadow_y_offset="200"/></text_symbol></symbol><symbol type="8" id="1"><text_symbol><font family="Arial" size="2117"/></text_symbol></symbol></symbols>"#;
        let out = enlarge(xml);
        assert!(out.contains(r#"size="10""#), "{out}");
        assert!(out.contains(r#"size="2646""#));
        assert!(out.contains(r#"icon_text="375""#));
        assert!(
            out.contains(r#"line_half_width="125" shadow_x_offset="250" shadow_y_offset="250""#)
        );
    }

    #[test]
    fn paragraph_spacing_and_line_below_scaled() {
        let xml = r#"<symbols><symbol type="8" id="0"><text_symbol><font size="4000"/><text color="1" line_spacing="1" paragraph_spacing="800"/><line_below color="1" width="200" distance="400"/></text_symbol></symbol></symbols>"#;
        let out = enlarge(xml);
        assert!(
            out.contains(r#"<text color="1" line_spacing="1" paragraph_spacing="1000"/>"#),
            "{out}"
        );
        assert!(
            out.contains(r#"<line_below color="1" width="250" distance="500"/>"#),
            "{out}"
        );
    }

    #[test]
    fn issprom_full_set_roundtrips() {
        let set = SymbolSet::issprom_2019();
        assert_eq!(
            enlarge_symbols(&set.symbols_xml, 1.0).unwrap(),
            set.symbols_xml
        );
    }
}
