//! The suite's rules for a page's assets (decision 0030, from freemapper-courseplan ADRs 0042 and 0045), as a
//! check for the apps' tests. Public with the feature `page-rules`.

/// Checks a page's assets against the suite rules and lists every breach, one English line each naming the
/// file. `files` are `(name, bytes)` pairs; the kind of a file comes from the extension of its name.
///
/// - HTML and SVG (`.html`, `.htm`, `.svg`): a `<script>` without `src`, an `on…=` attribute (an inline
///   handler), a `<script src>` or `<link href>` from the network. Comments are skipped.
/// - CSS (`.css`, and `<style>` in HTML): `@import` and `url(` that point to the network.
/// - JavaScript (`.js`, `.mjs`): `import … from`, `export … from`, `import "…"` and `import("…")` that point
///   to the network.
/// - Every text file (the kinds above, `.json` and `.txt`): a `javascript:` URL.
/// - All files together: more than `budget` bytes.
///
/// "From the network" is a URL that starts with `http:`, `https:` or `//`. The caller decides what to pass,
/// so vendored libraries with their own limit can stay out; [`suite_assets`](crate::suite_assets) gives the
/// suite's part. The scan is plain text matching, made for the suite's own hand-written files, not a parser.
pub fn check_page_rules(files: &[(&str, &[u8])], budget: usize) -> Result<(), Vec<String>> {
    let mut problems = Vec::new();
    let mut total: usize = 0;
    for &(name, bytes) in files {
        total = total.saturating_add(bytes.len());
        let extension = name.rsplit_once('.').map_or("", |(_, e)| e);
        let extension = extension.to_ascii_lowercase();
        let kind = match extension.as_str() {
            "html" | "htm" | "svg" => Kind::Markup,
            "css" => Kind::Css,
            "js" | "mjs" => Kind::Script,
            "json" | "txt" => Kind::Text,
            _ => continue,
        };
        // ASCII lower case keeps every byte offset, so positions in `text` hold in the original too.
        let text = String::from_utf8_lossy(bytes).to_ascii_lowercase();
        if text.contains("javascript:") {
            problems.push(format!("{name}: javascript: URL"));
        }
        match kind {
            Kind::Markup => {
                markup(name, &text, &mut problems);
                css(name, &text, &mut problems);
            }
            Kind::Css => css(name, &text, &mut problems),
            Kind::Script => script(name, &text, &mut problems),
            Kind::Text => {}
        }
    }
    if total > budget {
        problems.push(format!(
            "the files have {total} bytes, over the budget of {budget} bytes"
        ));
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(problems)
    }
}

enum Kind {
    Markup,
    Css,
    Script,
    Text,
}

/// A URL that loads from another host.
fn from_network(url: &str) -> bool {
    let url = url.trim_start();
    ["http:", "https:", "//", "\\\\", "/\\", "\\/"]
        .iter()
        .any(|start| url.starts_with(start))
}

fn markup(name: &str, text: &str, problems: &mut Vec<String>) {
    let mut rest = text;
    while let Some(at) = rest.find('<') {
        rest = &rest[at + 1..];
        if let Some(comment) = rest.strip_prefix("!--") {
            rest = comment.find("-->").map_or("", |end| &comment[end + 3..]);
            continue;
        }
        if !rest.starts_with(|c: char| c.is_ascii_alphabetic()) {
            continue;
        }
        let name_end = rest
            .find(|c: char| c.is_ascii_whitespace() || c == '>' || c == '/')
            .unwrap_or(rest.len());
        let tag = &rest[..name_end];
        let (attributes, after) = attributes(&rest[name_end..]);
        rest = after;
        for (attribute, _) in &attributes {
            if attribute.len() > 2 && attribute.starts_with("on") {
                problems.push(format!("{name}: inline handler {attribute}= in <{tag}>"));
            }
        }
        let value = |wanted: &str| {
            attributes
                .iter()
                .find(|(attribute, _)| *attribute == wanted)
                .map(|(_, value)| *value)
        };
        match tag {
            "script" => match value("src") {
                None => problems.push(format!("{name}: inline script")),
                Some(src) if from_network(src) => {
                    problems.push(format!("{name}: script from the network: {src}"));
                }
                Some(_) => {}
            },
            "link" => {
                if let Some(href) = value("href").filter(|href| from_network(href)) {
                    problems.push(format!("{name}: <link> from the network: {href}"));
                }
            }
            _ => {}
        }
    }
}

/// The attributes of a tag up to its `>`, and the text after it.
fn attributes(mut rest: &str) -> (Vec<(&str, &str)>, &str) {
    let mut found = Vec::new();
    loop {
        rest = rest.trim_start_matches(|c: char| c.is_ascii_whitespace() || c == '/');
        if rest.is_empty() {
            return (found, rest);
        }
        if let Some(after) = rest.strip_prefix('>') {
            return (found, after);
        }
        let end = rest
            .find(|c: char| c.is_ascii_whitespace() || matches!(c, '=' | '>' | '/'))
            .unwrap_or(rest.len());
        let attribute = &rest[..end];
        rest = rest[end..].trim_start();
        let mut value = "";
        if let Some(after) = rest.strip_prefix('=') {
            let after = after.trim_start();
            (value, rest) = match after.chars().next() {
                Some(quote @ ('"' | '\'')) => {
                    let inner = &after[1..];
                    let close = inner.find(quote).unwrap_or(inner.len());
                    (&inner[..close], inner.get(close + 1..).unwrap_or(""))
                }
                _ => {
                    let close = after
                        .find(|c: char| c.is_ascii_whitespace() || c == '>')
                        .unwrap_or(after.len());
                    (&after[..close], &after[close..])
                }
            };
        }
        if attribute.is_empty() {
            // A stray `=` or quote: step over one character.
            rest = rest.get(1..).unwrap_or("");
            continue;
        }
        found.push((attribute, value));
    }
}

fn css(name: &str, text: &str, problems: &mut Vec<String>) {
    for (at, _) in text.match_indices("url(") {
        let target = text[at + 4..].trim_start();
        let target = target.trim_start_matches(['"', '\'']);
        if from_network(target) {
            problems.push(format!(
                "{name}: url() from the network: {}",
                until_end(target)
            ));
        }
    }
    for (at, _) in text.match_indices("@import") {
        let target = text[at + 7..].trim_start();
        if let Some(target) = target.strip_prefix(['"', '\''])
            && from_network(target)
        {
            problems.push(format!(
                "{name}: @import from the network: {}",
                until_end(target)
            ));
        }
    }
}

fn script(name: &str, text: &str, problems: &mut Vec<String>) {
    for keyword in ["import", "from"] {
        for (at, _) in text.match_indices(keyword) {
            let before = text[..at].chars().next_back();
            if before.is_some_and(|c| c.is_alphanumeric() || matches!(c, '_' | '$' | '.')) {
                continue;
            }
            let mut target = text[at + keyword.len()..].trim_start();
            if keyword == "import" {
                target = target.strip_prefix('(').unwrap_or(target).trim_start();
            }
            if let Some(target) = target.strip_prefix(['"', '\'', '`'])
                && from_network(target)
            {
                problems.push(format!(
                    "{name}: {keyword} from the network: {}",
                    until_end(target)
                ));
            }
        }
    }
}

/// The URL at the start of `text`, up to a quote, a bracket or white space, for the message.
fn until_end(text: &str) -> &str {
    let end = text
        .find(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | '`' | ')' | ';'))
        .unwrap_or(text.len());
    &text[..end]
}
