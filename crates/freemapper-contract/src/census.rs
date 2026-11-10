//! How many objects of each class a map holds, and where they lie.

use serde::{Deserialize, Serialize};

use crate::class::{ObjectClass, classify};
use crate::{ContractError, read_map};

/// A box in map coordinates (1/1000 mm, y south): `[min_x, min_y, max_x, max_y]`.
pub type Bbox = [i32; 4];

/// Number of objects per class, counted over all parts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ClassCounts {
    pub furniture: usize,
    pub base: usize,
    pub north_lines: usize,
    pub content: usize,
}

/// Boxes around the coordinates of the objects of a class, control points of curves included. A class
/// without objects has no box, and the key is left out of TOML.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ClassBoxes {
    /// Map content: the classes [`ObjectClass::Content`] and [`ObjectClass::NorthLines`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<Bbox>,
    /// The sheet furniture.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub furniture: Option<Bbox>,
}

/// Counts and boxes of a map, as `expected.toml` of a fixture records them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Census {
    pub objects: ClassCounts,
    #[serde(default)]
    pub bbox: ClassBoxes,
}

/// Takes the census of a map given as OOM XML.
pub fn census(omap_xml: &str) -> Result<Census, ContractError> {
    let doc = read_map(omap_xml)?;
    let mut census = Census::default();
    for object in doc.parts.iter().flat_map(|p| &p.objects) {
        let points = object.coords.iter().map(|c| (c.x, c.y));
        match classify(&object.tags) {
            ObjectClass::Furniture => {
                census.objects.furniture += 1;
                grow(&mut census.bbox.furniture, points);
            }
            ObjectClass::Base => census.objects.base += 1,
            ObjectClass::NorthLines => {
                census.objects.north_lines += 1;
                grow(&mut census.bbox.content, points);
            }
            ObjectClass::Content => {
                census.objects.content += 1;
                grow(&mut census.bbox.content, points);
            }
        }
    }
    Ok(census)
}

fn grow(bbox: &mut Option<Bbox>, points: impl Iterator<Item = (i32, i32)>) {
    for (x, y) in points {
        let b = bbox.get_or_insert([x, y, x, y]);
        *b = [b[0].min(x), b[1].min(y), b[2].max(x), b[3].max(y)];
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{TestObject, test_map};

    fn tagged(source: &'static str, coords: &[(i32, i32)]) -> TestObject {
        TestObject::new(&[("freemapper:source", source)], coords)
    }

    #[test]
    fn classes_come_from_the_source_tag() {
        let at = [(0, 0), (1, 1)];
        let map = test_map(
            4000,
            "",
            &[(
                "p",
                vec![
                    tagged("sheet", &at),
                    tagged("scale-bar", &at),
                    tagged("base", &at),
                    tagged("north-lines", &at),
                    tagged("osm", &at),
                    TestObject::new(&[], &at),
                ],
            )],
        );
        assert_eq!(
            census(&map).unwrap().objects,
            ClassCounts {
                furniture: 2,
                base: 1,
                north_lines: 1,
                content: 2
            }
        );
    }

    #[test]
    fn boxes_cover_content_and_furniture_apart() {
        let map = test_map(
            4000,
            "",
            &[
                (
                    "default",
                    vec![
                        tagged("osm", &[(0, 0), (10_000, 5_000)]),
                        tagged("north-lines", &[(0, -2_000), (0, 7_000)]),
                    ],
                ),
                (
                    "peripheral information",
                    vec![tagged("sheet", &[(0, 20_000), (4_000, 21_000)])],
                ),
            ],
        );
        let bbox = census(&map).unwrap().bbox;
        assert_eq!(bbox.content, Some([0, -2_000, 10_000, 7_000]));
        assert_eq!(bbox.furniture, Some([0, 20_000, 4_000, 21_000]));
    }

    #[test]
    fn the_base_area_is_in_no_box() {
        let map = test_map(
            4000,
            "",
            &[(
                "base",
                vec![tagged("base", &[(-9_000, -9_000), (9_000, 9_000)])],
            )],
        );
        let census = census(&map).unwrap();
        assert_eq!(census.objects.base, 1);
        assert_eq!(census.bbox, ClassBoxes::default());
    }

    #[test]
    fn a_map_without_the_sheet_part_has_no_furniture() {
        let map = test_map(4000, "", &[("default", vec![tagged("osm", &[(0, 0)])])]);
        let census = census(&map).unwrap();
        assert_eq!(census.objects.furniture, 0);
        assert_eq!(census.bbox.furniture, None);
    }

    #[test]
    fn a_map_without_objects_has_no_boxes() {
        let census = census(&test_map(4000, "", &[("default", vec![])])).unwrap();
        assert_eq!(census, Census::default());
        assert_eq!(toml::to_string(&ClassBoxes::default()).unwrap(), "");
        assert_eq!(
            toml::from_str::<ClassBoxes>("").unwrap(),
            ClassBoxes::default()
        );
    }

    #[test]
    fn a_census_round_trips_through_toml() {
        let census = Census {
            objects: ClassCounts {
                furniture: 9,
                base: 1,
                north_lines: 14,
                content: 212,
            },
            bbox: ClassBoxes {
                content: Some([-61_200, -48_000, 61_200, 47_500]),
                furniture: None,
            },
        };
        let text = toml::to_string(&census).unwrap();
        assert_eq!(toml::from_str::<Census>(&text).unwrap(), census);
    }

    #[test]
    fn an_unreadable_map_is_an_error() {
        assert!(matches!(census("not xml"), Err(ContractError::Map(_))));
    }
}
