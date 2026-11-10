//! The crate's boundary (core ADR 0013): of the suite it uses `freemapper-omap` and `freemapper-georef` alone, so an app reads
//! courses without building rule profiles, a renderer or another app's code.

use std::path::{Path, PathBuf};

/// Every `.rs` file under `dir`.
fn sources(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            sources(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// The suite crates `text` names as a Rust path (`freemapper_…::`), other than this crate, the omap and the georef crate.
/// A name without `::`, as in the placement module's own check, is no path.
fn other_suite_crates(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for (i, _) in text.match_indices("freemapper_") {
        let name: String = text[i..]
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        let path = text[i + name.len()..].starts_with("::");
        if path
            && name != "freemapper_omap"
            && name != "freemapper_georef"
            && name != "freemapper_course"
            && !out.contains(&name)
        {
            out.push(name);
        }
    }
    out
}

#[test]
fn boundary_test_course_needs_only_omap_and_georef() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    sources(&root.join("src"), &mut files);
    assert!(files.len() > 5, "{files:?}");
    let mut found = Vec::new();
    for file in files {
        let text = std::fs::read_to_string(&file).unwrap();
        for name in other_suite_crates(&text) {
            found.push(format!("{}: {name}", file.display()));
        }
    }
    assert!(found.is_empty(), "{found:#?}");

    // The manifest names no other suite crate either.
    let manifest = std::fs::read_to_string(root.join("Cargo.toml")).unwrap();
    let suite: Vec<&str> = manifest
        .lines()
        .map(str::trim)
        .filter(|l| {
            l.starts_with("freemapper-")
                && !l.starts_with("freemapper-omap")
                && !l.starts_with("freemapper-georef")
        })
        .collect();
    assert!(suite.is_empty(), "{suite:?}");
}

#[test]
fn the_check_sees_a_suite_crate() {
    assert_eq!(
        other_suite_crates("use freemapper_rules::RuleSet; use freemapper_omap::MapCoord;"),
        ["freemapper_rules"]
    );
    assert!(other_suite_crates("// freemapper-print ADR 0039").is_empty());
    assert!(other_suite_crates("[\"crate::\", \"freemapper_print\"]").is_empty());
    assert_eq!(
        other_suite_crates("let r = freemapper_render::draw();"),
        ["freemapper_render"]
    );
}
