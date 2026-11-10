//! The fixtures agree with what the crate derives from them.

use freemapper_contract::{CONTRACT_VERSION, Vocabulary, census, check, fixture, fixtures};

#[test]
fn there_are_two_fixtures_named_as_the_spec_says() {
    let names: Vec<&str> = fixtures().iter().map(|f| f.name).collect();
    assert_eq!(names, ["schoolyard-1000", "sprint-4000"]);
    assert_eq!(fixture("sprint-4000").unwrap().name, "sprint-4000");
    assert!(fixture("forest-10000").is_none());
}

#[test]
fn every_fixture_breaks_no_rule() {
    for fixture in fixtures() {
        let violations = check(fixture.omap_xml, fixture.sidecar_toml).unwrap();
        assert!(violations.is_empty(), "{}: {violations:#?}", fixture.name);
    }
}

#[test]
fn every_vocabulary_file_matches_its_map() {
    for fixture in fixtures() {
        let of_the_map = Vocabulary::of(fixture.omap_xml, fixture.sidecar_toml).unwrap();
        let differences = of_the_map.differences(&fixture.vocabulary());
        assert!(differences.is_empty(), "{}: {differences:#?}", fixture.name);
    }
}

#[test]
fn every_expected_file_matches_its_map() {
    for fixture in fixtures() {
        let expected = fixture.expected();
        assert_eq!(expected.contract, CONTRACT_VERSION, "{}", fixture.name);
        assert!(
            expected.producer.starts_with("freemapper "),
            "{}",
            fixture.name
        );
        let census = census(fixture.omap_xml).unwrap();
        assert_eq!(census.objects, expected.objects, "{}", fixture.name);
        assert_eq!(census.bbox, expected.bbox, "{}", fixture.name);
        let sidecar: toml::Table = fixture.sidecar_toml.parse().unwrap();
        assert_eq!(sidecar["map_kind"].as_str(), Some(&*expected.map_kind));
        assert_eq!(sidecar["title"].as_str(), Some(&*expected.title));
        assert_eq!(
            sidecar["scale_denominator"].as_integer(),
            Some(i64::from(expected.scale_denominator))
        );
        assert_eq!(sidecar["crs"].as_integer(), Some(i64::from(expected.crs)));
    }
}

#[test]
fn every_class_a_kind_writes_is_in_its_fixture() {
    for fixture in fixtures() {
        let objects = fixture.expected().objects;
        assert!(objects.furniture > 0, "{}", fixture.name);
        assert!(objects.north_lines > 0, "{}", fixture.name);
        assert!(objects.content > 0, "{}", fixture.name);
        // Only the sprint map has a base area.
        assert_eq!(
            objects.base > 0,
            fixture.expected().map_kind == "sprint",
            "{}",
            fixture.name
        );
    }
}

#[test]
fn the_boxes_keep_the_furniture_outside_the_content() {
    for fixture in fixtures() {
        let bbox = fixture.expected().bbox;
        let (content, furniture) = (bbox.content.unwrap(), bbox.furniture.unwrap());
        // y points south: the scale bar lies below the map.
        assert!(furniture[3] > content[3], "{}", fixture.name);
    }
}

#[test]
fn write_into_puts_map_and_sidecar_side_by_side() {
    for fixture in fixtures() {
        let dir = tempfile::tempdir().unwrap();
        let map = fixture.write_into(dir.path()).unwrap();
        assert_eq!(map, dir.path().join(format!("{}.omap", fixture.name)));
        assert_eq!(std::fs::read_to_string(&map).unwrap(), fixture.omap_xml);
        assert_eq!(
            std::fs::read_to_string(dir.path().join("freemapper.toml")).unwrap(),
            fixture.sidecar_toml
        );
    }
}

#[test]
fn every_map_stays_under_400_kb() {
    for fixture in fixtures() {
        assert!(
            fixture.omap_xml.len() < 400_000,
            "{}: {} bytes",
            fixture.name,
            fixture.omap_xml.len()
        );
    }
}

#[test]
fn no_fixture_has_a_carriage_return() {
    // `.gitattributes` keeps the files LF on every system; a CR would make the embedded text differ
    // between a Windows checkout and the others.
    for fixture in fixtures() {
        for text in [
            fixture.omap_xml,
            fixture.sidecar_toml,
            fixture.expected_toml,
            fixture.vocabulary_toml,
        ] {
            assert!(!text.contains('\r'), "{}", fixture.name);
        }
    }
}
