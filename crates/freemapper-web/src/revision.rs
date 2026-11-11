//! Revisions of files and the answers to a stale or a refused write (decision 0030, from freemapper-courseplan
//! ADRs 0043 and 0044).

use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};

/// The request header in which a page names the revision it last saw (`X-Freemapper-Revision`), lower case
/// as [`axum::http::HeaderName`] needs it. `api.js` sends it with every write that names a revision.
pub const REVISION_HEADER: &str = "x-freemapper-revision";

/// The revision of a file: FNV-1a 64 over its bytes, as 16 lower-case hex digits.
///
/// It tells versions apart, it is no protection against a forged file. The value is the one
/// freemapper-courseplan's `fnv1a64` gives for the same bytes.
pub fn revision(bytes: &[u8]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{hash:016x}")
}

/// The answer to a write whose revision no longer matches: 409 with the JSON
/// `{ "error": sentence, "revision": current }`.
///
/// `current` is the file's revision now, so the page can load it again and name it next time; `sentence` is
/// one sentence for the person, already in the page's language, which the page shows in its banner. This is
/// the body freemapper-courseplan's editor sends today.
pub fn stale(current: &str, sentence: &str) -> Response {
    let body = format!(
        "{{\"error\":{},\"revision\":{}}}",
        json_string(sentence),
        json_string(current)
    );
    (
        StatusCode::CONFLICT,
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        )],
        body,
    )
        .into_response()
}

/// The answer to a change the app refuses: 422 with one sentence for the person as plain text, already in
/// the page's language. `api.js` hands it to the page as a string.
pub fn refused(sentence: &str) -> Response {
    (
        StatusCode::UNPROCESSABLE_ENTITY,
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("text/plain; charset=utf-8"),
        )],
        sentence.to_owned(),
    )
        .into_response()
}

/// `text` as a JSON string literal with its quotes.
fn json_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if u32::from(c) < 0x20 => out.push_str(&format!("\\u{:04x}", u32::from(c))),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
