//! The embedded symbol sets must ship with the crate. A git dependency builds the crate from its
//! own folder in cargo's checkout, so a file that `include_str!` takes from outside that folder
//! would be missing there.

use std::path::{Component, Path, PathBuf};

use freemapper_omap::SymbolSet;

/// Every `include_str!`/`include_bytes!` path in `src/`, resolved against the file that names it.
fn embedded_paths() -> Vec<(PathBuf, String)> {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut found = Vec::new();
    for entry in std::fs::read_dir(&src).unwrap() {
        let file = entry.unwrap().path();
        if file.extension().is_none_or(|e| e != "rs") {
            continue;
        }
        let text = std::fs::read_to_string(&file).unwrap();
        for macro_name in ["include_str!(\"", "include_bytes!(\""] {
            for (at, _) in text.match_indices(macro_name) {
                let rest = &text[at + macro_name.len()..];
                let literal = &rest[..rest.find('"').unwrap()];
                found.push((file.clone(), literal.to_string()));
            }
        }
    }
    found
}

/// `path` with `.` and `..` removed, without touching the file system.
fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other),
        }
    }
    out
}

#[test]
fn symbol_sets_are_found_from_a_git_dependency() {
    let root = normalize(Path::new(env!("CARGO_MANIFEST_DIR")));
    let embedded = embedded_paths();
    assert!(
        embedded.len() >= 3,
        "the three symbol-set files: {embedded:?}"
    );
    for (file, literal) in &embedded {
        let target = normalize(&file.parent().unwrap().join(literal));
        let inside = target
            .strip_prefix(&root)
            .unwrap_or_else(|_| panic!("{} embeds {literal}, outside the crate", file.display()));
        // `Cargo.toml` packages only `assets/symbol-sets/*` besides the sources.
        assert!(
            inside.starts_with("assets/symbol-sets") || inside.starts_with("src"),
            "{} embeds {literal}, which the package leaves out",
            file.display()
        );
        assert!(target.is_file(), "{} is missing", target.display());
    }

    // The public entry points an app calls.
    let issprom = SymbolSet::issprom_2019();
    assert_eq!(issprom.name, "ISSprOM 2019");
    assert!(issprom.id_for_code("529").is_some());
    let school = SymbolSet::school_2019().with_sheet_symbols();
    assert_eq!(school.scale_denominator, 1000);
    assert!(school.id_for_code("991").is_some());
}
