//! The patch of an existing `.ppen` ([`write_ppen_patch`]). The base file is read twice, by the reader (with
//! all its budgets) for the values Purple Pen and the reader see, and by the lossless parse (`base.rs`) for its
//! bytes. Event objects are matched to base objects; a matched object is merged field by field, an object
//! with no changed field keeps its original bytes, and everything the event does not mention is kept. Only
//! [`Deletions`] delete.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use quick_xml::Reader;
use quick_xml::events::Event;

use super::base::{Base, Object, Span, Store, base_text, parse_base};
use super::out::{Body, Child, Element, Style, escape_attr, write_element};
use super::{
    AssignedIds, Frame, KeptObject, MAX_ID, Plan, PpenWriteError, Written, check_limits,
    circle_gaps_element, control_element, course_control_element, course_element, courses_element,
    exchange_at_normal, leg_element, same_points, same_ranges, special_element,
};
use crate::CourseError;
use crate::model::*;
use crate::ppen::{DEFAULT_PRINT_SCALE, GapRules, KNOWN_SPECIALS, PKind, State, read_ppen, scale};
use crate::xml::{drive, xml_err};

/// What a patch does beyond writing the event into the base file.
#[derive(Debug, Clone, Default)]
pub struct PatchOptions {
    /// Objects of the base file to delete.
    pub delete: Deletions,
}

/// Objects of the base file to delete, by Purple Pen id. A patch deletes nothing else: an object the event does
/// not mention is kept.
///
/// A deleted control takes its legs with it; the patch refuses to delete a control that a course of the
/// output still visits. A deleted course takes its course-controls with it and leaves the course lists of
/// the special objects; a special object left with no course is deleted too, as Purple Pen does
/// (`ChangeEvent.DeleteCourse`). Nothing the event holds can be deleted.
#[derive(Debug, Clone, Default)]
pub struct Deletions {
    pub controls: Vec<u32>,
    pub courses: Vec<u32>,
    pub specials: Vec<u32>,
}

/// Writes `event` into the `.ppen` file `base` and returns the patched file.
///
/// Event objects are matched to the objects of the base file, each base object at most once. Normal
/// controls match by code, courses by name; if the code or name is not in the base file, the event's id
/// matches (a code change, a rename). Other controls match by id, then by the code the reader gives them
/// (`STA1`, `FIN1` …), then by kind and position; special objects by id, then by kind and points. Ids are
/// not used at all if they contradict the base file somewhere (an event control or course whose code or
/// name names a base object with another id): then the event comes from another file.
///
/// A matched object keeps the bytes of every field the event leaves as it is; changed fields are written
/// anew, and attributes and elements the writer does not know are carried. An object with no change keeps
/// its original bytes, so a patch with the event the reader read from `base` gives `base` back. New objects
/// get the event's id if it is free, else the largest id plus one, and go after the last object of their
/// kind in the base file's style. The `<event>` element is never changed.
pub fn write_ppen_patch(
    base: &[u8],
    event: &RawEvent,
    options: &PatchOptions,
) -> Result<Written, PpenWriteError> {
    let text = base_text(base)?;
    let parsed = parse_base(text)?;
    let reader = read_ppen(base)?;
    let mut st = State::default();
    drive(base, &mut st)?;
    check_limits(event)?;
    let view = View::new(text, &parsed, &st, &reader)?;
    Patch::new(event, &view, &options.delete)?.write()
}

fn mismatch() -> PpenWriteError {
    CourseError::Xml("the base file reads differently in the reader and in the patch".into()).into()
}

/// The base file as the reader and the lossless parse see it, indexed for the patch.
struct View<'a> {
    text: &'a str,
    base: &'a Base<'a>,
    st: &'a State,
    reader: &'a RawEvent,
    rules: GapRules,
    standard: String,
    /// Indices into `base.objects`, parallel to `st.controls`, `st.courses` and `st.specials`.
    control_objs: Vec<usize>,
    course_objs: Vec<usize>,
    special_objs: Vec<usize>,
    cc_objs: HashMap<i64, usize>,
    /// Per leg object: its pair of control ids.
    leg_pairs: Vec<(usize, (i64, i64))>,
    /// The first leg object of each pair, the one Purple Pen and the reader use.
    leg_objs: HashMap<(i64, i64), usize>,
    control_by_id: HashMap<i64, usize>,
    course_by_id: HashMap<i64, usize>,
    special_by_id: HashMap<i64, usize>,
    /// Per base course: its course-control ids in chain order, `None` for a variation course.
    chains: Vec<Option<Vec<i64>>>,
    /// Per base special: the index of the reader's special, if the reader reads it.
    read_specials: Vec<Option<usize>>,
}

fn int_attr(o: &Object<'_>, name: &str) -> Option<i64> {
    o.attrs
        .iter()
        .find(|(k, _)| *k == name)
        .and_then(|(_, v)| v.trim().parse().ok())
}

impl<'a> View<'a> {
    fn new(
        text: &'a str,
        base: &'a Base<'a>,
        st: &'a State,
        reader: &'a RawEvent,
    ) -> Result<View<'a>, PpenWriteError> {
        let mut v = View {
            text,
            base,
            st,
            reader,
            rules: st.gap_rules(),
            standard: st.standard.clone().unwrap_or_else(|| "2004".into()),
            control_objs: Vec::new(),
            course_objs: Vec::new(),
            special_objs: Vec::new(),
            cc_objs: HashMap::new(),
            leg_pairs: Vec::new(),
            leg_objs: HashMap::new(),
            control_by_id: HashMap::new(),
            course_by_id: HashMap::new(),
            special_by_id: HashMap::new(),
            chains: Vec::with_capacity(st.courses.len()),
            read_specials: Vec::with_capacity(st.specials.len()),
        };
        for (i, o) in base.objects.iter().enumerate() {
            match o.kind {
                Store::Control => {
                    v.control_by_id.insert(o.id, v.control_objs.len());
                    v.control_objs.push(i);
                }
                Store::Course => {
                    v.course_by_id.insert(o.id, v.course_objs.len());
                    v.course_objs.push(i);
                }
                Store::Special => {
                    v.special_by_id.insert(o.id, v.special_objs.len());
                    v.special_objs.push(i);
                }
                Store::CourseControl => {
                    v.cc_objs.insert(o.id, i);
                }
                Store::Leg => {
                    let pair = int_attr(o, "start-control").zip(int_attr(o, "end-control"));
                    let pair = pair.ok_or_else(mismatch)?;
                    v.leg_pairs.push((i, pair));
                    v.leg_objs.entry(pair).or_insert(i);
                }
            }
        }
        if v.control_objs.len() != st.controls.len()
            || reader.controls.len() != st.controls.len()
            || v.course_objs.len() != st.courses.len()
            || v.special_objs.len() != st.specials.len()
        {
            return Err(mismatch());
        }
        for pc in &st.courses {
            let mut chain = Vec::new();
            let mut seen = HashSet::new();
            let mut next = pc.first;
            let mut variation = false;
            while let Some(id) = next {
                let Some(cc) = st.ccs.get(&id).filter(|_| seen.insert(id)) else {
                    break;
                };
                if cc.variation {
                    variation = true;
                    break;
                }
                chain.push(id);
                next = cc.next;
            }
            v.chains.push((!variation).then_some(chain));
        }
        let mut next = 0;
        for ps in &st.specials {
            let read = KNOWN_SPECIALS.contains(&ps.kind.as_str())
                && (ps.kind != "text" || ps.locations.len() >= 2);
            v.read_specials.push(read.then(|| {
                next += 1;
                next - 1
            }));
        }
        if next != reader.specials.len() {
            return Err(mismatch());
        }
        Ok(v)
    }

    fn object(&self, i: usize) -> &'a Object<'a> {
        &self.base.objects[i]
    }

    fn course_label(&self, bi: usize) -> &str {
        &self.st.courses[bi].name
    }
}

/// A kind of control as one number, a map issue point counting as a start (as the reader reads it).
fn kind_code(k: ControlKind) -> u8 {
    match k {
        ControlKind::Control => 0,
        ControlKind::Start => 1,
        ControlKind::Finish => 2,
        ControlKind::CrossingPoint => 3,
        ControlKind::MapExchange => 4,
        ControlKind::EndOfMarkedRoute => 5,
    }
}

fn pkind_code(k: PKind) -> u8 {
    match k {
        PKind::Normal => 0,
        PKind::Start | PKind::MapIssue => 1,
        PKind::Finish => 2,
        PKind::CrossingPoint => 3,
        PKind::MapExchange => 4,
    }
}

/// The bits of `v` as an `f32`, `-0` as `0`: a hash key for numbers compared as Purple Pen stores them.
fn bits(v: f64) -> u32 {
    let f = v as f32;
    if f == 0.0 { 0 } else { f.to_bits() }
}

fn f32o(v: Option<f64>) -> Option<u32> {
    v.map(bits)
}

/// What a special object looks like, without id and courses: kind, points, and text and size of a text.
type Geometry = (&'static str, Vec<(u32, u32)>, Option<(String, u32)>);

fn geometry(s: &Special<RawPoint>) -> Geometry {
    let pts = |p: &[RawPoint]| -> Vec<(u32, u32)> {
        p.iter().map(|p| (bits(p.x_mm), bits(p.y_mm))).collect()
    };
    let rings =
        |r: &[Vec<RawPoint>]| -> Vec<(u32, u32)> { r.iter().flat_map(|r| pts(r)).collect() };
    match s {
        Special::FirstAid { pt, .. } => ("first-aid", pts(std::slice::from_ref(pt)), None),
        Special::Water { pt, .. } => ("water", pts(std::slice::from_ref(pt)), None),
        Special::ForbiddenRoute { path, .. } => ("forbidden-route", pts(path), None),
        Special::Boundary { path, .. } => ("boundary", pts(path), None),
        Special::Line { path, .. } => ("line", pts(path), None),
        Special::MarkedRoute { path, .. } => ("marked-route", pts(path), None),
        Special::OutOfBounds { rings: r, .. } => ("out-of-bounds", rings(r), None),
        Special::Dangerous { rings: r, .. } => ("dangerous-area", rings(r), None),
        Special::WhiteOut { rings: r, .. } => ("white-out", rings(r), None),
        Special::Text {
            pt, text, size_mm, ..
        } => (
            "text",
            pts(std::slice::from_ref(pt)),
            Some((text.clone(), bits(*size_mm))),
        ),
    }
}

/// For each event object, the index of its base object (into `st.controls`, `st.courses`, `st.specials`).
struct Matches {
    controls: Vec<Option<usize>>,
    courses: Vec<Option<usize>>,
    specials: Vec<Option<usize>>,
    /// The event's ids agree with the base file wherever a code or name also matches.
    trusted: bool,
}

fn match_objects(ev: &RawEvent, v: &View<'_>) -> Result<Matches, PpenWriteError> {
    let st = v.st;
    let mut by_code: HashMap<&str, usize> = HashMap::new();
    let mut by_derived: HashMap<&str, usize> = HashMap::new();
    for (i, rc) in v.reader.controls.iter().enumerate() {
        let map = if st.controls[i].kind == PKind::Normal {
            &mut by_code
        } else {
            &mut by_derived
        };
        map.insert(rc.code.as_str(), i);
    }
    let mut by_name: HashMap<&str, Vec<usize>> = HashMap::new();
    for (i, pc) in st.courses.iter().enumerate() {
        by_name.entry(pc.name.as_str()).or_default().push(i);
    }

    // Ids from another file: an id that disagrees with the base object the code or name names.
    let mut trusted = true;
    for c in &ev.controls {
        let map = if c.kind == ControlKind::Control {
            &by_code
        } else {
            &by_derived
        };
        if let (Some(id), Some(&bi)) = (c.id, map.get(c.code.as_str()))
            && st.controls[bi].id != i64::from(id)
        {
            trusted = false;
        }
    }
    for c in &ev.courses {
        if let (Some(id), Some(list)) = (c.id, by_name.get(c.name.as_str()))
            && !list.iter().any(|&bi| st.courses[bi].id == i64::from(id))
        {
            trusted = false;
        }
    }
    let given = |id: Option<u32>| id.filter(|_| trusted).map(i64::from);

    // Controls.
    let mut taken = vec![false; st.controls.len()];
    let mut controls = vec![None; ev.controls.len()];
    let take = |ei: usize, bi: usize, taken: &mut Vec<bool>, m: &mut Vec<Option<usize>>| {
        taken[bi] = true;
        m[ei] = Some(bi);
    };
    for (ei, c) in ev.controls.iter().enumerate() {
        if c.kind == ControlKind::Control
            && let Some(&bi) = by_code.get(c.code.as_str())
            && !taken[bi]
        {
            take(ei, bi, &mut taken, &mut controls);
        }
    }
    for (ei, c) in ev.controls.iter().enumerate() {
        if controls[ei].is_some() {
            continue;
        }
        if let Some(&bi) = given(c.id).and_then(|id| v.control_by_id.get(&id))
            && !taken[bi]
        {
            let fits = if c.kind == ControlKind::Control {
                !by_code.contains_key(c.code.as_str())
            } else {
                st.controls[bi].kind != PKind::Normal
            };
            if fits {
                take(ei, bi, &mut taken, &mut controls);
            }
        }
    }
    for (ei, c) in ev.controls.iter().enumerate() {
        if controls[ei].is_none()
            && c.kind != ControlKind::Control
            && let Some(&bi) = by_derived.get(c.code.as_str())
            && !taken[bi]
        {
            take(ei, bi, &mut taken, &mut controls);
        }
    }
    let mut by_place: HashMap<(u8, u32, u32), Vec<usize>> = HashMap::new();
    for (bi, pc) in st.controls.iter().enumerate() {
        if pc.kind != PKind::Normal
            && !taken[bi]
            && let Some(p) = pc.position
        {
            by_place
                .entry((pkind_code(pc.kind), bits(p.x_mm), bits(p.y_mm)))
                .or_default()
                .push(bi);
        }
    }
    for (ei, c) in ev.controls.iter().enumerate() {
        if controls[ei].is_none()
            && c.kind != ControlKind::Control
            && let Some(p) = c.position
            && let Some(list) = by_place.get(&(kind_code(c.kind), bits(p.x_mm), bits(p.y_mm)))
            && let Some(&bi) = list.iter().find(|&&bi| !taken[bi])
        {
            take(ei, bi, &mut taken, &mut controls);
        }
    }

    // Courses.
    let mut taken = vec![false; st.courses.len()];
    let mut courses = vec![None; ev.courses.len()];
    for (ei, c) in ev.courses.iter().enumerate() {
        if let Some(list) = by_name.get(c.name.as_str())
            && let Some(&bi) = list.iter().find(|&&bi| !taken[bi])
        {
            take(ei, bi, &mut taken, &mut courses);
        }
    }
    for (ei, c) in ev.courses.iter().enumerate() {
        if courses[ei].is_none()
            && !by_name.contains_key(c.name.as_str())
            && let Some(&bi) = given(c.id).and_then(|id| v.course_by_id.get(&id))
            && !taken[bi]
        {
            take(ei, bi, &mut taken, &mut courses);
        }
    }
    for (ei, c) in ev.courses.iter().enumerate() {
        if let Some(bi) = courses[ei]
            && v.chains[bi].is_none()
        {
            return Err(PpenWriteError::Unsupported(format!(
                "course {} matches course {} of the base file, which has forks or loops (Purple Pen \
                 variations); the patch does not rewrite those",
                c.name,
                v.course_label(bi)
            )));
        }
    }

    // Special objects, only those the reader reads.
    let mut taken = vec![false; st.specials.len()];
    let mut specials = vec![None; ev.specials.len()];
    for (ei, s) in ev.specials.iter().enumerate() {
        if let Some(&bi) = given(s.id()).and_then(|id| v.special_by_id.get(&id))
            && v.read_specials[bi].is_some()
            && !taken[bi]
        {
            take(ei, bi, &mut taken, &mut specials);
        }
    }
    let mut by_shape: HashMap<Geometry, Vec<usize>> = HashMap::new();
    for (bi, ri) in v.read_specials.iter().enumerate() {
        if let Some(ri) = ri
            && !taken[bi]
        {
            by_shape
                .entry(geometry(&v.reader.specials[*ri]))
                .or_default()
                .push(bi);
        }
    }
    for (ei, s) in ev.specials.iter().enumerate() {
        if specials[ei].is_none()
            && let Some(list) = by_shape.get(&geometry(s))
            && let Some(&bi) = list.iter().find(|&&bi| !taken[bi])
        {
            take(ei, bi, &mut taken, &mut specials);
        }
    }
    Ok(Matches {
        controls,
        courses,
        specials,
        trusted,
    })
}

/// The largest id among `ids` that Purple Pen can hold, 0 if none.
fn max_id(ids: impl Iterator<Item = i64>) -> u32 {
    ids.filter(|i| *i > 0)
        .map(|i| u32::try_from(i.min(i64::from(MAX_ID))).unwrap_or(MAX_ID))
        .max()
        .unwrap_or(0)
}

/// Counts up from the largest id of a store.
struct NextId {
    last: u32,
    what: &'static str,
}

impl NextId {
    fn take(&mut self) -> Result<u32, PpenWriteError> {
        if self.last >= MAX_ID {
            return Err(PpenWriteError::Limit(format!(
                "no {} id left below {MAX_ID}",
                self.what
            )));
        }
        self.last += 1;
        Ok(self.last)
    }
}

/// Ids of one store: a matched object keeps its base id; another takes its given id if that is free in the
/// store, else the largest id of base and event plus one.
fn patch_ids(
    given: &[Option<u32>],
    matched: &[Option<i64>],
    base_ids: &[i64],
    what: &'static str,
) -> Result<Vec<u32>, PpenWriteError> {
    let valid = |id: &Option<u32>| id.filter(|i| (1..=MAX_ID).contains(i));
    let mut next = NextId {
        last: max_id(base_ids.iter().copied())
            .max(given.iter().filter_map(valid).max().unwrap_or(0)),
        what,
    };
    let mut used: HashSet<i64> = base_ids.iter().copied().collect();
    let mut out = Vec::with_capacity(given.len());
    for (g, m) in given.iter().zip(matched) {
        out.push(match (m, valid(g)) {
            (Some(b), _) => u32::try_from(*b).unwrap_or(0),
            (None, Some(g)) if used.insert(i64::from(g)) => g,
            _ => {
                let id = next.take()?;
                used.insert(i64::from(id));
                id
            }
        });
    }
    Ok(out)
}

/// One attribute of a written element: carried from the base text, or new (escaped when written).
enum AttrOut<'t> {
    Base(&'t str, &'t str),
    New(&'static str, String),
}

enum KidOut<'t> {
    /// A child element of the base file, as it is.
    Base(&'t str),
    /// A child element of the base file, merged; without indent and line end.
    Text(String),
    New(Element),
}

/// The position of a child in Purple Pen's write order (`WriteAttributesAndContent`), then the scale of
/// circle gaps (`gaps` before `circle-gaps`). Unknown children go last.
type Rank = (u8, i64, u8);

fn rank(store: Option<Store>, name: &str, scale: i64) -> Rank {
    let r = match store {
        None => 0,
        Some(Store::Control) => match name {
            "code" => 0,
            "location" => 1,
            "description" => 2,
            "punch-pattern" => 3,
            "gaps" => return (4, scale, 0),
            "circle-gaps" => return (4, scale, 1),
            "description-text" => 5,
            "description-text-line" => 6,
            _ => u8::MAX,
        },
        Some(Store::Course) => match name {
            "name" => 0,
            "secondary-title" => 1,
            "labels" => 2,
            "first" => 3,
            "print-area" => 4,
            "options" => 5,
            "relay" => 6,
            "relay-branch" => 7,
            "part-options" => 8,
            _ => u8::MAX,
        },
        Some(Store::CourseControl) => match name {
            "next" => 0,
            "variation" => 1,
            "number-location" => 2,
            "description-text-line" => 3,
            _ => u8::MAX,
        },
        Some(Store::Leg) => match name {
            "flagging" => 0,
            "bends" => 1,
            "gaps" => 2,
            _ => u8::MAX,
        },
        Some(Store::Special) => match name {
            "text" => 0,
            "image-data" => 1,
            "appearance" | "font" => 2,
            "location" => 3,
            "courses" => 4,
            _ => u8::MAX,
        },
    };
    (r, 0, 0)
}

/// The name of the element that starts at the beginning of `t`.
fn name_at(t: &str) -> &str {
    let t = t.get(1..).unwrap_or("");
    let end = t
        .find(|c: char| c.is_ascii_whitespace() || c == '/' || c == '>')
        .unwrap_or(t.len());
    &t[..end]
}

/// One element of the base text below the top level: its start tag, attributes and child elements.
struct Sub<'t> {
    head: Span,
    attrs: Vec<(&'t str, &'t str)>,
    children: Vec<Span>,
}

/// `part`, borrowed from `text` by quick-xml, as a slice of `text`.
fn slice_in<'t>(text: &'t str, part: &str) -> Result<&'t str, PpenWriteError> {
    let start = (part.as_ptr() as usize).wrapping_sub(text.as_ptr() as usize);
    start
        .checked_add(part.len())
        .and_then(|end| text.get(start..end))
        .ok_or_else(mismatch)
}

/// Parses the element at `span` of `text` (already checked by the base parse).
fn sub(text: &str, span: Span) -> Result<Sub<'_>, PpenWriteError> {
    let part = &text[span.clone()];
    let mut reader = Reader::from_str(part);
    let at = |p: u64| span.start + usize::try_from(p).unwrap_or(usize::MAX - span.start);
    let mut depth = 0usize;
    let mut out = Sub {
        head: span.start..span.start,
        attrs: Vec::new(),
        children: Vec::new(),
    };
    let mut child_start = 0;
    loop {
        let before = at(reader.buffer_position());
        let event = reader.read_event().map_err(xml_err)?;
        let after = at(reader.buffer_position());
        match &event {
            Event::Start(e) | Event::Empty(e) => {
                let empty = matches!(event, Event::Empty(_));
                match depth {
                    0 => {
                        out.head = before..after;
                        for a in e.attributes() {
                            let a = a.map_err(xml_err)?;
                            out.attrs.push((
                                slice_in(text, a.key.as_ref())?,
                                slice_in(text, a.value.as_ref())?,
                            ));
                        }
                        if empty {
                            break;
                        }
                    }
                    1 if empty => out.children.push(before..after),
                    1 => child_start = before,
                    _ => {}
                }
                if !empty {
                    depth += 1;
                }
            }
            Event::End(_) => {
                depth = depth.saturating_sub(1);
                match depth {
                    0 => break,
                    1 => out.children.push(child_start..after),
                    _ => {}
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(out)
}

/// The integer attribute `name` of the element at `span`, if it has one.
fn sub_int(text: &str, span: &Span, name: &str) -> Option<i64> {
    sub(text, span.clone())
        .ok()?
        .attrs
        .iter()
        .find(|(k, _)| *k == name)
        .and_then(|(_, v)| v.trim().parse().ok())
}

/// An element of the base text rebuilt field by field: attributes and children the caller decides on, and
/// everything else carried as it is.
struct Merge<'t> {
    text: &'t str,
    store: Option<Store>,
    head: &'t str,
    base_attrs: Vec<(&'t str, &'t str)>,
    base_kids: Vec<(Span, &'t str)>,
    attr_used: Vec<bool>,
    kid_used: Vec<bool>,
    attrs: Vec<AttrOut<'t>>,
    kids: Vec<(Rank, KidOut<'t>)>,
    changed: bool,
    head_changed: bool,
}

impl<'t> Merge<'t> {
    fn new(
        text: &'t str,
        store: Option<Store>,
        head: Span,
        attrs: Vec<(&'t str, &'t str)>,
        children: &[Span],
    ) -> Merge<'t> {
        let base_kids: Vec<(Span, &'t str)> = children
            .iter()
            .map(|s| (s.clone(), name_at(&text[s.clone()])))
            .collect();
        Merge {
            text,
            store,
            head: &text[head],
            attr_used: vec![false; attrs.len()],
            kid_used: vec![false; base_kids.len()],
            base_attrs: attrs,
            base_kids,
            attrs: Vec::new(),
            kids: Vec::new(),
            changed: false,
            head_changed: false,
        }
    }

    fn object(text: &'t str, o: &Object<'t>) -> Merge<'t> {
        Merge::new(
            text,
            Some(o.kind),
            o.head.clone(),
            o.attrs.clone(),
            &o.children,
        )
    }

    fn sub(text: &'t str, span: &Span) -> Result<Merge<'t>, PpenWriteError> {
        let s = sub(text, span.clone())?;
        Ok(Merge::new(text, None, s.head, s.attrs, &s.children))
    }

    /// An attribute the writer owns: the base text if the value is the same, else `new` (none drops it).
    fn attr(&mut self, name: &'static str, same: bool, new: Option<String>) {
        let found = self.base_attrs.iter().position(|(k, _)| *k == name);
        if let Some(i) = found {
            self.attr_used[i] = true;
        }
        if same {
            if let Some(i) = found {
                let (k, v) = self.base_attrs[i];
                self.attrs.push(AttrOut::Base(k, v));
            }
            return;
        }
        if found.is_none() && new.is_none() {
            return;
        }
        self.changed = true;
        self.head_changed = true;
        if let Some(v) = new {
            self.attrs.push(AttrOut::New(name, v));
        }
    }

    fn drop_attr(&mut self, name: &str) {
        if let Some(i) = self.base_attrs.iter().position(|(k, _)| *k == name) {
            self.attr_used[i] = true;
            self.changed = true;
            self.head_changed = true;
        }
    }

    fn base_rank(&self, i: usize) -> Rank {
        let (span, name) = &self.base_kids[i];
        let scale = if self.store == Some(Store::Control) && matches!(*name, "gaps" | "circle-gaps")
        {
            sub_int(self.text, span, "scale").unwrap_or(0)
        } else {
            0
        };
        rank(self.store, name, scale)
    }

    fn keep_kid(&mut self, i: usize) {
        self.kid_used[i] = true;
        let r = self.base_rank(i);
        let t = &self.text[self.base_kids[i].0.clone()];
        self.kids.push((r, KidOut::Base(t)));
    }

    /// The children named `names`: the base ones if the value is the same, else `new`.
    fn kids(&mut self, names: &[&str], same: bool, new: Vec<Element>) {
        let mut found = false;
        for i in 0..self.base_kids.len() {
            if !self.kid_used[i] && names.contains(&self.base_kids[i].1) {
                found = true;
                if same {
                    self.keep_kid(i);
                } else {
                    self.kid_used[i] = true;
                }
            }
        }
        if same || (!found && new.is_empty()) {
            return;
        }
        self.changed = true;
        for e in new {
            self.push(e);
        }
    }

    /// The first base child named `name` not yet decided on.
    fn find_kid(&self, name: &str) -> Option<usize> {
        (0..self.base_kids.len()).find(|&i| !self.kid_used[i] && self.base_kids[i].1 == name)
    }

    fn drop_kid(&mut self, i: usize) {
        self.kid_used[i] = true;
        self.changed = true;
    }

    fn replace_kid(&mut self, i: usize, text: String) {
        let r = self.base_rank(i);
        self.kid_used[i] = true;
        self.changed = true;
        self.kids.push((r, KidOut::Text(text)));
    }

    fn push(&mut self, e: Element) {
        let scale = e
            .attrs
            .iter()
            .find(|(k, _)| k == "scale")
            .and_then(|(_, v)| v.parse().ok())
            .unwrap_or(0);
        self.changed = true;
        self.kids
            .push((rank(self.store, e.name, scale), KidOut::New(e)));
    }

    /// The element as text from its `<` to its last `>`, or `None` if nothing changed.
    fn finish(mut self, name: &str, depth: usize, style: &Style) -> Option<String> {
        if !self.changed {
            return None;
        }
        for i in 0..self.base_attrs.len() {
            if !self.attr_used[i] {
                let (k, v) = self.base_attrs[i];
                self.attrs.push(AttrOut::Base(k, v));
            }
        }
        for i in 0..self.base_kids.len() {
            if !self.kid_used[i] {
                self.keep_kid(i);
            }
        }
        self.kids.sort_by_key(|(r, _)| *r);
        let newline = style.newline.as_str();
        let indent = |s: &mut String, d: usize| {
            for _ in 0..d {
                s.push_str(&style.indent);
            }
        };
        let mut s = String::new();
        if !self.head_changed && !self.head.ends_with("/>") && !self.kids.is_empty() {
            s.push_str(self.head);
        } else {
            s.push('<');
            s.push_str(name);
            for a in &self.attrs {
                s.push(' ');
                match a {
                    AttrOut::Base(k, v) => {
                        s.push_str(k);
                        s.push_str("=\"");
                        s.push_str(&v.replace('"', "&quot;"));
                    }
                    AttrOut::New(k, v) => {
                        s.push_str(k);
                        s.push_str("=\"");
                        escape_attr(&mut s, v);
                    }
                }
                s.push('"');
            }
            if self.kids.is_empty() {
                s.push_str(" />");
                return Some(s);
            }
            s.push('>');
        }
        s.push_str(newline);
        for (_, k) in &self.kids {
            match k {
                KidOut::Base(t) => {
                    indent(&mut s, depth + 1);
                    s.push_str(t);
                    s.push_str(newline);
                }
                KidOut::Text(t) => {
                    indent(&mut s, depth + 1);
                    s.push_str(t);
                    s.push_str(newline);
                }
                KidOut::New(e) => write_element(&mut s, e, depth + 1, style),
            }
        }
        indent(&mut s, depth);
        s.push_str("</");
        s.push_str(name);
        s.push('>');
        Some(s)
    }
}

fn attr_of(e: &Element, name: &str) -> Option<String> {
    e.attrs
        .iter()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v.clone())
}

/// The children of a built element, by name.
struct Kids(Vec<Element>);

impl Kids {
    fn of(e: Element) -> Kids {
        Kids(match e.body {
            Body::Children(c) => c
                .into_iter()
                .filter_map(|c| match c {
                    Child::Element(e) => Some(e),
                    Child::Raw(_) => None,
                })
                .collect(),
            Body::Empty | Body::Text(_) => Vec::new(),
        })
    }

    fn take(&mut self, name: &str) -> Vec<Element> {
        let (a, b) = std::mem::take(&mut self.0)
            .into_iter()
            .partition(|e| e.name == name);
        self.0 = b;
        a
    }
}

/// What the patch does to one base object.
enum Edit {
    Drop,
    Replace(String),
}

struct Patch<'a> {
    ev: &'a RawEvent,
    v: &'a View<'a>,
    m: Matches,
    plan: Plan<'a>,
    style: &'a Style,
    /// Per event course: the ids of its course-controls in stop order.
    cc_ids: Vec<Vec<u32>>,
    /// Per event course: its `order`, and whether that is the base value.
    orders: Vec<(usize, bool)>,
    /// New `order` of kept base courses, when the courses are numbered anew.
    kept_orders: HashMap<usize, usize>,
    /// Base objects (indices into `st.controls`, `st.courses`, `st.specials`) the event matched.
    matched_controls: HashSet<usize>,
    matched_courses: HashSet<usize>,
    matched_specials: HashSet<usize>,
    deleted_controls: HashSet<usize>,
    deleted_courses: HashSet<usize>,
    deleted_specials: HashSet<usize>,
    /// Base course-controls of matched or deleted courses that the output no longer has.
    deleted_ccs: Vec<i64>,
    edits: BTreeMap<usize, Edit>,
    inserts: BTreeMap<Store, Vec<String>>,
    next_leg: NextId,
}

impl<'a> Patch<'a> {
    fn new(
        ev: &'a RawEvent,
        v: &'a View<'a>,
        delete: &Deletions,
    ) -> Result<Patch<'a>, PpenWriteError> {
        let st = v.st;
        let m = match_objects(ev, v)?;
        let given = |ids: Vec<Option<u32>>| -> Vec<Option<u32>> {
            ids.into_iter().map(|i| i.filter(|_| m.trusted)).collect()
        };
        let base_ids =
            |objs: &[usize]| -> Vec<i64> { objs.iter().map(|&i| v.object(i).id).collect() };
        let ids = AssignedIds {
            controls: patch_ids(
                &given(ev.controls.iter().map(|c| c.id).collect()),
                &m.controls
                    .iter()
                    .map(|b| b.map(|b| st.controls[b].id))
                    .collect::<Vec<_>>(),
                &base_ids(&v.control_objs),
                "control",
            )?,
            courses: patch_ids(
                &given(ev.courses.iter().map(|c| c.id).collect()),
                &m.courses
                    .iter()
                    .map(|b| b.map(|b| st.courses[b].id))
                    .collect::<Vec<_>>(),
                &base_ids(&v.course_objs),
                "course",
            )?,
            specials: patch_ids(
                &given(ev.specials.iter().map(Special::id).collect()),
                &m.specials
                    .iter()
                    .map(|b| b.map(|b| v.object(v.special_objs[b]).id))
                    .collect::<Vec<_>>(),
                &base_ids(&v.special_objs),
                "special object",
            )?,
        };

        // Deleted courses first: the course lists of special objects must not name them.
        let matched_courses: HashSet<usize> = m.courses.iter().flatten().copied().collect();
        let mut deleted_courses = HashSet::new();
        for &id in &delete.courses {
            let &bi = v.course_by_id.get(&i64::from(id)).ok_or_else(|| {
                PpenWriteError::Invalid(format!(
                    "course id {id} is to be deleted, but the base file has none"
                ))
            })?;
            if matched_courses.contains(&bi) {
                return Err(PpenWriteError::Invalid(format!(
                    "course {} (id {id}) is to be deleted, but the event has it",
                    v.course_label(bi)
                )));
            }
            if v.chains[bi].is_none() {
                return Err(PpenWriteError::Unsupported(format!(
                    "course {} (id {id}) has forks or loops (Purple Pen variations); the patch does not \
                     delete those",
                    v.course_label(bi)
                )));
            }
            deleted_courses.insert(bi);
        }
        let kept_courses: Vec<usize> = (0..st.courses.len())
            .filter(|bi| !matched_courses.contains(bi) && !deleted_courses.contains(bi))
            .collect();

        let map_issue = ev
            .controls
            .iter()
            .zip(&m.controls)
            .map(|(c, b)| {
                c.kind == ControlKind::Start
                    && b.is_some_and(|b| st.controls[b].kind == PKind::MapIssue)
            })
            .collect();
        let other_courses = kept_courses
            .iter()
            .filter_map(|&bi| {
                let pc = &st.courses[bi];
                u32::try_from(pc.id).ok().map(|id| (pc.name.as_str(), id))
            })
            .collect();
        let plan = Plan::new(
            ev,
            Frame {
                ids,
                map_scale: st.map_scale,
                item_scaling: st.item_scaling,
                map_issue,
                other_courses,
            },
        )?;

        // Course-controls: a matched course keeps its own in chain order.
        let mut next_cc = NextId {
            last: max_id(v.cc_objs.keys().copied()),
            what: "course-control",
        };
        let mut cc_ids = Vec::with_capacity(ev.courses.len());
        let mut deleted_ccs = Vec::new();
        for (ei, c) in ev.courses.iter().enumerate() {
            let chain: &[i64] = m.courses[ei]
                .and_then(|bi| v.chains[bi].as_deref())
                .unwrap_or(&[]);
            let mut ids = Vec::with_capacity(c.stops.len());
            for si in 0..c.stops.len() {
                ids.push(match chain.get(si) {
                    Some(&id) => u32::try_from(id).unwrap_or(0),
                    None => next_cc.take()?,
                });
            }
            deleted_ccs.extend(chain.iter().skip(c.stops.len()));
            cc_ids.push(ids);
        }
        for &bi in &deleted_courses {
            deleted_ccs.extend(v.chains[bi].iter().flatten());
        }

        // Controls the output still visits: the event's stops, and the course-controls left as they are.
        let mut rewritten: HashSet<i64> = deleted_ccs.iter().copied().collect();
        for bi in m.courses.iter().flatten() {
            rewritten.extend(v.chains[*bi].iter().flatten());
        }
        let mut visited: HashSet<i64> = st
            .ccs
            .iter()
            .filter(|(id, _)| !rewritten.contains(id))
            .map(|(_, cc)| cc.control)
            .collect();
        for stops in &plan.stop_controls {
            visited.extend(stops.iter().map(|&ci| i64::from(plan.ids.controls[ci])));
        }
        let matched_controls: HashSet<usize> = m.controls.iter().flatten().copied().collect();
        let mut deleted_controls = HashSet::new();
        for &id in &delete.controls {
            let &bi = v.control_by_id.get(&i64::from(id)).ok_or_else(|| {
                PpenWriteError::Invalid(format!(
                    "control id {id} is to be deleted, but the base file has none"
                ))
            })?;
            let code = &v.reader.controls[bi].code;
            if matched_controls.contains(&bi) {
                return Err(PpenWriteError::Invalid(format!(
                    "control {code} (id {id}) is to be deleted, but the event has it"
                )));
            }
            if visited.contains(&i64::from(id)) {
                return Err(PpenWriteError::Invalid(format!(
                    "control {code} (id {id}) is to be deleted, but a course still visits it"
                )));
            }
            deleted_controls.insert(bi);
        }
        let matched_specials: HashSet<usize> = m.specials.iter().flatten().copied().collect();
        let mut deleted_specials = HashSet::new();
        for &id in &delete.specials {
            let &bi = v.special_by_id.get(&i64::from(id)).ok_or_else(|| {
                PpenWriteError::Invalid(format!(
                    "special object id {id} is to be deleted, but the base file has none"
                ))
            })?;
            if matched_specials.contains(&bi) {
                return Err(PpenWriteError::Invalid(format!(
                    "special object id {id} is to be deleted, but the event has it"
                )));
            }
            deleted_specials.insert(bi);
        }

        // Course order: kept while the event keeps the base order, else 1..n.
        let base_rank = |bi: usize| {
            let o = st.courses[bi].order;
            (if o <= 0 { i64::MAX } else { o }, bi)
        };
        let mut last = None;
        let mut new_seen = false;
        let mut in_order = true;
        for b in &m.courses {
            match b {
                Some(bi) => {
                    let r = base_rank(*bi);
                    in_order &= !new_seen && last.is_none_or(|l| l < r);
                    last = Some(r);
                }
                None => new_seen = true,
            }
        }
        let mut orders = Vec::with_capacity(ev.courses.len());
        let mut kept_orders = HashMap::new();
        if in_order {
            let mut next = st.courses.iter().map(|c| c.order).max().unwrap_or(0).max(0);
            for b in &m.courses {
                orders.push(match b {
                    Some(bi) => (usize::try_from(st.courses[*bi].order).unwrap_or(0), true),
                    None => {
                        next += 1;
                        (usize::try_from(next).unwrap_or(0), false)
                    }
                });
            }
        } else {
            let mut n = 0usize;
            for b in &m.courses {
                n += 1;
                orders.push((n, b.is_some_and(|bi| st.courses[bi].order == n as i64)));
            }
            let mut kept = kept_courses.clone();
            kept.sort_by_key(|&bi| base_rank(bi));
            for bi in kept {
                n += 1;
                if st.courses[bi].order != n as i64 {
                    kept_orders.insert(bi, n);
                }
            }
        }

        let next_leg = NextId {
            last: max_id(v.leg_pairs.iter().map(|(i, _)| v.object(*i).id)),
            what: "leg",
        };
        Ok(Patch {
            ev,
            v,
            m,
            plan,
            style: &v.base.style,
            cc_ids,
            orders,
            kept_orders,
            matched_controls,
            matched_courses,
            matched_specials,
            deleted_controls,
            deleted_courses,
            deleted_specials,
            deleted_ccs,
            edits: BTreeMap::new(),
            inserts: BTreeMap::new(),
            next_leg,
        })
    }

    fn replace(&mut self, obj: usize, merged: Option<String>) {
        if let Some(t) = merged {
            let o = self.v.object(obj);
            let lead = &self.v.text[o.span.start..o.head.start];
            self.edits.insert(obj, Edit::Replace(format!("{lead}{t}")));
        }
    }

    fn insert(&mut self, store: Store, e: &Element) {
        let newline = self.style.newline.as_str();
        let mut s = newline.to_string();
        write_element(&mut s, e, 1, self.style);
        s.truncate(s.len() - newline.len());
        self.inserts.entry(store).or_default().push(s);
    }

    fn write(mut self) -> Result<Written, PpenWriteError> {
        self.controls()?;
        self.courses()?;
        self.legs()?;
        self.specials()?;
        let kept = self.kept();
        self.assemble(kept)
    }

    /// The circle-gap scales `EventDB.FixControlPointGaps` fills from old gaps: the map scale and the print
    /// scale of every course of the output.
    fn fix_keys(&self) -> BTreeSet<i64> {
        let st = self.v.st;
        let mut keys = BTreeSet::new();
        if let Some(s) = st.map_scale {
            keys.insert(s.round() as i64);
        }
        for c in &self.ev.courses {
            keys.insert(
                c.print_scale_hint
                    .map_or(DEFAULT_PRINT_SCALE, f64::from)
                    .round() as i64,
            );
        }
        for (bi, pc) in st.courses.iter().enumerate() {
            if !self.matched_courses.contains(&bi) && !self.deleted_courses.contains(&bi) {
                keys.insert(pc.print_scale.unwrap_or(DEFAULT_PRINT_SCALE).round() as i64);
            }
        }
        keys
    }

    fn controls(&mut self) -> Result<(), PpenWriteError> {
        let fix_keys = self.fix_keys();
        for ei in 0..self.ev.controls.len() {
            let ec = &self.ev.controls[ei];
            let id = self.plan.ids.controls[ei];
            let gaps = &self.plan.circle_gaps[ei];
            let Some(bi) = self.m.controls[ei] else {
                let e = control_element(ec, id, gaps, &self.v.standard)?;
                self.insert(Store::Control, &e);
                continue;
            };
            let v = self.v;
            let (pc, rc) = (&v.st.controls[bi], &v.reader.controls[bi]);
            let same_kind = kind_code(ec.kind) == pkind_code(pc.kind);
            let same_code = match (ec.kind == ControlKind::Control, pc.kind == PKind::Normal) {
                (true, true) => rc.code == ec.code,
                (false, false) => true,
                _ => false,
            };
            let same_location = match (&ec.position, &rc.position) {
                (Some(a), Some(b)) => same_points(std::slice::from_ref(a), std::slice::from_ref(b)),
                (None, None) => true,
                _ => false,
            };
            let same_description = ec.description == rc.description;
            let rules = &v.rules;
            let changed_gaps: BTreeMap<i64, &[(f64, f64)]> = gaps
                .iter()
                .filter(|(k, (g, _))| !same_ranges(rules.gaps_at(pc, **k), g))
                .map(|(k, (g, _))| (*k, *g))
                .collect();
            if same_kind
                && same_code
                && same_location
                && same_description
                && changed_gaps.is_empty()
            {
                continue;
            }
            let full = control_element(ec, id, &BTreeMap::new(), &v.standard)?;
            let obj = v.control_objs[bi];
            let mut m = Merge::object(v.text, v.object(obj));
            m.attr("id", true, None);
            m.attr("kind", same_kind, attr_of(&full, "kind"));
            let mut kids = Kids::of(full);
            m.kids(&["code"], same_code, kids.take("code"));
            m.kids(&["location"], same_location, kids.take("location"));
            m.kids(&["description"], same_description, kids.take("description"));
            if !changed_gaps.is_empty() {
                // Changed scales replace their `gaps` and `circle-gaps`; old gaps for every scale (the `gaps`
                // attribute) become explicit entries where Purple Pen would put them on load.
                let old = pc.gaps.get(&0);
                let mut write = changed_gaps;
                if let Some(old) = old {
                    m.drop_attr("gaps");
                    for k in &fix_keys {
                        write.entry(*k).or_insert(old.as_slice());
                    }
                }
                for i in 0..m.base_kids.len() {
                    let (span, name) = &m.base_kids[i];
                    if !m.kid_used[i] && matches!(*name, "gaps" | "circle-gaps") {
                        let k = sub_int(v.text, span, "scale").unwrap_or(0);
                        if write.contains_key(&k) || (old.is_some() && k == 0) {
                            m.drop_kid(i);
                        }
                    }
                }
                let what = format!("control {}", ec.code);
                for (k, g) in write {
                    if !g.is_empty() {
                        m.push(circle_gaps_element(k, g, &what)?);
                    }
                }
            }
            let merged = m.finish(Store::Control.element(), 1, self.style);
            self.replace(obj, merged);
        }
        for bi in self.deleted_controls.clone() {
            self.edits.insert(self.v.control_objs[bi], Edit::Drop);
        }
        Ok(())
    }

    fn courses(&mut self) -> Result<(), PpenWriteError> {
        let v = self.v;
        for ei in 0..self.ev.courses.len() {
            let ec = &self.ev.courses[ei];
            let id = self.plan.ids.courses[ei];
            let first = self.cc_ids[ei].first().copied();
            let (order, same_order) = self.orders[ei];
            let Some(bi) = self.m.courses[ei] else {
                let e = course_element(ec, id, order, first)?;
                self.insert(Store::Course, &e);
                self.course_controls(ei, &[])?;
                continue;
            };
            let pc = &v.st.courses[bi];
            let same_kind = pc.kind == ec.kind;
            let same_name = pc.name == ec.name;
            let same_title = pc.secondary_title == ec.secondary_title;
            let same_first = pc.first == first.map(i64::from);
            let same_scale = pc.print_scale.and_then(scale) == ec.print_scale_hint;
            let same_climb = f32o(pc.climb) == f32o(ec.climb_m);
            let same_length = f32o(pc.length) == f32o(ec.length_m);
            let same_options = same_scale && same_climb && same_length;
            if !(same_kind && same_name && same_title && same_first && same_order && same_options) {
                let full = course_element(ec, id, order, first)?;
                let obj = v.course_objs[bi];
                let mut m = Merge::object(v.text, v.object(obj));
                m.attr("id", true, None);
                m.attr("kind", same_kind, attr_of(&full, "kind"));
                m.attr("order", same_order, attr_of(&full, "order"));
                let mut kids = Kids::of(full);
                m.kids(&["name"], same_name, kids.take("name"));
                m.kids(
                    &["secondary-title"],
                    same_title,
                    kids.take("secondary-title"),
                );
                let new_first = kids.take("first");
                match (same_first, m.find_kid("first"), first) {
                    (true, _, _) => m.kids(&["first"], true, Vec::new()),
                    (false, Some(i), Some(f)) => {
                        let mut s = Merge::sub(v.text, &m.base_kids[i].0)?;
                        s.attr("course-control", false, Some(f.to_string()));
                        if let Some(t) = s.finish("first", 2, self.style) {
                            m.replace_kid(i, t);
                        }
                    }
                    (false, Some(i), None) => m.drop_kid(i),
                    (false, None, _) => m.kids(&["first"], false, new_first),
                }
                let new_options = kids.take("options");
                match (same_options, m.find_kid("options")) {
                    (true, _) => m.kids(&["options"], true, Vec::new()),
                    (false, Some(i)) => {
                        let built = new_options.first();
                        let get = |name: &str| built.and_then(|e| attr_of(e, name));
                        let mut s = Merge::sub(v.text, &m.base_kids[i].0)?;
                        s.attr("print-scale", same_scale, get("print-scale"));
                        s.attr("climb", same_climb, get("climb"));
                        s.attr("course-length", same_length, get("course-length"));
                        if let Some(t) = s.finish("options", 2, self.style) {
                            m.replace_kid(i, t);
                        }
                    }
                    (false, None) => m.kids(&["options"], false, new_options),
                }
                let merged = m.finish(Store::Course.element(), 1, self.style);
                self.replace(obj, merged);
            }
            let chain = v.chains[bi].as_deref().unwrap_or(&[]);
            self.course_controls(ei, chain)?;
        }
        for (&bi, &order) in &self.kept_orders.clone() {
            let obj = v.course_objs[bi];
            let mut m = Merge::object(v.text, v.object(obj));
            m.attr("order", false, Some(order.to_string()));
            let merged = m.finish(Store::Course.element(), 1, self.style);
            self.replace(obj, merged);
        }
        for bi in self.deleted_courses.clone() {
            self.edits.insert(v.course_objs[bi], Edit::Drop);
        }
        for id in self.deleted_ccs.clone() {
            if let Some(&obj) = v.cc_objs.get(&id) {
                self.edits.insert(obj, Edit::Drop);
            }
        }
        Ok(())
    }

    /// The course-controls of event course `ei`; `chain` holds those of its base course.
    fn course_controls(&mut self, ei: usize, chain: &[i64]) -> Result<(), PpenWriteError> {
        let v = self.v;
        let course = &self.ev.courses[ei];
        for (si, stop) in course.stops.iter().enumerate() {
            let ci = self.plan.stop_controls[ei][si];
            let control = self.plan.ids.controls[ci];
            let exchange = exchange_at_normal(stop, &self.ev.controls[ci]);
            let id = self.cc_ids[ei][si];
            let next = self.cc_ids[ei].get(si + 1).copied();
            let Some(bid) = chain.get(si) else {
                let e = course_control_element(id, control, stop, exchange, next)?;
                self.insert(Store::CourseControl, &e);
                continue;
            };
            let (Some(pcc), Some(&obj)) = (v.st.ccs.get(bid), v.cc_objs.get(bid)) else {
                return Err(mismatch());
            };
            let at_normal = v
                .control_by_id
                .get(&pcc.control)
                .is_some_and(|&i| v.st.controls[i].kind == PKind::Normal);
            let same_control = pcc.control == i64::from(control);
            let same_exchange = (pcc.map_exchange && at_normal) == exchange;
            let same_flip = (pcc.map_flip && pcc.map_exchange) == stop.map_flip;
            let same_next = pcc.next == next.map(i64::from);
            let same_number = pcc.number_at.map(|o| (bits(o.dx_mm), bits(o.dy_mm)))
                == stop.number_text_at.map(|o| (bits(o.dx_mm), bits(o.dy_mm)));
            if same_control && same_exchange && same_flip && same_next && same_number {
                continue;
            }
            let full = course_control_element(id, control, stop, exchange, next)?;
            let mut m = Merge::object(v.text, v.object(obj));
            m.attr("id", true, None);
            m.attr("control", same_control, attr_of(&full, "control"));
            m.attr(
                "map-exchange",
                same_exchange,
                attr_of(&full, "map-exchange"),
            );
            m.attr("map-flip", same_flip, attr_of(&full, "map-flip"));
            let mut kids = Kids::of(full);
            m.kids(&["next"], same_next, kids.take("next"));
            m.kids(
                &["number-location"],
                same_number,
                kids.take("number-location"),
            );
            let merged = m.finish(Store::CourseControl.element(), 1, self.style);
            self.replace(obj, merged);
        }
        Ok(())
    }

    fn legs(&mut self) -> Result<(), PpenWriteError> {
        let v = self.v;
        for li in 0..self.plan.legs.len() {
            let u = &self.plan.legs[li];
            let (a, b) = (self.plan.ids.controls[u.from], self.plan.ids.controls[u.to]);
            let pair = (i64::from(a), i64::from(b));
            let (Some(&obj), Some(pleg)) = (v.leg_objs.get(&pair), v.st.legs.get(&pair)) else {
                if u.written() {
                    let id = self.next_leg.take()?;
                    let e = leg_element(id, a, b, u.leg, u.flagging)?;
                    self.insert(Store::Leg, &e);
                }
                continue;
            };
            let base_marked = u.implied || pleg.flagging.as_deref() == Some("all");
            let same_flagging = base_marked == u.leg.marked_route;
            let same_bends = same_points(&pleg.bends, &u.leg.bends);
            let same_gaps = same_ranges(&pleg.gaps, &u.leg.gaps);
            if same_flagging && same_bends && same_gaps {
                continue;
            }
            let id = u32::try_from(v.object(obj).id).unwrap_or(0);
            let full = leg_element(id, a, b, u.leg, u.flagging)?;
            let mut m = Merge::object(v.text, v.object(obj));
            for name in ["id", "start-control", "end-control"] {
                m.attr(name, true, None);
            }
            let mut kids = Kids::of(full);
            m.kids(&["flagging"], same_flagging, kids.take("flagging"));
            m.kids(&["bends"], same_bends, kids.take("bends"));
            m.kids(&["gaps"], same_gaps, kids.take("gaps"));
            let merged = m.finish(Store::Leg.element(), 1, self.style);
            self.replace(obj, merged);
        }
        for &(obj, (a, b)) in &v.leg_pairs {
            let gone = |id: i64| {
                v.control_by_id
                    .get(&id)
                    .is_some_and(|bi| self.deleted_controls.contains(bi))
            };
            if gone(a) || gone(b) {
                self.edits.insert(obj, Edit::Drop);
            }
        }
        Ok(())
    }

    fn specials(&mut self) -> Result<(), PpenWriteError> {
        let v = self.v;
        let deleted_course_ids: HashSet<i64> = self
            .deleted_courses
            .iter()
            .map(|&bi| v.st.courses[bi].id)
            .collect();
        for si in 0..self.ev.specials.len() {
            let s = &self.ev.specials[si];
            let id = self.plan.ids.specials[si];
            let courses = &self.plan.special_courses[si];
            let Some(bi) = self.m.specials[si] else {
                let e = special_element(s, id, courses)?;
                self.insert(Store::Special, &e);
                continue;
            };
            let ps = &v.st.specials[bi];
            let Some(ri) = v.read_specials[bi] else {
                return Err(mismatch());
            };
            let (kind, ..) = geometry(s);
            let same_kind = ps.kind == kind;
            let same_points = geometry(s) == geometry(&v.reader.specials[ri]);
            let base_courses = (!ps.all).then(|| {
                ps.courses
                    .iter()
                    .copied()
                    .filter(|c| v.course_by_id.contains_key(c))
                    .collect::<Vec<_>>()
            });
            let same_courses = base_courses
                == courses
                    .as_ref()
                    .map(|c| c.iter().map(|&i| i64::from(i)).collect());
            if same_kind && same_points && same_courses {
                continue;
            }
            let obj = v.special_objs[bi];
            let mut m = Merge::object(v.text, v.object(obj));
            m.attr("id", true, None);
            if same_kind && same_points {
                m.attr("kind", true, None);
                m.kids(&["location"], true, Vec::new());
            } else {
                let full = special_element(s, id, courses)?;
                m.attr("kind", same_kind, attr_of(&full, "kind"));
                m.kids(&["location"], false, Kids::of(full).take("location"));
            }
            m.kids(&["courses"], same_courses, vec![courses_element(courses)]);
            let merged = m.finish(Store::Special.element(), 1, self.style);
            self.replace(obj, merged);
        }
        // Kept special objects leave the course lists of deleted courses (`ChangeEvent.DeleteCourse`).
        for (bi, ps) in v.st.specials.iter().enumerate() {
            let obj = v.special_objs[bi];
            if self.deleted_specials.contains(&bi) {
                self.edits.insert(obj, Edit::Drop);
                continue;
            }
            if self.matched_specials.contains(&bi)
                || ps.all
                || !ps.courses.iter().any(|c| deleted_course_ids.contains(c))
            {
                continue;
            }
            if ps.courses.iter().all(|c| deleted_course_ids.contains(c)) {
                self.deleted_specials.insert(bi);
                self.edits.insert(obj, Edit::Drop);
                continue;
            }
            let mut m = Merge::object(v.text, v.object(obj));
            if let Some(i) = m.find_kid("courses") {
                let mut list = Merge::sub(v.text, &m.base_kids[i].0)?;
                for k in 0..list.base_kids.len() {
                    let (span, name) = &list.base_kids[k];
                    if *name == "course"
                        && sub_int(v.text, span, "course")
                            .is_some_and(|c| deleted_course_ids.contains(&c))
                    {
                        list.drop_kid(k);
                    }
                }
                if let Some(t) = list.finish("courses", 2, self.style) {
                    m.replace_kid(i, t);
                }
            }
            let merged = m.finish(Store::Special.element(), 1, self.style);
            self.replace(obj, merged);
        }
        Ok(())
    }

    /// The base objects the event does not mention and the patch does not delete.
    fn kept(&self) -> Vec<KeptObject> {
        let v = self.v;
        let id = |i: i64| u32::try_from(i).unwrap_or(0);
        let mut kept = Vec::new();
        for (bi, pc) in v.st.controls.iter().enumerate() {
            if !self.matched_controls.contains(&bi) && !self.deleted_controls.contains(&bi) {
                kept.push(KeptObject::Control {
                    id: id(pc.id),
                    code: v.reader.controls[bi].code.clone(),
                });
            }
        }
        for (bi, pc) in v.st.courses.iter().enumerate() {
            if !self.matched_courses.contains(&bi) && !self.deleted_courses.contains(&bi) {
                kept.push(KeptObject::Course {
                    id: id(pc.id),
                    name: pc.name.clone(),
                });
            }
        }
        for (bi, ps) in v.st.specials.iter().enumerate() {
            if !self.matched_specials.contains(&bi) && !self.deleted_specials.contains(&bi) {
                kept.push(KeptObject::Special {
                    id: id(v.object(v.special_objs[bi]).id),
                    kind: ps.kind.clone(),
                });
            }
        }
        kept
    }

    /// The base text with the edits applied and the new objects after the last base object of their store.
    fn assemble(self, kept: Vec<KeptObject>) -> Result<Written, PpenWriteError> {
        let v = self.v;
        let text = v.text;
        let mut last: BTreeMap<Store, usize> = BTreeMap::new();
        for o in &v.base.objects {
            last.insert(o.kind, o.span.end);
        }
        let stores = [
            Store::Control,
            Store::Course,
            Store::CourseControl,
            Store::Leg,
            Store::Special,
        ];
        let anchor = |s: Store| {
            stores
                .iter()
                .rev()
                .filter(|t| **t <= s)
                .find_map(|t| last.get(t).copied())
                .unwrap_or(v.base.event.end)
        };
        let mut splices: Vec<(usize, usize, String)> = Vec::new();
        for (obj, e) in self.edits {
            let span = &v.object(obj).span;
            let t = match e {
                Edit::Drop => String::new(),
                Edit::Replace(t) => t,
            };
            splices.push((span.start, span.end, t));
        }
        let inserting = !self.inserts.is_empty();
        for (store, texts) in self.inserts {
            let at = anchor(store);
            for t in texts {
                splices.push((at, at, t));
            }
        }
        if inserting && v.base.event.is_empty() && text[..v.base.event.start].ends_with("/>") {
            return Err(PpenWriteError::Unsupported(
                "the base file has an empty root element; write the event from scratch".into(),
            ));
        }
        splices.sort_by_key(|(s, e, _)| (*s, *e));
        let mut out = String::with_capacity(text.len());
        let mut pos = 0;
        for (s, e, t) in splices {
            out.push_str(&text[pos..s]);
            out.push_str(&t);
            pos = e;
        }
        out.push_str(&text[pos..]);
        if out.len() > MAX_COURSE_FILE_BYTES {
            return Err(PpenWriteError::Limit(format!(
                "the file would be larger than {} MiB",
                MAX_COURSE_FILE_BYTES / (1024 * 1024)
            )));
        }
        Ok(Written {
            text: out,
            ids: self.plan.ids,
            kept,
            notes: self.plan.notes,
        })
    }
}
