//! The streaming XML loop of the course readers. [`drive`] runs a [`Handler`] over the quick-xml events of a
//! file and refuses a file larger than `MAX_COURSE_FILE_BYTES`, a DOCTYPE and any entity other than the five
//! predefined ones (ADR 0019). [`local`], [`attr`] and [`number`] read element names, attributes and finite
//! numbers. These five are public so that another reader of XML in an app gets the same guards and errors.

use std::collections::HashSet;

use quick_xml::events::{BytesStart, Event};
use quick_xml::{Reader, XmlVersion};

use super::CourseError;
use super::model::{CourseNote, MAX_COURSE_FILE_BYTES, MAX_NOTES, MAX_TOTAL_TEXT_BYTES};

/// Receives the elements and the text of a document.
pub trait Handler {
    fn begin(&mut self, e: &BytesStart<'_>) -> Result<(), CourseError>;
    fn end(&mut self) -> Result<(), CourseError>;
    fn text(&mut self, s: &str);
}

/// Runs `h` over `bytes`. An empty element is a `begin` followed by an `end`.
pub fn drive(bytes: &[u8], h: &mut impl Handler) -> Result<(), CourseError> {
    check_size(bytes.len())?;
    let mut reader = Reader::from_reader(bytes);
    loop {
        let event = reader.read_event().map_err(xml_err)?;
        let reference = guard(&event)?;
        match event {
            Event::Start(e) => h.begin(&e)?,
            Event::Empty(e) => {
                h.begin(&e)?;
                h.end()?;
            }
            Event::End(_) => h.end()?,
            Event::Text(t) => h.text(&t.xml_content(XmlVersion::Explicit1_0)),
            Event::CData(t) => h.text(&t.xml_content(XmlVersion::Explicit1_0)),
            Event::GeneralRef(_) => {
                if let Some(c) = reference {
                    h.text(c.encode_utf8(&mut [0; 4]));
                }
            }
            Event::Eof => return Ok(()),
            _ => {}
        }
    }
}

/// Refuses a file of `len` bytes if it is larger than `MAX_COURSE_FILE_BYTES`.
pub(crate) fn check_size(len: usize) -> Result<(), CourseError> {
    if len > MAX_COURSE_FILE_BYTES {
        return Err(CourseError::Limit(format!(
            "file larger than {} MiB",
            MAX_COURSE_FILE_BYTES / (1024 * 1024)
        )));
    }
    Ok(())
}

/// The refusals of ADR 0019 for one event, shared by every XML pass of the crate. A DOCTYPE is refused, and
/// so is a general reference other than a character reference or one of the five predefined entities. For a
/// reference, returns the character it stands for.
pub(crate) fn guard(event: &Event<'_>) -> Result<Option<char>, CourseError> {
    match event {
        Event::DocType(_) => Err(CourseError::Xml("a DOCTYPE is refused".into())),
        Event::GeneralRef(r) => {
            if let Some(c) = r.resolve_char_ref().map_err(xml_err)? {
                return Ok(Some(c));
            }
            let c = match r.xml_content(XmlVersion::Explicit1_0).as_ref() {
                "lt" => '<',
                "gt" => '>',
                "amp" => '&',
                "apos" => '\'',
                "quot" => '"',
                other => {
                    return Err(CourseError::Xml(format!("entity &{other}; is refused")));
                }
            };
            Ok(Some(c))
        }
        _ => Ok(None),
    }
}

pub(super) fn xml_err(e: impl std::fmt::Display) -> CourseError {
    CourseError::Xml(e.to_string())
}

/// The local name of the element, without a namespace prefix.
pub fn local(e: &BytesStart<'_>) -> String {
    e.local_name().as_ref().to_string()
}

/// The value of attribute `name`, normalised as XML 1.0 prescribes (references resolved), `None` if missing.
pub fn attr(e: &BytesStart<'_>, name: &str) -> Result<Option<String>, CourseError> {
    let a = e.try_get_attribute(name).map_err(xml_err)?;
    a.map(|a| {
        a.normalized_value(XmlVersion::Explicit1_0)
            .map(|v| v.into_owned())
    })
    .transpose()
    .map_err(xml_err)
}

/// A finite number.
pub fn number(s: &str, what: &str) -> Result<f64, CourseError> {
    match s.trim().parse::<f64>() {
        Ok(v) if v.is_finite() => Ok(v),
        _ => Err(CourseError::Xml(format!("bad number in {what}: {s:?}"))),
    }
}

/// The notes of one read: deduplicated, in first-seen order, at most `MAX_NOTES` and then one
/// `NotesTruncated`.
#[derive(Default)]
pub(super) struct Notes {
    list: Vec<CourseNote>,
    seen: HashSet<CourseNote>,
    truncated: bool,
}

impl Notes {
    pub(super) fn add(&mut self, n: CourseNote) {
        if self.truncated || self.seen.contains(&n) {
            return;
        }
        if self.list.len() >= MAX_NOTES {
            self.truncated = true;
            self.list.push(CourseNote::NotesTruncated);
            return;
        }
        self.seen.insert(n.clone());
        self.list.push(n);
    }

    /// Adds `n` (once) even after the list was cut short: for a course left out and for the notes a reader adds at
    /// the end, about the file as a whole. There is at most one such note per course and a few more, so the list
    /// stays bounded by the course limit.
    pub(super) fn add_final(&mut self, n: CourseNote) {
        if self.seen.insert(n.clone()) {
            self.list.push(n);
        }
    }

    pub(super) fn into_vec(self) -> Vec<CourseNote> {
        self.list
    }
}

/// `text` if it has at most `max` characters, else a `Limit` error naming `what`.
pub(super) fn bounded(text: &str, max: usize, what: &str) -> Result<String, CourseError> {
    if text.chars().count() > max {
        return Err(CourseError::Limit(format!(
            "{what} longer than {max} characters"
        )));
    }
    Ok(text.to_string())
}

/// The bytes of codes and names one read may still copy into its result (`MAX_TOTAL_TEXT_BYTES`).
pub(super) struct TextBudget {
    left: usize,
}

impl Default for TextBudget {
    fn default() -> Self {
        TextBudget {
            left: MAX_TOTAL_TEXT_BYTES,
        }
    }
}

impl TextBudget {
    /// A copy of `s`, paid from the budget.
    pub(super) fn copy(&mut self, s: &str) -> Result<String, CourseError> {
        self.left = self.left.checked_sub(s.len()).ok_or_else(|| {
            CourseError::Limit(format!(
                "more than {} MiB of codes, names and description standards in the result",
                MAX_TOTAL_TEXT_BYTES / (1024 * 1024)
            ))
        })?;
        Ok(s.to_string())
    }
}
