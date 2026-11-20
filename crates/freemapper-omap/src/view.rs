// SPDX-License-Identifier: AGPL-3.0-or-later

//! Views of a finished map: which symbols a variant of the map leaves out (freemapper ADR 0160).
//!
//! A [`View`] is a rule on the code and the colours of a symbol. It writes no file and knows no object model:
//! freemapper and freemapper-print ask it, one object at a time, whether the object stays. The frame of a map
//! (the part "peripheral information" and the objects tagged `freemapper:source=sheet`) is never left out;
//! [`View::keeps_frame`] recognises it.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use quick_xml::Reader;
use quick_xml::events::Event;

use crate::SymbolSet;
use crate::symbol_set::attribute;

/// The map part that holds the information block, title and credit lines.
pub const FRAME_PART: &str = "peripheral information";
/// The tag value of `freemapper:source` on the objects of the frame.
pub const FRAME_SOURCE: (&str, &str) = ("freemapper:source", "sheet");
/// The magnetic north lines.
pub const NORTH_LINE_CODE: &str = "601";

/// Roads, paths and stairs: 502 to 507, the paved paths 501.6 to 501.8 and the stairs 532.
const PATH_CODES: [&str; 5] = ["501.6", "501.7", "501.8", "532", "502"];

/// The attributes of a symbol definition that name a colour of the table (a negative number is no colour).
const COLOUR_ATTRIBUTES: [&str; 3] = ["color", "inner_color", "outer_color"];

/// A preset of a [`View`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Preset {
    /// Roads, paths and stairs go (Ultimate maps).
    NoPaths,
    /// Only symbols drawn in brown stay (contour orienteering), and the frame.
    BrownOnly,
    /// The north lines go.
    NoNorthLines,
}

impl Preset {
    const ALL: [Preset; 3] = [Preset::NoPaths, Preset::BrownOnly, Preset::NoNorthLines];

    /// The name as it is typed.
    pub fn name(self) -> &'static str {
        match self {
            Preset::NoPaths => "no-paths",
            Preset::BrownOnly => "brown-only",
            Preset::NoNorthLines => "no-north-lines",
        }
    }

    fn keeps(self, code: &str, colours: &[&str]) -> bool {
        match self {
            Preset::NoPaths => !is_path_code(code),
            Preset::BrownOnly => {
                code != NORTH_LINE_CODE
                    && !colours.is_empty()
                    && colours.iter().all(|c| is_brown(c))
            }
            Preset::NoNorthLines => code != NORTH_LINE_CODE,
        }
    }
}

/// Whether a symbol code is a road, path or stair: 502 to 507, 501.6 to 501.8, 532 and their sub-numbers.
fn is_path_code(code: &str) -> bool {
    let whole = code.split('.').next().unwrap_or(code);
    if matches!(whole, "502" | "503" | "504" | "505" | "506" | "507" | "532") {
        return true;
    }
    PATH_CODES[..3].contains(&code)
}

/// A colour of the brown family: "Brown 100%" and its screens. The lower browns of the paved areas and the mixed
/// colours such as "White over light brown" are no brown symbols.
fn is_brown(name: &str) -> bool {
    name.trim().to_ascii_lowercase().starts_with("brown")
}

/// What a [`View`] was asked to parse and could not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ViewError {
    /// A name that is no preset and no list.
    UnknownName(String),
    /// A list (`no:` or `only:`) without a code.
    EmptyList(String),
    /// A symbol code that is no number like `505` or `501.6`.
    BadCode(String),
}

/// A view: presets and lists of codes, all of which must keep an object.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct View {
    presets: BTreeSet<Preset>,
    no: BTreeSet<String>,
    only: Option<BTreeSet<String>>,
}

impl View {
    /// The view of one preset.
    pub fn preset(preset: Preset) -> Self {
        Self {
            presets: BTreeSet::from([preset]),
            ..Self::default()
        }
    }

    /// Parses `no-paths`, `brown-only`, `no-north-lines`, `no:<code>,<code>`, `only:<code>,<code>` and
    /// their combination with `+`, for example `no-paths+no-north-lines`. Several `no:` lists add up, several
    /// `only:` lists keep the codes they share.
    pub fn parse(text: &str) -> Result<Self, ViewError> {
        let mut view = Self::default();
        for part in text.split('+').map(str::trim) {
            if let Some(preset) = Preset::ALL.iter().find(|p| p.name() == part) {
                view.presets.insert(*preset);
            } else if let Some(list) = part.strip_prefix("no:") {
                view.no.extend(codes(part, list)?);
            } else if let Some(list) = part.strip_prefix("only:") {
                let codes: BTreeSet<String> = codes(part, list)?.into_iter().collect();
                view.only = Some(match view.only.take() {
                    Some(have) => have.intersection(&codes).cloned().collect(),
                    None => codes,
                });
            } else {
                return Err(ViewError::UnknownName(part.to_string()));
            }
        }
        Ok(view)
    }

    /// Whether the view leaves nothing out.
    pub fn is_empty(&self) -> bool {
        self.presets.is_empty() && self.no.is_empty() && self.only.is_none()
    }

    /// Whether an object of the symbol `code`, drawn in `colours` (their names, from [`SymbolColours`]), stays.
    /// It answers for a code without an object on the map as well: freemapper-print asks it for
    /// [`NORTH_LINE_CODE`] to know whether to generate north lines.
    pub fn keeps(&self, code: &str, colours: &[&str]) -> bool {
        self.presets.iter().all(|p| p.keeps(code, colours))
            && !self.no.contains(code)
            && self.only.as_ref().is_none_or(|only| only.contains(code))
    }

    /// Whether an object belongs to the frame of the map, which no view leaves out: it lies in the part
    /// [`FRAME_PART`] or carries the tag `freemapper:source=sheet`.
    pub fn keeps_frame(part_name: &str, tags: &[(String, String)]) -> bool {
        part_name == FRAME_PART
            || tags
                .iter()
                .any(|(k, v)| (k.as_str(), v.as_str()) == FRAME_SOURCE)
    }
}

/// The canonical form: presets in a fixed order joined by `+`, then `no:` and `only:` with sorted codes. Two
/// programs that name a file after the view name it alike.
impl fmt::Display for View {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut parts: Vec<String> = self.presets.iter().map(|p| p.name().to_string()).collect();
        if !self.no.is_empty() {
            parts.push(format!("no:{}", join(&self.no)));
        }
        if let Some(only) = &self.only {
            parts.push(format!("only:{}", join(only)));
        }
        f.write_str(&parts.join("+"))
    }
}

fn join(codes: &BTreeSet<String>) -> String {
    codes.iter().cloned().collect::<Vec<_>>().join(",")
}

fn codes(part: &str, list: &str) -> Result<Vec<String>, ViewError> {
    let items: Vec<&str> = list
        .split(',')
        .map(str::trim)
        .filter(|c| !c.is_empty())
        .collect();
    if items.is_empty() {
        return Err(ViewError::EmptyList(part.to_string()));
    }
    for code in &items {
        let ok = code
            .split('.')
            .all(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
            && code.split('.').count() <= 2;
        if !ok {
            return Err(ViewError::BadCode((*code).to_string()));
        }
    }
    Ok(items.into_iter().map(str::to_string).collect())
}

/// The names of the colours a symbol is drawn in, read from the symbol set.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SymbolColours {
    by_code: BTreeMap<String, Vec<String>>,
}

impl SymbolColours {
    /// The colours of every symbol of `set`: the colours its definition names, as the names of the colour table.
    pub fn of(set: &SymbolSet) -> Self {
        let names = colour_names(&set.colors_xml);
        let mut by_code = BTreeMap::new();
        let mut reader = Reader::from_str(&set.symbols_xml);
        let (mut code, mut in_symbol): (Option<String>, usize) = (None, 0);
        let mut found: BTreeSet<String> = BTreeSet::new();
        while let Ok(event) = reader.read_event() {
            let (e, opens) = match event {
                Event::Start(e) => (e, true),
                Event::Empty(e) => (e, false),
                Event::End(_) if in_symbol > 0 => {
                    in_symbol -= 1;
                    if in_symbol == 0
                        && let Some(code) = code.take()
                    {
                        by_code.insert(code, found.iter().cloned().collect());
                    }
                    continue;
                }
                Event::Eof => break,
                _ => continue,
            };
            if in_symbol == 0 && e.name().as_ref() == "symbol" {
                code = attribute(&e, "code").ok().flatten();
                found.clear();
                in_symbol = usize::from(opens);
            } else if in_symbol > 0 {
                in_symbol += usize::from(opens);
                for name in COLOUR_ATTRIBUTES {
                    let colour = attribute(&e, name).ok().flatten();
                    if let Some(name) = colour
                        .and_then(|c| c.parse::<i32>().ok())
                        .and_then(|n| names.get(&n))
                    {
                        found.insert(name.clone());
                    }
                }
            }
        }
        Self { by_code }
    }

    /// The colour names of the symbol `code`; empty when the set has no such symbol or it has no colour.
    pub fn of_code(&self, code: &str) -> Vec<&str> {
        self.by_code
            .get(code)
            .map(|v| v.iter().map(String::as_str).collect())
            .unwrap_or_default()
    }

    /// [`View::keeps`] for the symbol `code`, with the colours of this table.
    pub fn keeps(&self, view: &View, code: &str) -> bool {
        view.keeps(code, &self.of_code(code))
    }
}

/// The colour table: priority → name.
fn colour_names(colors_xml: &str) -> BTreeMap<i32, String> {
    let mut names = BTreeMap::new();
    let mut reader = Reader::from_str(colors_xml);
    while let Ok(event) = reader.read_event() {
        match event {
            Event::Start(e) | Event::Empty(e) if e.name().as_ref() == "color" => {
                if let (Some(p), Some(n)) = (
                    attribute(&e, "priority")
                        .ok()
                        .flatten()
                        .and_then(|p| p.parse::<i32>().ok()),
                    attribute(&e, "name").ok().flatten(),
                ) {
                    names.insert(p, n);
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(text: &str) -> View {
        View::parse(text).unwrap()
    }

    #[test]
    fn presets_parse_and_print_in_one_order() {
        assert_eq!(
            view("no-north-lines+no-paths").to_string(),
            "no-paths+no-north-lines"
        );
        assert_eq!(view("brown-only").to_string(), "brown-only");
        assert_eq!(
            view("no:506,505+only:101,102").to_string(),
            "no:505,506+only:101,102"
        );
        assert!(View::default().is_empty());
    }

    #[test]
    fn parse_errors_carry_the_cause_as_data() {
        assert_eq!(
            View::parse("paths"),
            Err(ViewError::UnknownName("paths".into()))
        );
        assert_eq!(View::parse("no:"), Err(ViewError::EmptyList("no:".into())));
        assert_eq!(View::parse("no:50x"), Err(ViewError::BadCode("50x".into())));
        assert_eq!(
            View::parse("no:1.2.3"),
            Err(ViewError::BadCode("1.2.3".into()))
        );
    }

    #[test]
    fn no_paths_leaves_out_roads_paths_and_stairs() {
        let v = view("no-paths");
        for code in [
            "502", "505", "505.1", "506", "507", "501.6", "501.7", "501.8", "532", "532.6",
        ] {
            assert!(!v.keeps(code, &[]), "{code}");
        }
        for code in ["521", "101", "501.5", "601", "401"] {
            assert!(v.keeps(code, &[]), "{code}");
        }
    }

    #[test]
    fn brown_only_keeps_brown_symbols_and_drops_the_north_lines() {
        let v = view("brown-only");
        assert!(v.keeps("101", &["Brown 100%"]));
        assert!(v.keeps("102", &["Brown 100%", "Brown 50%"]));
        assert!(!v.keeps("521", &["Black 100%"]));
        assert!(!v.keeps("104", &["Brown 100%", "Black 100%"]));
        assert!(!v.keeps("601", &["Brown 100%"]));
        assert!(!v.keeps("999", &[]));
    }

    #[test]
    fn a_code_without_an_object_is_answered_too() {
        assert!(!view("no-north-lines").keeps("601", &[]));
        assert!(view("no-paths").keeps("601", &[]));
    }

    #[test]
    fn lists_and_presets_combine() {
        let v = view("no-paths+no:521+only:521,101,505");
        assert!(v.keeps("101", &[]));
        assert!(!v.keeps("521", &[]));
        assert!(!v.keeps("505", &[]));
        assert!(!v.keeps("401", &[]));
        let two = view("only:101,102+only:102,103");
        assert!(two.keeps("102", &[]) && !two.keeps("101", &[]));
    }

    #[test]
    fn the_frame_is_found_by_part_or_tag_not_by_code() {
        assert!(View::keeps_frame(FRAME_PART, &[]));
        assert!(View::keeps_frame(
            "default part",
            &[("freemapper:source".into(), "sheet".into())]
        ));
        assert!(!View::keeps_frame(
            "default part",
            &[("freemapper:source".into(), "osm".into())]
        ));
    }

    #[test]
    fn the_colours_of_the_embedded_set_tell_the_brown_symbols() {
        let set = SymbolSet::issprom_2019();
        let colours = SymbolColours::of(&set);
        assert_eq!(
            colours.of_code("101"),
            vec!["Brown 100%"],
            "{:?}",
            colours.of_code("101")
        );
        let brown = view("brown-only");
        assert!(colours.keeps(&brown, "101"));
        assert!(!colours.keeps(&brown, "521"));
        assert!(!colours.keeps(&brown, "601"));
        // Paved areas are drawn in the lower browns: not brown symbols.
        assert!(!colours.keeps(&brown, "501.5"));
        let kept: Vec<&str> = set
            .codes()
            .map(|(c, _)| c)
            .filter(|c| colours.keeps(&brown, c))
            .collect();
        assert_eq!(
            kept,
            [
                "101", "101.1", "102", "102.1", "102.2", "103", "103.1", "104", "104.1", "104.2",
                "105", "107", "108", "109", "110", "111", "112", "113", "113.1", "113.2", "113.6",
                "115"
            ]
        );
    }
}
