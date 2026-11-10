//! Small maps for the tests of this crate.

/// A path object with tags.
pub(crate) struct TestObject {
    pub tags: Vec<(&'static str, &'static str)>,
    pub coords: Vec<(i32, i32)>,
}

impl TestObject {
    pub(crate) fn new(tags: &[(&'static str, &'static str)], coords: &[(i32, i32)]) -> Self {
        Self {
            tags: tags.to_vec(),
            coords: coords.to_vec(),
        }
    }
}

/// OOM XML (version 9) of a map with the given scale, CRS text (none when empty) and parts. Symbol 0
/// exists and is hidden, as in the reader tests of `freemapper-omap`.
pub(crate) fn test_map(scale: u32, crs_spec: &str, parts: &[(&str, Vec<TestObject>)]) -> String {
    let crs = if crs_spec.is_empty() {
        String::new()
    } else {
        format!(
            r#"<projected_crs id="EPSG"><spec language="PROJ.4">{crs_spec}</spec></projected_crs>"#
        )
    };
    let parts_xml: String = parts
        .iter()
        .map(|(name, objects)| {
            let objects_xml: String = objects.iter().map(object_xml).collect();
            format!(
                r#"<part name="{name}"><objects count="{}">{objects_xml}</objects></part>"#,
                objects.len()
            )
        })
        .collect();
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<map xmlns="http://openorienteering.org/apps/mapper/xml/v2" version="9">
<notes></notes>
<georeferencing scale="{scale}">{crs}</georeferencing>
<colors count="0"></colors>
<symbols count="1" id="Test"><symbol type="1" id="0" code="1" name="x" is_hidden="true"><point_symbol/></symbol></symbols>
<parts count="{}" current="0">{parts_xml}</parts>
<templates count="0"/>
</map>"#,
        parts.len()
    )
}

fn object_xml(object: &TestObject) -> String {
    let tags: String = object
        .tags
        .iter()
        .map(|(k, v)| format!(r#"<t k="{k}">{v}</t>"#))
        .collect();
    let coords: String = object
        .coords
        .iter()
        .map(|(x, y)| format!("{x} {y};"))
        .collect();
    format!(
        r#"<object type="1" symbol="0"><tags>{tags}</tags><coords count="{}">{coords}</coords></object>"#,
        object.coords.len()
    )
}

/// The parts of a small map as freemapper writes it: content with a north line, the sheet furniture
/// with a scale bar, and the base area.
pub(crate) fn good_parts() -> Vec<(&'static str, Vec<TestObject>)> {
    vec![
        (
            "default",
            vec![
                TestObject::new(
                    &[("freemapper:source", "osm"), ("freemapper:osm", "x")],
                    &[(0, 0), (40_000, 30_000)],
                ),
                TestObject::new(
                    &[("freemapper:source", "north-lines")],
                    &[(10_000, -1_000), (10_000, 31_000)],
                ),
            ],
        ),
        (
            "peripheral information",
            vec![
                TestObject::new(&[("freemapper:source", "sheet")], &[(0, -10_000)]),
                TestObject::new(
                    &[
                        ("freemapper:source", "sheet"),
                        ("freemapper:sheet", "scale-bar"),
                    ],
                    &[(0, 60_000), (20_000, 60_000)],
                ),
            ],
        ),
        (
            "base",
            vec![TestObject::new(
                &[("freemapper:source", "base")],
                &[(0, 0), (40_000, 0), (40_000, 30_000), (0, 30_000)],
            )],
        ),
    ]
}

/// [`good_parts`] as a schoolyard map at 1:1000 in EPSG:25833.
pub(crate) fn good_map() -> String {
    test_map(1000, "+init=epsg:25833", &good_parts())
}

/// The sidecar that belongs to [`good_map`], with one key outside the contract.
pub(crate) fn good_sidecar() -> String {
    "map_kind = \"schoolyard\"\nscale_denominator = 1000\ncrs = 25833\ntitle = \"Clara-Zetkin-Schule\"\nbuffer_m = 20.0\n"
        .to_string()
}
