//! Reader for finished OOM maps (`.omap` and `.xmap`, XML format).
//!
//! Colours and symbols come from [`SymbolSet::from_omap_xml`]; this module adds the objects of
//! every part, the georeferencing and the notes. Templates are not read. Objects of an unknown
//! `type` are skipped; objects without a numeric `symbol` get `symbol_id = u32::MAX`.

use std::path::Path;

use quick_xml::events::{BytesStart, Event};
use quick_xml::{Reader, XmlVersion};

use super::symbol_set::attribute;
use super::{
    MAX_XML_DEPTH, MapCoord, MapDocument, MapGeoref, MapObject, MapObjectKind, MapPart, OmapError,
    PatternOrigin, SymbolSet, too_deep,
};

const BINARY_FORMAT_MESSAGE: &str = "This file is in the old binary format of OpenOrienteering Mapper, which this program cannot read. Open it in Mapper and save it again as a current .omap or .xmap file.";

/// Parses a map file's XML text.
pub fn read_map(xml: &str) -> Result<MapDocument, OmapError> {
    if xml.starts_with("OMAP") {
        return Err(OmapError::Unsupported(BINARY_FORMAT_MESSAGE.into()));
    }
    let symbol_set = SymbolSet::from_omap_xml(xml)?;
    let state = read_state(xml)?;
    let mut georef = state.georef;
    georef.scale_denominator = symbol_set.scale_denominator;
    Ok(MapDocument {
        notes: state.notes,
        georef,
        symbol_set,
        parts: state.parts,
        templates: Vec::new(),
        unknown_objects: state.unknown_objects,
    })
}

/// Objects, georeferencing and notes of a map's XML text.
fn read_state(xml: &str) -> Result<State, OmapError> {
    let mut state = State::default();
    let mut reader = Reader::from_str(xml);
    loop {
        match reader.read_event().map_err(xml_err)? {
            Event::Start(e) => state.begin(&e)?,
            Event::Empty(e) => {
                state.begin(&e)?;
                state.end(&name_of(&e))?;
            }
            Event::End(e) => state.end(e.name().as_ref())?,
            Event::Text(t) => state.text(&t.xml_content(XmlVersion::Explicit1_0)),
            Event::CData(t) => state.text(&t.xml_content(XmlVersion::Explicit1_0)),
            Event::GeneralRef(r) => {
                if let Some(c) = r.resolve_char_ref().map_err(xml_err)? {
                    state.text(c.encode_utf8(&mut [0; 4]));
                } else {
                    match r.xml_content(XmlVersion::Explicit1_0).as_ref() {
                        "lt" => state.text("<"),
                        "gt" => state.text(">"),
                        "amp" => state.text("&"),
                        "apos" => state.text("'"),
                        "quot" => state.text("\""),
                        _ => {}
                    }
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(state)
}

/// Reads a `.omap` or `.xmap` file.
pub fn read_map_file(path: &Path) -> Result<MapDocument, OmapError> {
    let bytes = std::fs::read(path)?;
    if bytes.starts_with(b"OMAP") {
        return Err(OmapError::Unsupported(BINARY_FORMAT_MESSAGE.into()));
    }
    let xml = String::from_utf8(bytes).map_err(|e| OmapError::Xml(e.to_string()))?;
    read_map(&xml)
}

fn xml_err(e: impl std::fmt::Display) -> OmapError {
    OmapError::Xml(e.to_string())
}

fn name_of(e: &BytesStart<'_>) -> String {
    e.name().as_ref().to_string()
}

/// Which element's character data is being collected.
enum Collect {
    Notes,
    CrsSpec,
    Coords,
    Text,
    Tag(String),
}

struct ObjectBuilder {
    object: MapObject,
}

struct State {
    stack: Vec<String>,
    /// How many `<parts>` elements are open (objects are read only inside one).
    parts_depth: usize,
    /// Objects of a type the reader does not know, skipped with their children.
    unknown_objects: usize,
    notes: String,
    georef: MapGeoref,
    parts: Vec<MapPart>,
    object: Option<ObjectBuilder>,
    /// Inside an object of an unknown type: its children are ignored.
    skipping: bool,
    collect: Option<Collect>,
    buffer: String,
}

impl Default for State {
    fn default() -> Self {
        Self {
            stack: Vec::new(),
            parts_depth: 0,
            unknown_objects: 0,
            notes: String::new(),
            georef: MapGeoref {
                scale_denominator: 0,
                declination_deg: 0.0,
                grivation_deg: 0.0,
                ref_projected: None,
                crs_spec: None,
                combined_scale_factor: 1.0,
                map_ref_mm: (0.0, 0.0),
            },
            parts: Vec::new(),
            object: None,
            skipping: false,
            collect: None,
            buffer: String::new(),
        }
    }
}

/// A numeric attribute: 0 when missing, an error when present but not a number (the same policy
/// as for the text form of coordinates).
fn number_attr<T: std::str::FromStr + Default>(
    e: &BytesStart<'_>,
    name: &str,
) -> Result<T, OmapError> {
    match attribute(e, name)? {
        None => Ok(T::default()),
        Some(v) => v.trim().parse().map_err(|_| {
            OmapError::Xml(format!("invalid number {name}=\"{v}\" in <{}>", name_of(e)))
        }),
    }
}

/// A numeric attribute that must be finite: `NaN` and `inf` parse as numbers but are broken.
fn float_attr(e: &BytesStart<'_>, name: &str) -> Result<f64, OmapError> {
    let v: f64 = number_attr(e, name)?;
    if v.is_finite() {
        Ok(v)
    } else {
        Err(invalid_number(e, name, v))
    }
}

fn invalid_number(e: &BytesStart<'_>, name: &str, v: f64) -> OmapError {
    OmapError::Xml(format!("invalid number {name}=\"{v}\" in <{}>", name_of(e)))
}

/// A box size attribute: finite and not negative.
fn size_attr(e: &BytesStart<'_>, name: &str) -> Result<f64, OmapError> {
    let v = float_attr(e, name)?;
    if v < 0.0 {
        return Err(invalid_number(e, name, v));
    }
    Ok(v)
}

fn int_attr(e: &BytesStart<'_>, name: &str) -> Result<i32, OmapError> {
    number_attr(e, name)
}

fn coord_from_attrs(e: &BytesStart<'_>) -> Result<MapCoord, OmapError> {
    Ok(MapCoord {
        x: int_attr(e, "x")?,
        y: int_attr(e, "y")?,
        flags: flags_from(int_attr(e, "flags")?),
    })
}

fn flags_from(flags: i32) -> u8 {
    (flags & 0xFF) as u8
}

/// Parses OOM's text form of coordinates: `x y [flags];` repeated.
fn parse_coords(text: &str) -> Result<Vec<MapCoord>, OmapError> {
    let mut coords = Vec::new();
    for item in text.split(';') {
        let mut parts = item.split_whitespace();
        let Some(x) = parts.next() else { continue };
        let bad = || OmapError::Xml(format!("invalid coordinate \"{}\"", item.trim()));
        let y = parts.next().ok_or_else(bad)?;
        let flags = parts.next();
        if parts.next().is_some() {
            return Err(bad());
        }
        coords.push(MapCoord {
            x: x.parse().map_err(|_| bad())?,
            y: y.parse().map_err(|_| bad())?,
            flags: match flags {
                Some(f) => flags_from(f.parse().map_err(|_| bad())?),
                None => 0,
            },
        });
    }
    Ok(coords)
}

impl State {
    fn parent(&self) -> Option<&str> {
        self.stack.last().map(String::as_str)
    }

    fn begin(&mut self, e: &BytesStart<'_>) -> Result<(), OmapError> {
        if self.stack.len() >= MAX_XML_DEPTH {
            return Err(too_deep());
        }
        let tag = name_of(e);
        let parent = self.parent().map(str::to_owned);
        let parent = parent.as_deref();
        let in_parts = self.parts_depth > 0;
        if tag == "parts" {
            self.parts_depth += 1;
        }
        if self.skipping {
            self.stack.push(tag);
            return Ok(());
        }
        match (tag.as_str(), parent) {
            ("notes", Some("map")) => self.start_collect(Collect::Notes),
            ("georeferencing", Some("map")) => {
                self.georef.declination_deg = float_attr(e, "declination")?;
                self.georef.grivation_deg = float_attr(e, "grivation")?;
                if attribute(e, "grid_scale_factor")?.is_some() {
                    let f = float_attr(e, "grid_scale_factor")?;
                    if f <= 0.0 {
                        return Err(invalid_number(e, "grid_scale_factor", f));
                    }
                    self.georef.combined_scale_factor = f;
                }
            }
            ("ref_point", Some("georeferencing")) => {
                self.georef.map_ref_mm = (float_attr(e, "x")?, float_attr(e, "y")?);
            }
            ("spec", Some("projected_crs")) if self.georef.crs_spec.is_none() => {
                self.start_collect(Collect::CrsSpec);
            }
            ("ref_point", Some("projected_crs")) => {
                self.georef.ref_projected = Some((float_attr(e, "x")?, float_attr(e, "y")?));
            }
            ("part", Some(_)) if in_parts && self.object.is_none() => {
                self.parts.push(MapPart {
                    name: attribute(e, "name")?.unwrap_or_default(),
                    objects: Vec::new(),
                });
            }
            ("object", Some("objects")) if in_parts && !self.parts.is_empty() => {
                self.begin_object(e)?;
            }
            _ if self.object.is_some() => self.begin_object_child(&tag, parent, e)?,
            _ => {}
        }
        self.stack.push(tag);
        Ok(())
    }

    fn begin_object(&mut self, e: &BytesStart<'_>) -> Result<(), OmapError> {
        let ty = int_attr(e, "type")?;
        let kind = match ty {
            0 => MapObjectKind::Point {
                rotation_rad: float_attr(e, "rotation")?,
            },
            1 => MapObjectKind::Path,
            4 => MapObjectKind::Text {
                text: String::new(),
                h_align: u8::try_from(int_attr(e, "h_align")?).unwrap_or(0),
                v_align: u8::try_from(int_attr(e, "v_align")?).unwrap_or(0),
                rotation_rad: float_attr(e, "rotation")?,
                box_size: None,
            },
            _ => {
                self.skipping = true;
                self.unknown_objects += 1;
                return Ok(());
            }
        };
        let symbol_id = attribute(e, "symbol")?
            .and_then(|v| v.trim().parse::<u32>().ok())
            .unwrap_or(u32::MAX);
        self.object = Some(ObjectBuilder {
            object: MapObject {
                symbol_id,
                kind,
                coords: Vec::new(),
                tags: Vec::new(),
                pattern: None,
            },
        });
        Ok(())
    }

    fn begin_object_child(
        &mut self,
        tag: &str,
        parent: Option<&str>,
        e: &BytesStart<'_>,
    ) -> Result<(), OmapError> {
        match (tag, parent) {
            ("coords", Some("object")) => self.start_collect(Collect::Coords),
            ("coord", Some("coords")) => {
                let c = coord_from_attrs(e)?;
                self.current().coords.push(c);
            }
            ("text", Some("object")) => self.start_collect(Collect::Text),
            ("size", Some("object")) => {
                let size = (size_attr(e, "width")?, size_attr(e, "height")?);
                if let MapObjectKind::Text { box_size, .. } = &mut self.current().kind {
                    *box_size = Some(size);
                }
            }
            ("t", Some("tags")) => {
                let key = attribute(e, "k")?.unwrap_or_default();
                self.start_collect(Collect::Tag(key));
            }
            ("pattern", Some("object")) => {
                let rotation_rad = float_attr(e, "rotation")?;
                self.current().pattern = Some(PatternOrigin {
                    rotation_rad,
                    origin: MapCoord::default(),
                });
            }
            ("coord", Some("pattern")) => {
                let origin = coord_from_attrs(e)?;
                if let Some(p) = self.current().pattern.as_mut() {
                    p.origin = origin;
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// A text object with more than one coordinate is a box text from before `<size>`: the second
    /// coordinate holds the box size [OBJ `Object::load`]. Only the anchor stays a coordinate.
    fn legacy_box_size(&mut self) {
        let object = self.current();
        if let MapObjectKind::Text { box_size, .. } = &mut object.kind
            && let Some(size) = object.coords.get(1)
        {
            *box_size = Some((f64::from(size.x), f64::from(size.y)));
            object.coords.truncate(1);
        }
    }

    /// The object being read; only called while `self.object` is set.
    fn current(&mut self) -> &mut MapObject {
        &mut self
            .object
            .as_mut()
            .expect("an object is being read")
            .object
    }

    fn start_collect(&mut self, what: Collect) {
        self.collect = Some(what);
        self.buffer.clear();
    }

    fn text(&mut self, s: &str) {
        if self.collect.is_some() {
            self.buffer.push_str(s);
        }
    }

    fn end(&mut self, tag: &str) -> Result<(), OmapError> {
        self.stack.pop();
        if tag == "parts" {
            self.parts_depth = self.parts_depth.saturating_sub(1);
        }
        let parent = self.parent().map(str::to_owned);
        if self.skipping {
            if tag == "object" && parent.as_deref() == Some("objects") {
                self.skipping = false;
            }
            return Ok(());
        }
        if let Some(what) = self.collect.take() {
            let text = std::mem::take(&mut self.buffer);
            match what {
                Collect::Notes => self.notes = text,
                Collect::CrsSpec => self.georef.crs_spec = Some(text.trim().to_owned()),
                Collect::Coords => {
                    let coords = parse_coords(&text)?;
                    self.current().coords.extend(coords);
                }
                Collect::Text => {
                    if let MapObjectKind::Text { text: t, .. } = &mut self.current().kind {
                        *t = text;
                    }
                }
                Collect::Tag(key) => self.current().tags.push((key, text)),
            }
        }
        // After both forms of coordinates: the text form, and `<coord>` elements.
        if tag == "coords" && parent.as_deref() == Some("object") && self.object.is_some() {
            self.legacy_box_size();
        }
        if tag == "object"
            && parent.as_deref() == Some("objects")
            && let (Some(b), Some(part)) = (self.object.take(), self.parts.last_mut())
        {
            part.objects.push(b.object);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MapCoord, MapObjectKind, PatternOrigin};

    /// A minimal map around `objects` (symbol 0 exists and is hidden).
    fn map_with(objects: &str, georef: &str) -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<map xmlns="http://openorienteering.org/apps/mapper/xml/v2" version="9">
<notes>a &amp; b</notes>
{georef}
<colors count="0"></colors>
<symbols count="1" id="Test"><symbol type="1" id="0" code="1" name="x" is_hidden="true"><point_symbol/></symbol></symbols>
<parts count="1" current="0"><part name="p"><objects count="1">{objects}</objects></part></parts>
<templates count="0"/>
</map>"#
        )
    }

    fn one(objects: &str) -> crate::MapObject {
        let doc = read_map(&map_with(objects, r#"<georeferencing scale="4000"/>"#)).unwrap();
        assert_eq!(doc.parts.len(), 1);
        doc.parts[0].objects[0].clone()
    }

    fn c(x: i32, y: i32, flags: u8) -> MapCoord {
        MapCoord { x, y, flags }
    }

    #[test]
    fn reads_coords_text_form() {
        let o = one(
            r#"<object type="1" symbol="0"><coords count="3">0 0;1000 0 1;2000 500;</coords></object>"#,
        );
        assert_eq!(o.symbol_id, 0);
        assert_eq!(o.kind, MapObjectKind::Path);
        assert_eq!(o.coords, vec![c(0, 0, 0), c(1000, 0, 1), c(2000, 500, 0)]);
    }

    #[test]
    fn reads_legacy_coord_elements() {
        let o = one(
            r#"<object type="1" symbol="0"><coords><coord x="0" y="0"/><coord x="1000" y="0" flags="18"/></coords></object>"#,
        );
        assert_eq!(o.coords, vec![c(0, 0, 0), c(1000, 0, 18)]);
    }

    #[test]
    fn reads_point_rotation_and_text() {
        let p = one(
            r#"<object type="0" symbol="0" rotation="1.5"><coords count="1">5 6;</coords></object>"#,
        );
        assert_eq!(p.kind, MapObjectKind::Point { rotation_rad: 1.5 });
        let t = one(
            r#"<object type="4" symbol="0" h_align="1" v_align="2" rotation="0.25"><coords count="1">5 6;</coords><size width="1" height="2"/><text>Hütte &amp; Co</text></object>"#,
        );
        assert_eq!(
            t.kind,
            MapObjectKind::Text {
                text: "Hütte & Co".into(),
                h_align: 1,
                v_align: 2,
                rotation_rad: 0.25,
                box_size: Some((1.0, 2.0)),
            }
        );
    }

    #[test]
    fn reads_box_text_size() {
        let text = |o: &crate::MapObject| match &o.kind {
            MapObjectKind::Text { box_size, .. } => *box_size,
            other => panic!("{other:?}"),
        };
        let single = one(
            r#"<object type="4" symbol="0"><coords count="1">5 6;</coords><text>a</text></object>"#,
        );
        assert_eq!(text(&single), None);
        let sized = one(
            r#"<object type="4" symbol="0"><coords count="1">5 6;</coords><size width="40000" height="20000"/><text>a</text></object>"#,
        );
        assert_eq!(text(&sized), Some((40_000.0, 20_000.0)));
        // Before <size>, OOM kept the box size as a second coordinate [OBJ `Object::load`].
        let legacy = one(
            r#"<object type="4" symbol="0"><coords count="2">5 6;40000 20000;</coords><text>a</text></object>"#,
        );
        assert_eq!(text(&legacy), Some((40_000.0, 20_000.0)));
        assert_eq!(legacy.coords, vec![c(5, 6, 0)], "only the anchor stays");
        // The same in the older element form of coordinates.
        let elements = one(
            r#"<object type="4" symbol="0"><coords><coord x="5" y="6"/><coord x="40000" y="20000"/></coords><text>a</text></object>"#,
        );
        assert_eq!(text(&elements), Some((40_000.0, 20_000.0)));
        assert_eq!(elements.coords, vec![c(5, 6, 0)], "only the anchor stays");
    }

    #[test]
    fn rejects_broken_box_sizes_and_rotations() {
        for object in [
            r#"<object type="4" symbol="0"><coords count="1">0 0;</coords><size width="NaN" height="1"/></object>"#,
            r#"<object type="4" symbol="0"><coords count="1">0 0;</coords><size width="1" height="inf"/></object>"#,
            r#"<object type="4" symbol="0"><coords count="1">0 0;</coords><size width="-1" height="1"/></object>"#,
            r#"<object type="4" symbol="0" rotation="NaN"><coords count="1">0 0;</coords></object>"#,
            r#"<object type="0" symbol="0" rotation="-inf"><coords count="1">0 0;</coords></object>"#,
            r#"<object type="1" symbol="0"><coords count="1">0 0;</coords><pattern rotation="inf"><coord x="0" y="0"/></pattern></object>"#,
        ] {
            let err = read_map(&map_with(object, r#"<georeferencing scale="4000"/>"#)).unwrap_err();
            assert!(
                matches!(&err, OmapError::Xml(m) if m.contains("invalid number")),
                "{object}: {err}"
            );
        }
    }

    #[test]
    fn reads_tags_and_pattern() {
        let o = one(
            r#"<object type="1" symbol="0"><tags><t k="freemapper:source">sheet</t></tags><coords count="1">0 0;</coords><pattern rotation="0.5"><coord x="10" y="20"/></pattern></object>"#,
        );
        assert_eq!(
            o.tags,
            vec![("freemapper:source".to_string(), "sheet".to_string())]
        );
        assert_eq!(
            o.pattern,
            Some(PatternOrigin {
                rotation_rad: 0.5,
                origin: c(10, 20, 0)
            })
        );
    }

    #[test]
    fn reads_georeferencing_and_hidden() {
        let g = r#"<georeferencing scale="4000" declination="3.2" grivation="1.1"><projected_crs id="EPSG"><spec language="PROJ.4">+init=epsg:25832</spec><parameter>25832</parameter><ref_point x="610932.5" y="5969932.25"/></projected_crs></georeferencing>"#;
        let doc = read_map(&map_with("", g)).unwrap();
        assert_eq!(doc.notes, "a & b");
        let geo = doc.georef;
        assert_eq!(geo.scale_denominator, 4000);
        assert_eq!(geo.declination_deg, 3.2);
        assert_eq!(geo.grivation_deg, 1.1);
        assert_eq!(geo.ref_projected, Some((610932.5, 5969932.25)));
        assert_eq!(geo.crs_spec.as_deref(), Some("+init=epsg:25832"));
        assert_eq!(geo.combined_scale_factor, 1.0);
        assert_eq!(geo.map_ref_mm, (0.0, 0.0));
        assert!(doc.symbol_set.is_hidden(0));
        assert!(!doc.symbol_set.is_hidden(1));
    }

    #[test]
    fn reads_scale_factor_and_map_reference_point() {
        let g = r#"<georeferencing scale="4000" grid_scale_factor="0.999751" auxiliary_scale_factor="1"><ref_point x="12.5" y="-3.25"/><projected_crs id="EPSG"><ref_point x="1" y="2"/></projected_crs></georeferencing>"#;
        let geo = read_map(&map_with("", g)).unwrap().georef;
        assert_eq!(geo.combined_scale_factor, 0.999751);
        assert_eq!(geo.map_ref_mm, (12.5, -3.25));
        assert_eq!(geo.ref_projected, Some((1.0, 2.0)));
        for bad in ["0", "-1", "nan"] {
            let g = format!(r#"<georeferencing scale="4000" grid_scale_factor="{bad}"/>"#);
            assert!(read_map(&map_with("", &g)).is_err(), "{bad}");
        }
    }

    #[test]
    fn tolerates_odd_objects() {
        let doc = read_map(&map_with(
            r#"<object type="1"><coords count="1">0 0;</coords></object><object type="1" symbol="x"/><object type="9" symbol="0"/>"#,
            r#"<georeferencing scale="4000"/>"#,
        ))
        .unwrap();
        let objects = &doc.parts[0].objects;
        assert_eq!(objects.len(), 2);
        assert!(objects.iter().all(|o| o.symbol_id == u32::MAX));
    }

    #[test]
    fn rejects_malformed_xml_and_bad_coords() {
        assert!(matches!(
            read_map("<map><a></b></map>"),
            Err(OmapError::Xml(_))
        ));
        let bad = map_with(
            r#"<object type="1" symbol="0"><coords count="1">a b;</coords></object>"#,
            r#"<georeferencing scale="4000"/>"#,
        );
        assert!(matches!(read_map(&bad), Err(OmapError::Xml(_))));
    }

    #[test]
    fn rejects_unparseable_numbers_like_bad_coords() {
        for object in [
            r#"<object type="1" symbol="0"><coords><coord x="abc" y="0"/></coords></object>"#,
            r#"<object type="0" symbol="0" rotation="left"><coords count="1">0 0;</coords></object>"#,
            r#"<object type="4" symbol="0"><coords count="1">0 0;</coords><size width="wide" height="1"/></object>"#,
        ] {
            let err = read_map(&map_with(object, r#"<georeferencing scale="4000"/>"#)).unwrap_err();
            assert!(
                matches!(&err, OmapError::Xml(m) if m.contains("invalid number")),
                "{err}"
            );
        }
    }

    #[test]
    fn flags_keep_the_low_byte() {
        // OOM stores flags in 8 bits; higher bits are dropped in both coordinate forms.
        let o = one(
            r#"<object type="1" symbol="0"><coords count="2">0 0 274;1 1 18;</coords></object>"#,
        );
        assert_eq!(o.coords, vec![c(0, 0, 18), c(1, 1, 18)]);
        let o = one(
            r#"<object type="1" symbol="0"><coords><coord x="0" y="0" flags="290"/></coords></object>"#,
        );
        assert_eq!(o.coords, vec![c(0, 0, 34)]);
    }

    #[test]
    fn unknown_object_type_is_skipped_with_its_children_and_counted() {
        let doc = read_map(&map_with(
            r#"<object type="9" symbol="0"><coords count="1">7 7;</coords><text>x</text><object type="1" symbol="0"/></object><object type="1" symbol="0"><coords count="1">1 2;</coords></object>"#,
            r#"<georeferencing scale="4000"/>"#,
        ))
        .unwrap();
        assert_eq!(doc.unknown_objects, 1);
        assert_eq!(doc.parts[0].objects.len(), 1);
        assert_eq!(doc.parts[0].objects[0].coords, vec![c(1, 2, 0)]);
    }

    #[test]
    fn reads_parts_inside_barrier_and_several_parts() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<map xmlns="http://openorienteering.org/apps/mapper/xml/v2" version="9">
<georeferencing scale="4000"/>
<colors count="0"></colors>
<barrier version="6" required="0.6.0">
<symbols count="1" id="Test"><symbol type="1" id="0" code="1" name="x"><point_symbol/></symbol></symbols>
<parts count="2" current="0"><part name="a"><objects count="1"><object type="0" symbol="0"><coords count="1">1 1;</coords></object></objects></part><part name="b"><objects count="2"><object type="0" symbol="0"><coords count="1">2 2;</coords></object><object type="0" symbol="0"><coords count="1">3 3;</coords></object></objects></part></parts>
</barrier>
</map>"#;
        let doc = read_map(xml).unwrap();
        let counts: Vec<(&str, usize)> = doc
            .parts
            .iter()
            .map(|p| (p.name.as_str(), p.objects.len()))
            .collect();
        assert_eq!(counts, [("a", 1), ("b", 2)]);
    }

    #[test]
    fn reads_a_file_with_byte_order_mark() {
        let xml = format!(
            "\u{feff}{}",
            map_with(
                r#"<object type="1" symbol="0"><coords count="1">0 0;</coords></object>"#,
                r#"<georeferencing scale="4000"/>"#
            )
        );
        let doc = read_map(&xml).unwrap();
        assert_eq!(doc.parts[0].objects.len(), 1);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bom.omap");
        std::fs::write(&path, xml).unwrap();
        assert_eq!(read_map_file(&path).unwrap().parts[0].objects.len(), 1);
    }

    /// `depth` nested `<a>` elements.
    fn nested(depth: usize) -> String {
        format!("{}{}", "<a>".repeat(depth), "</a>".repeat(depth))
    }

    #[test]
    fn deep_nesting_is_an_error_not_a_stack_overflow() {
        let too_deep = |r: Result<(), OmapError>| matches!(r, Err(OmapError::Xml(m)) if m.contains("nesting too deep"));
        let deep = nested(100_000);
        // Inside the parts, where the object reader sees it.
        let in_parts = format!("<map><parts><part name=\"p\">{deep}</part></parts></map>");
        assert!(too_deep(read_state(&in_parts).map(|_| ())));
        // The whole file: the symbol set reader sees it first.
        let map = map_with(&deep, r#"<georeferencing scale="4000"/>"#);
        assert!(too_deep(read_map(&map).map(|_| ())));
        assert!(too_deep(SymbolSet::from_omap_xml(&map).map(|_| ())));
        let symbols = format!("<symbols>{deep}</symbols>");
        assert!(too_deep(
            crate::enlarge::enlarge_symbols(&symbols, 1.5).map(|_| ())
        ));
        // 256 levels are fine.
        let ok = format!(
            "<map><parts><part name=\"p\">{}</part></parts></map>",
            nested(250)
        );
        assert!(read_state(&ok).is_ok());
    }

    #[test]
    fn rejects_binary_omap() {
        let err = read_map("OMAP\u{0}\u{0}\u{0}\u{1}rest").unwrap_err();
        assert!(matches!(err, OmapError::Unsupported(_)));
        let msg = err.to_string();
        assert!(msg.contains("binary"), "{msg}");
        assert!(msg.contains("save"), "{msg}");
    }
}
