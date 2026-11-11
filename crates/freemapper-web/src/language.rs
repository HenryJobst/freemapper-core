//! The language of a page's texts (decision 0030, the rule of freemapper ADR 0092).

/// The language in which `api/texts` answers: `"de"` or `"en"`.
///
/// - `fixed` is the language the person set: the app's `--lang`, then its language variable. A code whose
///   language part is `de` or `en` (any case, `de-AT` too) wins. Any other code, or `None`, counts as unset.
/// - Otherwise the browser's `Accept-Language` header decides: the entry with the highest q-value whose
///   language part is `de` or `en`; on equal q-values the one named first. An entry with `q=0` is not wanted,
///   one with a broken q-value is skipped, and `*` names no language.
/// - Otherwise English.
pub fn texts_language(fixed: Option<&str>, accept_language: Option<&str>) -> &'static str {
    if let Some(code) = fixed.and_then(known) {
        return code;
    }
    let mut best: Option<(f32, &'static str)> = None;
    for entry in accept_language.unwrap_or_default().split(',') {
        let mut parts = entry.split(';');
        let Some(code) = parts.next().and_then(known) else {
            continue;
        };
        let Some(q) = q_value(parts) else { continue };
        if q > 0.0 && best.is_none_or(|(top, _)| q > top) {
            best = Some((q, code));
        }
    }
    best.map_or("en", |(_, code)| code)
}

/// `"de"` or `"en"` if the language part of `tag` (before `-` or `_`) is one of them, in any case.
fn known(tag: &str) -> Option<&'static str> {
    let language = tag.trim().split(['-', '_']).next().unwrap_or_default();
    if language.eq_ignore_ascii_case("de") {
        Some("de")
    } else if language.eq_ignore_ascii_case("en") {
        Some("en")
    } else {
        None
    }
}

/// The q-value among the parameters of one entry: 1 without one, `None` if it is not a number from 0 to 1.
fn q_value<'a>(parameters: impl Iterator<Item = &'a str>) -> Option<f32> {
    let mut q = 1.0;
    for parameter in parameters {
        let Some((name, value)) = parameter.split_once('=') else {
            continue;
        };
        if name.trim().eq_ignore_ascii_case("q") {
            let value = value.trim();
            if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit() || b == b'.') {
                return None;
            }
            q = value
                .parse::<f32>()
                .ok()
                .filter(|q| (0.0..=1.0).contains(q))?;
        }
    }
    Some(q)
}
