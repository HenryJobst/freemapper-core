//! The class of an object.

use crate::tags::{
    LEGACY_SOURCE_SCALE_BAR, SOURCE_BASE, SOURCE_KEY, SOURCE_NORTH_LINES, SOURCE_SHEET,
};

/// What an object is to a reader of the map.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectClass {
    /// Sheet objects (title block, scale bar, credit line): not map content, not part of the map extent.
    Sheet,
    /// The base area: printed, but no feature.
    Base,
    /// A magnetic north line: map content.
    NorthLines,
    /// Everything else, with or without tags.
    Content,
}

impl ObjectClass {
    /// The class of an object, from its source tag alone; the part it lies in plays no role. `tags` are
    /// the object's tags as key and value, in any order.
    pub fn of(tags: &[(String, String)]) -> ObjectClass {
        match tag(tags, SOURCE_KEY) {
            Some(SOURCE_SHEET | LEGACY_SOURCE_SCALE_BAR) => ObjectClass::Sheet,
            Some(SOURCE_BASE) => ObjectClass::Base,
            Some(SOURCE_NORTH_LINES) => ObjectClass::NorthLines,
            _ => ObjectClass::Content,
        }
    }

    /// Whether objects of this class are map content. Sheet objects and the base area are not: they do
    /// not count for the map extent and are no features.
    pub fn is_map_content(self) -> bool {
        matches!(self, ObjectClass::NorthLines | ObjectClass::Content)
    }
}

/// The value of the tag `key`, if the object carries it.
pub fn tag<'a>(tags: &'a [(String, String)], key: &str) -> Option<&'a str> {
    tags.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn the_source_tag_decides_the_class() {
        let class = |source: &str| ObjectClass::of(&tags(&[("freemapper:source", source)]));
        assert_eq!(class("sheet"), ObjectClass::Sheet);
        assert_eq!(class("scale-bar"), ObjectClass::Sheet);
        assert_eq!(class("base"), ObjectClass::Base);
        assert_eq!(class("north-lines"), ObjectClass::NorthLines);
        assert_eq!(class("osm"), ObjectClass::Content);
        assert_eq!(ObjectClass::of(&[]), ObjectClass::Content);
        // The role alone makes no sheet object.
        assert_eq!(
            ObjectClass::of(&tags(&[("freemapper:sheet", "scale-bar")])),
            ObjectClass::Content
        );
    }

    #[test]
    fn sheet_objects_and_the_base_area_are_no_map_content() {
        assert!(!ObjectClass::Sheet.is_map_content());
        assert!(!ObjectClass::Base.is_map_content());
        assert!(ObjectClass::NorthLines.is_map_content());
        assert!(ObjectClass::Content.is_map_content());
    }
}
