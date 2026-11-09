//! Program texts from Fluent catalogs.
//!
//! The crate holds no texts. An app embeds its own `.ftl` files and hands them to a [`Catalog`],
//! usually in a `static`:
//!
//! ```
//! use freemapper_text::{Catalog, Lang};
//!
//! static TEXTS: Catalog = Catalog::new(&[
//!     ("en", "hello = Hello, { $name }\n"),
//!     ("de", "hello = Hallo, { $name }\n"),
//! ]);
//!
//! let lang = TEXTS.detect_lang(Some("de"), &|name| std::env::var(name).ok());
//! assert_eq!(lang.code(), "de");
//! let args = [("name", "Anna".into())];
//! assert_eq!(TEXTS.text(&lang, "hello", &args), "Hallo, Anna");
//! assert_eq!(TEXTS.text(&Lang::english(), "no-such-id", &[]), "no-such-id");
//! ```
//!
//! English is the reference and the fallback: a message missing in another language is taken
//! from English, and a message missing everywhere comes back as its id. English must be the
//! first source.
//!
//! # Language detection
//!
//! [`Catalog::detect_lang`] looks at these sources in order and takes the first one that names an
//! available language:
//!
//! 1. the `--lang` argument,
//! 2. `FREEMAPPER_LANG`,
//! 3. `LC_ALL`, 4. `LC_MESSAGES`, 5. `LANG`.
//!
//! Only the language part counts, case-insensitively: `de`, `de-DE`, `de_DE.UTF-8` and `de_AT`
//! all mean German. An unset or empty variable, `C`, `POSIX` and a language the catalog does not
//! have (`fr`) pass to the next source. If nothing matches, the result is English. A `--lang`
//! value that names no available language is a usage error for the caller:
//! [`Catalog::parse_lang_arg`] reports it with the available languages, while
//! [`Catalog::detect_lang`] alone skips it.
//!
//! # Checking a catalog
//!
//! [`Catalog::problems`] is meant for a test in the app: it returns one line per defect and the
//! test asserts that the list is empty. [`ids_in_source`] finds the message ids named in the
//! app's code, so the same test can check that each of them exists.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

pub use fluent_bundle::FluentValue;
use fluent_bundle::concurrent::FluentBundle;
use fluent_bundle::{FluentArgs, FluentResource};
use unic_langid::LanguageIdentifier;

/// A language of the program texts, identified by its language part (`en`, `de`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lang(pub LanguageIdentifier);

impl Lang {
    /// English, the reference language and the fallback.
    pub fn english() -> Lang {
        Lang(LanguageIdentifier::from_parts(
            "en".parse().expect("valid language"),
            None,
            None,
            &[],
        ))
    }

    /// The language part as text, for example `de`.
    pub fn code(&self) -> String {
        self.0.language.to_string()
    }
}

fn lang_of(code: &str) -> Option<Lang> {
    Some(Lang(LanguageIdentifier::from_parts(
        code.parse().ok()?,
        None,
        None,
        &[],
    )))
}

type Bundle = FluentBundle<FluentResource>;

fn build_bundle(code: &str, source: &str) -> Bundle {
    // A file with syntax errors still yields its valid messages; `Catalog::problems` reports them.
    let resource = match FluentResource::try_new(source.to_owned()) {
        Ok(resource) => resource,
        Err((resource, _)) => resource,
    };
    let id = lang_of(code).unwrap_or_else(Lang::english).0;
    let mut bundle = FluentBundle::new_concurrent(vec![id]);
    // Isolation marks would show up as stray characters in terminals.
    bundle.set_use_isolating(false);
    // Overriding is impossible with a single resource; ignore the (empty) error list.
    let _ = bundle.add_resource(resource);
    bundle
}

fn fluent_args(args: &[(&str, FluentValue)]) -> FluentArgs<'static> {
    let mut out = FluentArgs::new();
    for (name, value) in args {
        out.set(name.to_string(), value.clone().into_owned());
    }
    out
}

fn format_in(bundle: &Bundle, id: &str, args: &FluentArgs) -> Option<String> {
    let pattern = bundle.get_message(id)?.value()?;
    let mut errors = Vec::new();
    Some(
        bundle
            .format_pattern(pattern, Some(args), &mut errors)
            .into_owned(),
    )
}

/// The language part of a locale value such as `de_DE.UTF-8`, `de-AT` or `en`; `None` for an
/// empty value, `C` and `POSIX`.
fn language_part(value: &str) -> Option<String> {
    let head = value.split(['.', '@']).next().unwrap_or("");
    let language = head.split(['_', '-']).next().unwrap_or("").trim();
    if language.is_empty()
        || language.eq_ignore_ascii_case("c")
        || language.eq_ignore_ascii_case("posix")
    {
        None
    } else {
        Some(language.to_ascii_lowercase())
    }
}

/// A set of Fluent catalogs, one per language. `Catalog` is `Sync`, so an app keeps it in a
/// `static`.
pub struct Catalog {
    sources: &'static [(&'static str, &'static str)],
    bundles: OnceLock<Vec<(String, Bundle)>>,
}

impl Catalog {
    /// A catalog from `(language code, .ftl content)` pairs, English first. The bundles are built
    /// on first use.
    pub const fn new(sources: &'static [(&'static str, &'static str)]) -> Catalog {
        Catalog {
            sources,
            bundles: OnceLock::new(),
        }
    }

    fn bundles(&self) -> &[(String, Bundle)] {
        self.bundles.get_or_init(|| {
            self.sources
                .iter()
                .map(|(code, source)| ((*code).to_owned(), build_bundle(code, source)))
                .collect()
        })
    }

    fn bundle_of(&self, code: &str) -> Option<&Bundle> {
        self.bundles()
            .iter()
            .find(|(c, _)| c == code)
            .map(|(_, b)| b)
    }

    /// The languages the catalog has texts for, in source order (English first).
    pub fn available(&self) -> Vec<Lang> {
        self.sources
            .iter()
            .filter_map(|(code, _)| lang_of(code))
            .collect()
    }

    fn match_available(&self, value: &str) -> Option<Lang> {
        let language = language_part(value)?;
        self.available().into_iter().find(|l| l.code() == language)
    }

    /// The `--lang` value as a language, or the list of available languages when it names none.
    pub fn parse_lang_arg(&self, arg: &str) -> Result<Lang, Vec<Lang>> {
        self.match_available(arg.trim())
            .ok_or_else(|| self.available())
    }

    /// Picks the language: `--lang`, else `FREEMAPPER_LANG`, else `LC_ALL`, `LC_MESSAGES`, `LANG`,
    /// else English. Values naming no available language pass to the next source. `env` reads one
    /// environment variable (`|name| std::env::var(name).ok()` in the programs).
    pub fn detect_lang(&self, arg: Option<&str>, env: &dyn Fn(&str) -> Option<String>) -> Lang {
        let from_arg = arg.and_then(|a| self.match_available(a));
        let from_env = || {
            ["FREEMAPPER_LANG", "LC_ALL", "LC_MESSAGES", "LANG"]
                .iter()
                .find_map(|name| env(name).and_then(|v| self.match_available(&v)))
        };
        from_arg.or_else(from_env).unwrap_or_else(Lang::english)
    }

    /// The text `id` in `lang`, falling back to English, and to the id itself as a last resort.
    pub fn text(&self, lang: &Lang, id: &str, args: &[(&str, FluentValue)]) -> String {
        let fluent_args = fluent_args(args);
        let code = lang.code();
        [code.as_str(), "en"]
            .iter()
            .filter_map(|c| self.bundle_of(c))
            .find_map(|bundle| format_in(bundle, id, &fluent_args))
            .unwrap_or_else(|| id.to_owned())
    }

    /// The message ids of the catalog for `code`; empty for an unknown code. Syntax errors are
    /// skipped, [`Catalog::problems`] reports them.
    pub fn message_ids(&self, code: &str) -> BTreeSet<String> {
        self.sources
            .iter()
            .find(|(c, _)| *c == code)
            .map(|(_, source)| parse_tolerant(source).0)
            .map(|resource| ids_of(&resource))
            .unwrap_or_default()
    }

    /// Defects of the catalog, one English line each; empty when the catalog is sound. Each line
    /// names the language code and, where one applies, the message id. Checked:
    ///
    /// - a syntax error in any source,
    /// - English not being the first source,
    /// - message ids that differ from English,
    /// - variables of a message that differ from English,
    /// - a message that fails to format with `sample_args`, or formats to nothing,
    /// - isolation marks in the output.
    ///
    /// `sample_args` should hold a value for every variable the catalog uses.
    pub fn problems(&self, sample_args: &[(&str, FluentValue)]) -> Vec<String> {
        let mut out = Vec::new();
        let Some((first, _)) = self.sources.first() else {
            return vec!["the catalog has no sources".to_owned()];
        };
        if *first != "en" {
            out.push(format!("{first}: English must be the first source"));
        }
        let mut seen = BTreeSet::new();
        for (code, _) in self.sources {
            if !seen.insert(*code) {
                out.push(format!("{code}: language code appears twice"));
            }
        }
        let parsed: Vec<_> = self
            .sources
            .iter()
            .map(|(code, source)| {
                let (resource, errors) = parse_tolerant(source);
                match lang_of(code) {
                    None => out.push(format!("{code}: language code is not valid")),
                    Some(lang) if lang.code() != *code => out.push(format!(
                        "{code}: language code is not in canonical form, use {}",
                        lang.code()
                    )),
                    Some(_) => {}
                }
                for error in errors {
                    out.push(format!("{code}: {error}"));
                }
                (*code, resource)
            })
            .collect();
        let english = parsed.iter().find(|(c, _)| *c == "en");
        let english_ids = english.map(|(_, r)| ids_of(r));
        let english_vars = english.map(|(_, r)| variables_of(r));
        let args = fluent_args(sample_args);
        for (code, resource) in &parsed {
            let ids = ids_of(resource);
            if let Some(english_ids) = &english_ids {
                for id in english_ids.difference(&ids) {
                    out.push(format!("{code}/{id}: message missing, English has it"));
                }
                for id in ids.difference(english_ids) {
                    out.push(format!("{code}/{id}: message not in English"));
                }
            }
            if let Some(english_vars) = &english_vars {
                for (id, vars) in variables_of(resource) {
                    if let Some(expected) = english_vars.get(&id)
                        && *expected != vars
                    {
                        out.push(format!(
                            "{code}/{id}: variables {vars:?} differ from English {expected:?}"
                        ));
                    }
                }
            }
            let Some(bundle) = self.bundle_of(code) else {
                continue;
            };
            for id in &ids {
                let Some(pattern) = bundle.get_message(id).and_then(|m| m.value()) else {
                    out.push(format!("{code}/{id}: message has no value"));
                    continue;
                };
                let mut errors = Vec::new();
                let formatted = bundle.format_pattern(pattern, Some(&args), &mut errors);
                for error in &errors {
                    out.push(format!("{code}/{id}: formatting fails: {error}"));
                }
                if formatted.trim().is_empty() {
                    out.push(format!("{code}/{id}: formats to an empty text"));
                }
                if formatted.contains(['\u{2068}', '\u{2069}']) {
                    out.push(format!("{code}/{id}: output contains isolation marks"));
                }
            }
        }
        out
    }
}

/// Parses a source; a source with syntax errors still yields its valid entries. The errors come
/// back as text, each naming its line.
fn parse_tolerant(source: &str) -> (fluent_syntax::ast::Resource<&str>, Vec<String>) {
    match fluent_syntax::parser::parse(source) {
        Ok(resource) => (resource, Vec::new()),
        Err((resource, errors)) => (
            resource,
            errors
                .iter()
                .map(|e| {
                    let line = 1 + source[..e.pos.start.min(source.len())]
                        .matches('\n')
                        .count();
                    format!("line {line}: syntax error: {:?}", e.kind)
                })
                .collect(),
        ),
    }
}

fn ids_of(resource: &fluent_syntax::ast::Resource<&str>) -> BTreeSet<String> {
    use fluent_syntax::ast::Entry;
    resource
        .body
        .iter()
        .filter_map(|entry| match entry {
            Entry::Message(m) => Some(m.id.name.to_owned()),
            _ => None,
        })
        .collect()
}

/// The variables each message refers to, including those inside selectors and variants.
fn variables_of(
    resource: &fluent_syntax::ast::Resource<&str>,
) -> BTreeMap<String, BTreeSet<String>> {
    use fluent_syntax::ast::{
        CallArguments, Entry, Expression, InlineExpression, Pattern, PatternElement,
    };
    fn in_inline(e: &InlineExpression<&str>, out: &mut BTreeSet<String>) {
        match e {
            InlineExpression::VariableReference { id } => {
                out.insert(id.name.to_owned());
            }
            InlineExpression::FunctionReference { arguments, .. } => in_args(arguments, out),
            InlineExpression::TermReference {
                arguments: Some(arguments),
                ..
            } => in_args(arguments, out),
            InlineExpression::Placeable { expression } => in_expression(expression, out),
            _ => {}
        }
    }
    fn in_args(a: &CallArguments<&str>, out: &mut BTreeSet<String>) {
        a.positional.iter().for_each(|e| in_inline(e, out));
        a.named.iter().for_each(|n| in_inline(&n.value, out));
    }
    fn in_expression(e: &Expression<&str>, out: &mut BTreeSet<String>) {
        match e {
            Expression::Inline(i) => in_inline(i, out),
            Expression::Select { selector, variants } => {
                in_inline(selector, out);
                variants.iter().for_each(|v| in_pattern(&v.value, out));
            }
        }
    }
    fn in_pattern(p: &Pattern<&str>, out: &mut BTreeSet<String>) {
        for element in &p.elements {
            if let PatternElement::Placeable { expression } = element {
                in_expression(expression, out);
            }
        }
    }
    let mut map = BTreeMap::new();
    for entry in &resource.body {
        if let Entry::Message(m) = entry {
            let mut vars = BTreeSet::new();
            if let Some(pattern) = &m.value {
                in_pattern(pattern, &mut vars);
            }
            map.insert(m.id.name.to_owned(), vars);
        }
    }
    map
}

/// Program texts for a value, for example an error.
pub trait Localize {
    /// The text in `lang`. In English it equals `to_string()` for error types.
    fn localize(&self, lang: &Lang) -> String;
}

/// Message ids written as string literals in `source` above its first `#[cfg(test)]`: literals
/// that start with one of `prefixes`, consist of `a-z` and `-` and do not end in `-`. Apps use it
/// to test that every id their code names exists in the catalog.
pub fn ids_in_source(source: &str, prefixes: &[&str]) -> BTreeSet<String> {
    let code = source.split("#[cfg(test)]").next().unwrap_or("");
    code.split('"')
        .skip(1)
        .step_by(2)
        .filter(|lit| {
            prefixes.iter().any(|p| lit.starts_with(p))
                && lit.chars().all(|c| c.is_ascii_lowercase() || c == '-')
                && !lit.ends_with('-')
        })
        .map(str::to_owned)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const EN: &str = include_str!("../tests/fixtures/en.ftl");
    const DE: &str = include_str!("../tests/fixtures/de.ftl");

    static FIXTURE: Catalog = Catalog::new(&[("en", EN), ("de", DE)]);

    const _: fn() = || {
        fn assert_sync<T: Sync>() {}
        assert_sync::<Catalog>();
    };

    fn de() -> Lang {
        lang_of("de").unwrap()
    }

    fn sample() -> Vec<(&'static str, FluentValue<'static>)> {
        vec![("name", "Anna".into()), ("count", 2.into())]
    }

    fn env_of<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |name| {
            assert!(
                ["FREEMAPPER_LANG", "LC_ALL", "LC_MESSAGES", "LANG"].contains(&name),
                "unexpected variable {name}"
            );
            pairs
                .iter()
                .find(|(n, _)| *n == name)
                .map(|(_, v)| (*v).to_owned())
        }
    }

    #[test]
    fn detects_language_in_order() {
        let c = &FIXTURE;
        let env = env_of(&[("FREEMAPPER_LANG", "en"), ("LANG", "de_DE.UTF-8")]);
        assert_eq!(c.detect_lang(Some("de"), &env), de());
        assert_eq!(c.detect_lang(Some("DE"), &env), de());
        assert_eq!(c.detect_lang(None, &env), Lang::english());
        let env = env_of(&[("LANG", "de_DE.UTF-8")]);
        assert_eq!(c.detect_lang(None, &env), de());
        assert_eq!(c.detect_lang(Some("en"), &env), Lang::english());
        let env = env_of(&[("LANG", "C")]);
        assert_eq!(c.detect_lang(None, &env), Lang::english());
        let env = env_of(&[("FREEMAPPER_LANG", "fr"), ("LANG", "de_DE.UTF-8")]);
        assert_eq!(c.detect_lang(None, &env), de());
        // LC_ALL before LC_MESSAGES before LANG; empty values and POSIX pass on.
        let env = env_of(&[("LC_ALL", ""), ("LC_MESSAGES", "POSIX"), ("LANG", "de_AT")]);
        assert_eq!(c.detect_lang(None, &env), de());
        let env = env_of(&[("LC_ALL", "en_US.UTF-8"), ("LANG", "de_DE")]);
        assert_eq!(c.detect_lang(None, &env), Lang::english());
        let env = env_of(&[("LC_MESSAGES", "de-DE")]);
        assert_eq!(c.detect_lang(None, &env), de());
        assert_eq!(c.detect_lang(None, &env_of(&[])), Lang::english());
        // An unknown --lang passes to the next source.
        let env = env_of(&[("LANG", "de_DE")]);
        assert_eq!(c.detect_lang(Some("fr"), &env), de());
    }

    #[test]
    fn unknown_lang_argument_is_refused() {
        let c = &FIXTURE;
        assert_eq!(c.parse_lang_arg("de"), Ok(de()));
        assert_eq!(c.parse_lang_arg("EN"), Ok(Lang::english()));
        let available = c.parse_lang_arg("fr").unwrap_err();
        assert_eq!(available, vec![Lang::english(), de()]);
        assert!(c.parse_lang_arg("").is_err());
        assert!(c.parse_lang_arg("C").is_err());
    }

    #[test]
    fn missing_message_falls_back_to_english() {
        static PARTIAL: Catalog = Catalog::new(&[("en", EN), ("de", "hello = Hallo\n")]);
        let args = [("name", "Anna".into())];
        assert_eq!(PARTIAL.text(&de(), "hello", &[]), "Hallo");
        assert_eq!(PARTIAL.text(&de(), "greet", &args), "Hello, Anna");
        assert_eq!(PARTIAL.text(&de(), "no-such-id", &[]), "no-such-id");
        assert_eq!(
            PARTIAL.text(&Lang::english(), "no-such-id", &[]),
            "no-such-id"
        );
    }

    #[test]
    fn text_formats_variables_selectors_and_terms() {
        let c = &FIXTURE;
        assert_eq!(
            c.text(&de(), "greet", &[("name", "Anna".into())]),
            "Hallo, Anna"
        );
        assert_eq!(c.text(&de(), "files", &[("count", 1.into())]), "eine Datei");
        assert_eq!(c.text(&de(), "files", &[("count", 3.into())]), "3 Dateien");
        assert_eq!(c.text(&Lang::english(), "about", &[]), "About Fixture");
        // No isolation marks around variables.
        assert!(
            !c.text(&Lang::english(), "greet", &sample())
                .contains('\u{2068}')
        );
    }

    #[test]
    fn two_catalogs_do_not_share_bundles() {
        static ONE: Catalog = Catalog::new(&[("en", "x = one\n")]);
        static TWO: Catalog = Catalog::new(&[("en", "x = two\n")]);
        assert_eq!(ONE.text(&Lang::english(), "x", &[]), "one");
        assert_eq!(TWO.text(&Lang::english(), "x", &[]), "two");
        assert_eq!(ONE.text(&Lang::english(), "x", &[]), "one");
    }

    #[test]
    fn clean_catalog_has_no_problems() {
        assert_eq!(FIXTURE.problems(&sample()), Vec::<String>::new());
        let ids = FIXTURE.message_ids("de");
        assert_eq!(ids, FIXTURE.message_ids("en"));
        assert!(ids.contains("greet") && !ids.contains("-brand"));
        assert!(FIXTURE.message_ids("fr").is_empty());
    }

    #[test]
    fn problems_report_syntax_errors() {
        static BROKEN: Catalog = Catalog::new(&[("en", "ok = fine\nbroken = {}\n")]);
        let problems = BROKEN.problems(&[]);
        assert!(
            problems
                .iter()
                .any(|p| p.starts_with("en: line 2: syntax error")),
            "{problems:?}"
        );
    }

    #[test]
    fn problems_report_missing_ids_and_variables() {
        static C: Catalog = Catalog::new(&[
            ("en", "a = A\nb = B { $x }\nc = C\n"),
            ("de", "a = Ä\nb = B { $y }\nextra = E\n"),
        ]);
        let args = [("x", "1".into()), ("y", "2".into())];
        let problems = C.problems(&args);
        let has = |needle: &str| problems.iter().any(|p| p.contains(needle));
        assert!(has("de/c: message missing"), "{problems:?}");
        assert!(has("de/extra: message not in English"), "{problems:?}");
        assert!(has("de/b: variables"), "{problems:?}");
        assert_eq!(problems.len(), 3, "{problems:?}");
    }

    #[test]
    fn problems_report_format_failures_and_empty_texts() {
        static C: Catalog = Catalog::new(&[("en", "a = A { $x }\nb = { \"\" }\n")]);
        let problems = C.problems(&[]);
        assert!(
            problems
                .iter()
                .any(|p| p.starts_with("en/a: formatting fails")),
            "{problems:?}"
        );
        assert!(
            problems
                .iter()
                .any(|p| p == "en/b: formats to an empty text"),
            "{problems:?}"
        );
    }

    #[test]
    fn problems_report_messages_without_value() {
        static C: Catalog = Catalog::new(&[("en", "a = A\nb =\n    .title = T\n")]);
        let problems = C.problems(&[]);
        assert!(
            problems.iter().any(|p| p == "en/b: message has no value"),
            "{problems:?}"
        );
    }

    #[test]
    fn problems_report_codes_not_in_canonical_form() {
        static C: Catalog = Catalog::new(&[("en", "a = A\n"), ("DE", "a = A\n")]);
        let problems = C.problems(&[]);
        assert!(
            problems.iter().any(|p| p.starts_with("DE: language code")),
            "{problems:?}"
        );
    }

    #[test]
    fn problems_report_duplicate_codes() {
        static C: Catalog =
            Catalog::new(&[("en", "a = A\n"), ("de", "a = A\n"), ("de", "a = B\n")]);
        let problems = C.problems(&[]);
        assert!(
            problems
                .iter()
                .any(|p| p == "de: language code appears twice"),
            "{problems:?}"
        );
    }

    #[test]
    fn syntax_errors_name_the_line() {
        static C: Catalog = Catalog::new(&[("en", "ok = fine\n\nbroken = {}\nnext = n\n")]);
        let problems = C.problems(&[]);
        assert!(
            problems
                .iter()
                .any(|p| p.starts_with("en: line 3: syntax error")),
            "{problems:?}"
        );
    }

    #[test]
    fn problems_report_isolation_marks() {
        static C: Catalog = Catalog::new(&[("en", "a = x \u{2068}y\n")]);
        let problems = C.problems(&[]);
        assert!(
            problems
                .iter()
                .any(|p| p == "en/a: output contains isolation marks"),
            "{problems:?}"
        );
    }

    #[test]
    fn problems_require_english_first() {
        static C: Catalog = Catalog::new(&[("de", "a = A\n"), ("en", "a = A\n")]);
        let problems = C.problems(&[]);
        assert!(
            problems
                .iter()
                .any(|p| p.contains("English must be the first source")),
            "{problems:?}"
        );
        assert_eq!(Catalog::new(&[]).problems(&[]).len(), 1);
        // Without a source coded "en" there is no fallback: the id comes back.
        assert_eq!(C.text(&de(), "a", &[]), "A");
        static NO_EN: Catalog = Catalog::new(&[("de", "a = A\n")]);
        assert_eq!(NO_EN.text(&Lang::english(), "a", &[]), "a");
    }

    #[test]
    fn ids_in_source_skips_tests_and_partial_ids() {
        let source = r#"
            let a = text("error-one");
            let b = format!("word-{x}");
            let c = "word-";
            let d = "Error-upper";
            let e = "cli-two";
            let f = "other-id";
            #[cfg(test)]
            mod tests { fn t() { let _ = "error-in-test"; } }
        "#;
        let ids = ids_in_source(source, &["error-", "cli-", "word-"]);
        let expected: BTreeSet<String> = ["error-one", "cli-two"]
            .iter()
            .map(|s| (*s).to_owned())
            .collect();
        assert_eq!(ids, expected);
    }
}
