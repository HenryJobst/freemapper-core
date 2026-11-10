//! Writer for Purple Pen's native event files (`.ppen`), the inverse of [`read_ppen`](super::read_ppen).
//!
//! The output follows Purple Pen's own writer (`EventDB.Save` and the `WriteAttributesAndContent` methods in
//! `src/PurplePenCore/EventDB.cs` at the commit the reader names): .NET's `XmlTextWriter` with
//! `Formatting.Indented`. That means one element per line, two spaces of indent per level, attributes as
//! `name="value"` separated by one space, an empty element as `<x a="1" />`, an element with only text on one
//! line, and no XML declaration. Purple Pen 3.x on Windows writes a UTF-8 byte order mark and CRLF line ends.
//! Every number is written as an `f32` in its shortest round-trip form without an exponent, since Purple Pen
//! stores `float`.
//!
//! [`write_ppen`] writes an event from scratch. It takes the event in file frame (mm, y up) and never flips.
//! The stores come in `EventDB.Save`'s order (`event`, controls, courses, course-controls, legs, special
//! objects), each in event order. Purple Pen keeps one leg per ordered pair of controls and circle gaps per
//! control and scale, so two courses that draw them differently are refused. The event is checked against the
//! reader's limits before anything is built, and content the file cannot hold is refused instead of dropped.
//!
//! A patch of an existing file keeps every byte the writer does not change. The base parse (`base.rs`) splits
//! the file into its top-level objects and the bytes around them, takes the output style from the file, and
//! refuses a file from a Purple Pen version the writer does not know. Purple Pen writes no version number, so
//! the gate checks the shape of the file: root `course-scribe-event` without attributes, at most one
//! `event`, top-level children only `event`, `control`, `course`, `course-control`, `leg` and
//! `special-object`, UTF-8. Unknown content below the top level passes, because Purple Pen skips it on load
//! and the patch carries it.
//!
//! [`write_ppen_patch`] (`patch.rs`) writes an event into such a base file. It reads the base with the reader
//! too and compares every field it owns with the reader's value as `f32`. A field that did not change keeps
//! its bytes; a changed one comes from the same element builders as a file from scratch. The `<event>`
//! element stays as it is, and so does every object the event does not mention.

mod base;
mod out;
mod patch;

pub use patch::{Deletions, PatchOptions, write_ppen_patch};

use std::collections::{BTreeMap, HashMap, HashSet};

use out::{Body, Child, Element, Style, num, text_ok, write_element};

use super::{ROOT, gap_key};
use crate::CourseError;
use crate::model::*;

/// Why an event could not be written as a `.ppen`.
#[derive(Debug, thiserror::Error)]
pub enum PpenWriteError {
    /// The base file of a patch is unreadable or over a reader limit.
    #[error(transparent)]
    Base(#[from] CourseError),
    #[error("the .ppen comes from a Purple Pen version this writer does not know: {0}")]
    UnknownVersion(String),
    #[error("cannot be written to a .ppen: {0}")]
    Unsupported(String),
    #[error("invalid event: {0}")]
    Invalid(String),
    #[error(
        "courses {first} and {second} draw the leg {from} to {to} differently; Purple Pen keeps one leg per pair of controls"
    )]
    SharedLeg {
        from: String,
        to: String,
        first: String,
        second: String,
    },
    #[error(
        "courses {first} and {second} give control {control} different circle gaps at 1:{scale}"
    )]
    SharedCircleGaps {
        control: String,
        scale: i64,
        first: String,
        second: String,
    },
    #[error("too large for a .ppen: {0}")]
    Limit(String),
}

/// The line end of a written file. Purple Pen on Windows writes CRLF.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineEnding {
    #[default]
    Crlf,
    Lf,
}

impl LineEnding {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            LineEnding::Crlf => "\r\n",
            LineEnding::Lf => "\n",
        }
    }
}

/// What a file written from scratch states about the event beyond the courses: the map, the standards and the
/// language of the descriptions.
#[derive(Debug, Clone, PartialEq)]
pub struct ScratchOptions {
    pub map: PpenMap,
    pub map_standard: MapStandard,
    /// The standard of every control description; each `Description::standard` must equal it.
    pub description_standard: DescriptionStandard,
    /// The language of the description texts, such as "de" or "en".
    pub description_lang: String,
    pub line_ending: LineEnding,
}

/// The map file an event refers to (`event/map`), with its path as Purple Pen stores it.
#[derive(Debug, Clone, PartialEq)]
pub enum PpenMap {
    None,
    Ocad { path: String },
    Bitmap { path: String, dpi: f64 },
    Pdf { path: String },
}

/// The map standard (`standards/@map`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapStandard {
    Isom2000,
    Isom2017,
    Issprom2019,
}

impl MapStandard {
    /// The value Purple Pen writes.
    pub fn as_str(self) -> &'static str {
        match self {
            MapStandard::Isom2000 => "2000",
            MapStandard::Isom2017 => "2017",
            MapStandard::Issprom2019 => "Spr2019",
        }
    }
}

/// The control description standard (`standards/@description`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DescriptionStandard {
    Iof2004,
    Iof2018,
}

impl DescriptionStandard {
    /// The value Purple Pen writes, which is also the reader's `Description::standard`.
    pub fn as_str(self) -> &'static str {
        match self {
            DescriptionStandard::Iof2004 => "2004",
            DescriptionStandard::Iof2018 => "2018",
        }
    }
}

/// A written `.ppen`.
#[derive(Debug, Clone, PartialEq)]
pub struct Written {
    /// The file, byte order mark included.
    pub text: String,
    /// The id each object of the event got.
    pub ids: AssignedIds,
    /// Objects of a patch's base file that the event did not mention and that were kept; empty from scratch.
    pub kept: Vec<KeptObject>,
    pub notes: Vec<WriteNote>,
}

/// The Purple Pen ids of the written objects, parallel to the event's `controls`, `courses` and `specials`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AssignedIds {
    pub controls: Vec<u32>,
    pub courses: Vec<u32>,
    pub specials: Vec<u32>,
}

/// An object of a patch's base file that the event did not mention, kept as it was.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeptObject {
    Control { id: u32, code: String },
    Course { id: u32, name: String },
    Special { id: u32, kind: String },
}

/// Something written differently from what the event says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriteNote {
    /// The leg is not a marked route in the event, but Purple Pen flags it on its own: it leads into a map
    /// exchange or into a finish whose symbol is 14.1.
    FlaggingImplied {
        course: String,
        from: String,
        to: String,
    },
}

/// Purple Pen keeps ids as `int`, and 0 means "none".
const MAX_ID: u32 = i32::MAX as u32;

/// Writes `event` as a new `.ppen` file.
pub fn write_ppen(event: &RawEvent, options: &ScratchOptions) -> Result<Written, PpenWriteError> {
    check_limits(event)?;
    let map_scale = event
        .source
        .map_scale
        .filter(|s| *s > 0)
        .ok_or_else(|| PpenWriteError::Invalid("the event has no map scale".into()))?;
    let ids = AssignedIds {
        controls: assign_ids(event.controls.iter().map(|c| c.id), "control")?,
        courses: assign_ids(event.courses.iter().map(|c| c.id), "course")?,
        specials: assign_ids(event.specials.iter().map(Special::id), "special object")?,
    };
    let plan = Plan::new(
        event,
        Frame {
            ids,
            map_scale: Some(f64::from(map_scale)),
            item_scaling: ItemScaling::None,
            map_issue: vec![false; event.controls.len()],
            other_courses: Vec::new(),
        },
    )?;
    let style = Style::purple_pen(options.line_ending);

    let mut children = vec![Child::Element(event_element(event, options, map_scale)?)];
    for (i, c) in event.controls.iter().enumerate() {
        children.push(Child::Element(control_element(
            c,
            plan.ids.controls[i],
            &plan.circle_gaps[i],
            options.description_standard.as_str(),
        )?));
    }
    for (i, c) in event.courses.iter().enumerate() {
        let first = plan.cc_ids[i].first().copied();
        children.push(Child::Element(course_element(
            c,
            plan.ids.courses[i],
            i + 1,
            first,
        )?));
    }
    for (ci, course) in event.courses.iter().enumerate() {
        let ids = &plan.cc_ids[ci];
        for (si, stop) in course.stops.iter().enumerate() {
            let control = plan.stop_controls[ci][si];
            children.push(Child::Element(course_control_element(
                ids[si],
                plan.ids.controls[control],
                stop,
                exchange_at_normal(stop, &event.controls[control]),
                ids.get(si + 1).copied(),
            )?));
        }
    }
    let mut next_leg = 0;
    for leg in &plan.legs {
        if !leg.written() {
            continue;
        }
        next_leg += 1;
        children.push(Child::Element(leg_element(
            next_leg,
            plan.ids.controls[leg.from],
            plan.ids.controls[leg.to],
            leg.leg,
            leg.flagging,
        )?));
    }
    for (i, s) in event.specials.iter().enumerate() {
        children.push(Child::Element(special_element(
            s,
            plan.ids.specials[i],
            &plan.special_courses[i],
        )?));
    }

    let root = Element {
        name: ROOT,
        attrs: Vec::new(),
        body: Body::Children(children),
    };
    let mut text = String::new();
    if style.bom {
        text.push('\u{FEFF}');
    }
    write_element(&mut text, &root, 0, &style);
    // XmlTextWriter ends the file with the root's end tag.
    let newline = style.newline.as_str();
    if text.ends_with(newline) {
        text.truncate(text.len() - newline.len());
    }
    if text.len() > MAX_COURSE_FILE_BYTES {
        return Err(PpenWriteError::Limit(format!(
            "the file would be larger than {} MiB",
            MAX_COURSE_FILE_BYTES / (1024 * 1024)
        )));
    }
    Ok(Written {
        text,
        ids: plan.ids,
        kept: Vec::new(),
        notes: plan.notes,
    })
}

/// The reader's limits, checked on the event before any work, so that the writer never builds a file its
/// own reader refuses. Points and text are counted as the reader counts them, once per course.
fn check_limits(ev: &RawEvent) -> Result<(), PpenWriteError> {
    let limit = |m: String| Err(PpenWriteError::Limit(m));
    if ev.controls.len() > MAX_CONTROLS {
        return limit(format!("more than {MAX_CONTROLS} controls"));
    }
    if ev.courses.len() > MAX_COURSES {
        return limit(format!("more than {MAX_COURSES} courses"));
    }
    if ev.specials.len() > MAX_SPECIALS {
        return limit(format!("more than {MAX_SPECIALS} special objects"));
    }
    let path = |n: usize| {
        if n > MAX_POINTS_PER_PATH {
            limit(format!(
                "more than {MAX_POINTS_PER_PATH} points in one path"
            ))
        } else {
            Ok(())
        }
    };
    let code = |c: &str| {
        if c.chars().count() > MAX_CODE_LEN {
            limit(format!(
                "control code longer than {MAX_CODE_LEN} characters"
            ))
        } else {
            Ok(())
        }
    };
    let name = |n: &str, what: &str| {
        if n.chars().count() > MAX_NAME_LEN {
            limit(format!("{what} longer than {MAX_NAME_LEN} characters"))
        } else {
            Ok(())
        }
    };
    let mut points = 0usize;
    let mut text = 0usize;
    for c in &ev.controls {
        code(&c.code)?;
        text = text.saturating_add(c.code.len());
        if let Some(d) = &c.description {
            text = text.saturating_add(d.standard.len());
        }
    }
    for c in &ev.courses {
        name(&c.name, "course name")?;
        text = text.saturating_add(c.name.len());
        if let Some(t) = &c.secondary_title {
            name(t, "secondary title")?;
            text = text.saturating_add(t.len());
        }
        if c.stops.len() > MAX_STOPS_PER_COURSE {
            return limit(format!(
                "more than {MAX_STOPS_PER_COURSE} stops in a course"
            ));
        }
        for s in &c.stops {
            code(&s.control)?;
            text = text.saturating_add(s.control.len());
            path(s.circle_gaps.len())?;
            points = points.saturating_add(s.circle_gaps.len());
        }
        for l in &c.legs {
            path(l.bends.len())?;
            path(l.gaps.len())?;
            points = points.saturating_add(l.bends.len() + l.gaps.len());
        }
    }
    for s in &ev.specials {
        let n = special_points(s);
        path(n)?;
        points = points.saturating_add(n);
        if let CourseFilter::Only(names) = s.courses() {
            if names.len() > MAX_COURSES {
                return limit(format!(
                    "more than {MAX_COURSES} courses for one special object"
                ));
            }
            for n in names {
                text = text.saturating_add(n.len());
            }
        }
    }
    if points > MAX_TOTAL_POINTS {
        return limit(format!(
            "more than {MAX_TOTAL_POINTS} points, gaps and special points in all courses together"
        ));
    }
    if text > MAX_TOTAL_TEXT_BYTES {
        return limit(format!(
            "more than {} MiB of codes, names and description standards",
            MAX_TOTAL_TEXT_BYTES / (1024 * 1024)
        ));
    }
    Ok(())
}

/// The points of a special object, all rings together.
fn special_points(s: &Special<RawPoint>) -> usize {
    match s {
        Special::OutOfBounds { rings, .. }
        | Special::Dangerous { rings, .. }
        | Special::WhiteOut { rings, .. } => rings.iter().map(Vec::len).sum(),
        Special::ForbiddenRoute { path, .. }
        | Special::Boundary { path, .. }
        | Special::MarkedRoute { path, .. }
        | Special::Line { path, .. } => path.len(),
        Special::FirstAid { .. } | Special::Water { .. } | Special::Text { .. } => 1,
    }
}

/// One use of a leg: the ordered pair of controls (indices into the event's controls) and what is drawn.
struct LegUse<'a> {
    from: usize,
    to: usize,
    leg: &'a Leg<RawPoint>,
    /// Purple Pen flags the leg on its own (`QueryEvent.GetLegFlagging`).
    implied: bool,
    /// `flagging kind="all"` is written: a marked route that Purple Pen does not flag on its own.
    flagging: bool,
    course: &'a str,
}

impl LegUse<'_> {
    /// A leg is written when there is something to write.
    fn written(&self) -> bool {
        !self.leg.bends.is_empty() || !self.leg.gaps.is_empty() || self.flagging
    }

    /// Both uses give the same leg element.
    fn same(&self, other: &LegUse<'_>) -> bool {
        self.flagging == other.flagging
            && same_points(&self.leg.bends, &other.leg.bends)
            && same_ranges(&self.leg.gaps, &other.leg.gaps)
    }
}

static NO_LEG: Leg<RawPoint> = Leg {
    bends: Vec::new(),
    gaps: Vec::new(),
    marked_route: false,
};

fn same_points(a: &[RawPoint], b: &[RawPoint]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|(p, q)| p.x_mm as f32 == q.x_mm as f32 && p.y_mm as f32 == q.y_mm as f32)
}

fn same_ranges(a: &[(f64, f64)], b: &[(f64, f64)]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|(p, q)| p.0 as f32 == q.0 as f32 && p.1 as f32 == q.1 as f32)
}

/// The circle gaps of one control by circle-gap scale, with the name of the course that set them first.
type ControlGaps<'a> = BTreeMap<i64, (&'a [(f64, f64)], &'a str)>;

/// What the plan of an event needs beyond the event: the ids of its objects and what the file around it
/// says. From scratch that is the event alone; a patch takes it from the base file.
struct Frame<'a> {
    ids: AssignedIds,
    /// The map scale and item scaling the circle gaps are keyed by.
    map_scale: Option<f64>,
    item_scaling: ItemScaling,
    /// Per control of the event: it is a map issue point in the file (the model reads those as starts).
    map_issue: Vec<bool>,
    /// Courses of the file that are not in the event, by name and id, for the course lists of specials.
    other_courses: Vec<(&'a str, u32)>,
}

/// Everything about the event that the elements need and that takes more than one object to decide: ids,
/// the circle gaps per control and scale, the legs, and the course ids of the special objects.
struct Plan<'a> {
    ids: AssignedIds,
    /// Per course, the ids of its course-controls in stop order.
    cc_ids: Vec<Vec<u32>>,
    /// Per course and stop, the index of the control.
    stop_controls: Vec<Vec<usize>>,
    /// Per control, its circle gaps by scale and the course that set them first.
    circle_gaps: Vec<ControlGaps<'a>>,
    /// Every use of a pair of controls once, in first-use order.
    legs: Vec<LegUse<'a>>,
    special_courses: Vec<Option<Vec<u32>>>,
    notes: Vec<WriteNote>,
}

impl<'a> Plan<'a> {
    fn new(ev: &'a RawEvent, frame: Frame<'a>) -> Result<Plan<'a>, PpenWriteError> {
        let Frame {
            ids,
            map_scale,
            item_scaling,
            map_issue,
            other_courses,
        } = frame;
        let mut by_code: HashMap<&str, usize> = HashMap::with_capacity(ev.controls.len());
        for (i, c) in ev.controls.iter().enumerate() {
            if by_code.insert(c.code.as_str(), i).is_some() {
                return Err(PpenWriteError::Invalid(format!(
                    "control code {} is used twice",
                    c.code
                )));
            }
        }

        let mut plan = Plan {
            ids,
            cc_ids: Vec::with_capacity(ev.courses.len()),
            stop_controls: Vec::with_capacity(ev.courses.len()),
            circle_gaps: vec![BTreeMap::new(); ev.controls.len()],
            legs: Vec::new(),
            special_courses: Vec::with_capacity(ev.specials.len()),
            notes: Vec::new(),
        };
        let mut leg_index: HashMap<(usize, usize), usize> = HashMap::new();
        let mut next_cc = 0u32;
        for course in &ev.courses {
            if course.family.is_some() {
                return Err(PpenWriteError::Unsupported(format!(
                    "course {} belongs to a course family; Purple Pen has course families only as variations",
                    course.name
                )));
            }
            if course.legs.len() > course.stops.len().saturating_sub(1) {
                return Err(PpenWriteError::Invalid(format!(
                    "course {} has more legs than stops allow",
                    course.name
                )));
            }
            let key = gap_key(
                item_scaling,
                map_scale,
                course.print_scale_hint.map(f64::from),
            );
            let mut controls = Vec::with_capacity(course.stops.len());
            for stop in &course.stops {
                let &ci = by_code.get(stop.control.as_str()).ok_or_else(|| {
                    PpenWriteError::Invalid(format!(
                        "course {} visits control {}, which the event does not have",
                        course.name, stop.control
                    ))
                })?;
                check_stop(course, stop, &ev.controls[ci])?;
                match plan.circle_gaps[ci].get(&key) {
                    Some((gaps, first)) if !same_ranges(gaps, &stop.circle_gaps) => {
                        return Err(PpenWriteError::SharedCircleGaps {
                            control: stop.control.clone(),
                            scale: key,
                            first: first.to_string(),
                            second: course.name.clone(),
                        });
                    }
                    Some(_) => {}
                    None => {
                        plan.circle_gaps[ci].insert(key, (&stop.circle_gaps, &course.name));
                    }
                }
                controls.push(ci);
            }
            for (i, w) in controls.windows(2).enumerate() {
                let (from, to) = (w[0], w[1]);
                let leg = course.legs.get(i).unwrap_or(&NO_LEG);
                let target = &ev.controls[to];
                // Purple Pen flags these legs on its own (`QueryEvent.GetLegFlagging`).
                let implied = map_issue[from]
                    || target.kind == ControlKind::MapExchange
                    || (target.kind == ControlKind::Finish
                        && target
                            .description
                            .as_ref()
                            .and_then(|d| d.columns[0].as_deref())
                            == Some("14.1"));
                if implied && !leg.marked_route {
                    plan.notes.push(WriteNote::FlaggingImplied {
                        course: course.name.clone(),
                        from: ev.controls[from].code.clone(),
                        to: target.code.clone(),
                    });
                }
                let use_ = LegUse {
                    from,
                    to,
                    leg,
                    implied,
                    flagging: leg.marked_route && !implied,
                    course: &course.name,
                };
                match leg_index.get(&(from, to)) {
                    Some(&li) => {
                        let first = &plan.legs[li];
                        if !first.same(&use_) {
                            return Err(PpenWriteError::SharedLeg {
                                from: ev.controls[from].code.clone(),
                                to: target.code.clone(),
                                first: first.course.to_string(),
                                second: course.name.clone(),
                            });
                        }
                    }
                    None => {
                        leg_index.insert((from, to), plan.legs.len());
                        plan.legs.push(use_);
                    }
                }
            }
            let mut ccs = Vec::with_capacity(course.stops.len());
            for _ in &course.stops {
                next_cc += 1;
                ccs.push(next_cc);
            }
            plan.cc_ids.push(ccs);
            plan.stop_controls.push(controls);
        }

        // Course names to ids; a name of two courses is ambiguous.
        let mut by_name: HashMap<&str, Option<u32>> = HashMap::with_capacity(ev.courses.len());
        let named = ev
            .courses
            .iter()
            .map(|c| c.name.as_str())
            .zip(plan.ids.courses.iter().copied())
            .chain(other_courses);
        for (name, id) in named {
            by_name
                .entry(name)
                .and_modify(|v| *v = None)
                .or_insert(Some(id));
        }
        for s in &ev.specials {
            let list = match s.courses() {
                CourseFilter::All => None,
                CourseFilter::Only(names) => Some(
                    names
                        .iter()
                        .map(|n| match by_name.get(n.as_str()) {
                            Some(Some(id)) => Ok(*id),
                            Some(None) => Err(PpenWriteError::Invalid(format!(
                                "a special object names course {n}, and two courses have that name"
                            ))),
                            None => Err(PpenWriteError::Invalid(format!(
                                "a special object names course {n}, which the event does not have"
                            ))),
                        })
                        .collect::<Result<Vec<_>, _>>()?,
                ),
            };
            plan.special_courses.push(list);
        }
        Ok(plan)
    }
}

/// An object keeps its id when it has one that is valid and unique in its store; any other gets the largest
/// id of the store plus one, counting up in event order.
fn assign_ids(
    given: impl Iterator<Item = Option<u32>> + Clone,
    what: &str,
) -> Result<Vec<u32>, PpenWriteError> {
    let valid = |id: Option<u32>| id.filter(|i| (1..=MAX_ID).contains(i));
    let mut next = given.clone().filter_map(valid).max().unwrap_or(0);
    let mut used = HashSet::new();
    given
        .map(|id| match valid(id) {
            Some(i) if used.insert(i) => Ok(i),
            _ => {
                if next >= MAX_ID {
                    return Err(PpenWriteError::Limit(format!(
                        "no {what} id left below {MAX_ID}"
                    )));
                }
                next += 1;
                used.insert(next);
                Ok(next)
            }
        })
        .collect()
}

/// Refuses what a course-control cannot hold.
fn check_stop(
    course: &Course<RawPoint>,
    stop: &Stop,
    control: &Control<RawPoint>,
) -> Result<(), PpenWriteError> {
    let at = || format!("course {}, control {}", course.name, stop.control);
    if stop.number.is_some() {
        return Err(PpenWriteError::Unsupported(format!(
            "{}: a number placement; convert it with offset_from_placement first",
            at()
        )));
    }
    match stop.kind {
        None => {}
        Some(k) if k == control.kind => {}
        Some(ControlKind::MapExchange) if control.kind == ControlKind::Control => {}
        Some(k) => {
            return Err(PpenWriteError::Unsupported(format!(
                "{}: the stop is a {k:?} only in this course; Purple Pen allows that only for a map exchange \
                 at a normal control",
                at()
            )));
        }
    }
    if stop.map_flip && !exchange_at_normal(stop, control) {
        return Err(PpenWriteError::Unsupported(format!(
            "{}: a map flip without a map exchange at a normal control",
            at()
        )));
    }
    Ok(())
}

fn exchange_at_normal(stop: &Stop, control: &Control<RawPoint>) -> bool {
    stop.kind == Some(ControlKind::MapExchange) && control.kind == ControlKind::Control
}

fn el(name: &'static str, attrs: Vec<(&'static str, String)>, body: Body) -> Element {
    Element {
        name,
        attrs: attrs.into_iter().map(|(k, v)| (k.to_string(), v)).collect(),
        body,
    }
}

fn text_el(name: &'static str, text: &str, what: &str) -> Result<Element, PpenWriteError> {
    text_ok(text, what)?;
    Ok(el(name, Vec::new(), Body::Text(text.to_string())))
}

fn point_el(name: &'static str, p: &RawPoint, what: &str) -> Result<Element, PpenWriteError> {
    Ok(el(
        name,
        vec![
            ("x", num(p.x_mm, &format!("{what} x"))?),
            ("y", num(p.y_mm, &format!("{what} y"))?),
        ],
        Body::Empty,
    ))
}

fn children(elements: Vec<Element>) -> Body {
    Body::Children(elements.into_iter().map(Child::Element).collect())
}

/// `<event id="1">`: title, map, standards, all-controls, course-appearance (no item scaling), descriptions.
fn event_element(
    ev: &RawEvent,
    options: &ScratchOptions,
    map_scale: u32,
) -> Result<Element, PpenWriteError> {
    let mut kids = Vec::new();
    if let Some(t) = &ev.source.event_name {
        kids.push(text_el("title", t, "event title")?);
    }
    let scale = map_scale.to_string();
    let mut attrs = vec![("kind", String::new()), ("scale", scale.clone())];
    let (kind, path) = match &options.map {
        PpenMap::None => ("none", None),
        PpenMap::Ocad { path } => {
            attrs.push(("ignore-missing-fonts", "false".into()));
            ("OCAD", Some(path))
        }
        PpenMap::Bitmap { path, dpi } => {
            attrs.push(("dpi", num(*dpi, "map dpi")?));
            ("bitmap", Some(path))
        }
        PpenMap::Pdf { path } => ("PDF", Some(path)),
    };
    attrs[0].1 = kind.into();
    let body = match path {
        Some(p) => {
            text_ok(p, "map path")?;
            Body::Text(p.clone())
        }
        None => Body::Empty,
    };
    kids.push(el("map", attrs, body));
    kids.push(el(
        "standards",
        vec![
            ("map", options.map_standard.as_str().into()),
            ("description", options.description_standard.as_str().into()),
        ],
        Body::Empty,
    ));
    kids.push(el(
        "all-controls",
        vec![
            ("print-scale", scale),
            ("description-kind", "symbols".into()),
        ],
        Body::Empty,
    ));
    kids.push(el("course-appearance", Vec::new(), Body::Empty));
    text_ok(&options.description_lang, "description language")?;
    kids.push(el(
        "descriptions",
        vec![("lang", options.description_lang.clone())],
        Body::Empty,
    ));
    Ok(el("event", vec![("id", "1".into())], children(kids)))
}

/// `<control>`: id and kind; code (normal controls only), location, descriptions and the circle gaps per
/// scale (`CircleGap.EncodeGaps`).
fn control_element(
    c: &Control<RawPoint>,
    id: u32,
    gaps: &ControlGaps<'_>,
    standard: &str,
) -> Result<Element, PpenWriteError> {
    let what = format!("control {}", c.code);
    let kind = match c.kind {
        ControlKind::Start => "start",
        ControlKind::Control => "normal",
        ControlKind::Finish => "finish",
        ControlKind::CrossingPoint => "crossing-point",
        ControlKind::MapExchange => "map-exchange",
        ControlKind::EndOfMarkedRoute => {
            return Err(PpenWriteError::Unsupported(format!(
                "{what} is the end of a marked route, which Purple Pen does not have"
            )));
        }
    };
    let mut kids = Vec::new();
    if c.kind == ControlKind::Control {
        if c.code.is_empty() {
            return Err(PpenWriteError::Invalid(
                "a normal control has no code".into(),
            ));
        }
        kids.push(text_el("code", &c.code, &what)?);
    }
    let p = c
        .position
        .as_ref()
        .ok_or_else(|| PpenWriteError::Invalid(format!("{what} has no location")))?;
    kids.push(point_el("location", p, &what)?);
    if let Some(d) = &c.description {
        kids.extend(description_elements(c, d, standard, &what)?);
    }
    for (scale, (g, _)) in gaps {
        if !g.is_empty() {
            kids.push(circle_gaps_element(*scale, g, &what)?);
        }
    }
    Ok(el(
        "control",
        vec![("id", id.to_string()), ("kind", kind.into())],
        children(kids),
    ))
}

/// `<circle-gaps scale=K>start:stop,…</circle-gaps>` (`CircleGap.EncodeGaps`).
fn circle_gaps_element(
    scale: i64,
    gaps: &[(f64, f64)],
    what: &str,
) -> Result<Element, PpenWriteError> {
    let pairs = gaps
        .iter()
        .map(|(a, b)| {
            Ok(format!(
                "{}:{}",
                num(*a, &format!("{what} circle gap"))?,
                num(*b, &format!("{what} circle gap"))?
            ))
        })
        .collect::<Result<Vec<_>, PpenWriteError>>()?;
    Ok(el(
        "circle-gaps",
        vec![("scale", scale.to_string())],
        Body::Text(pairs.join(",")),
    ))
}

/// The `description` elements of a control: box `all` with the one symbol of a finish or crossing point,
/// else one per column C to H with a symbol, and the column F text as the text of box F.
fn description_elements(
    c: &Control<RawPoint>,
    d: &Description,
    standard: &str,
    what: &str,
) -> Result<Vec<Element>, PpenWriteError> {
    if d.standard != standard {
        return Err(PpenWriteError::Invalid(format!(
            "{what} has descriptions in the standard {}, the file states {standard}",
            d.standard
        )));
    }
    let symbol = |s: &Option<String>| -> Result<Vec<(&'static str, String)>, PpenWriteError> {
        match s {
            Some(s) => {
                text_ok(s, what)?;
                Ok(vec![("iof-2004-ref", s.clone())])
            }
            None => Ok(Vec::new()),
        }
    };
    if matches!(c.kind, ControlKind::Finish | ControlKind::CrossingPoint) {
        if d.columns[1..].iter().any(Option::is_some) || d.column_f_text.is_some() {
            return Err(PpenWriteError::Unsupported(format!(
                "{what} is a finish or crossing point, which has one description symbol (column C) in Purple Pen"
            )));
        }
        let mut attrs = vec![("box", "all".to_string())];
        attrs.extend(symbol(&d.columns[0])?);
        return Ok(vec![el("description", attrs, Body::Empty)]);
    }
    let mut out = Vec::new();
    for (i, col) in d.columns.iter().enumerate() {
        let text = if i == 3 {
            d.column_f_text.as_ref()
        } else {
            None
        };
        if col.is_none() && text.is_none() {
            continue;
        }
        let mut attrs = vec![("box", char::from(b'C' + i as u8).to_string())];
        attrs.extend(symbol(col)?);
        let body = match text {
            Some(t) => {
                text_ok(t, what)?;
                Body::Text(t.clone())
            }
            None => Body::Empty,
        };
        out.push(el("description", attrs, body));
    }
    Ok(out)
}

/// `<course>`: id, kind and order; name, secondary title, first course-control and options.
fn course_element(
    c: &Course<RawPoint>,
    id: u32,
    order: usize,
    first: Option<u32>,
) -> Result<Element, PpenWriteError> {
    let what = format!("course {}", c.name);
    let kind = match c.kind {
        CourseKind::Normal => "normal",
        CourseKind::Score => "score",
    };
    let mut kids = vec![text_el("name", &c.name, &what)?];
    if let Some(t) = &c.secondary_title {
        kids.push(text_el("secondary-title", t, &what)?);
    }
    if let Some(f) = first {
        kids.push(el(
            "first",
            vec![("course-control", f.to_string())],
            Body::Empty,
        ));
    }
    let mut options = Vec::new();
    if let Some(s) = c.print_scale_hint {
        if s == 0 {
            return Err(PpenWriteError::Invalid(format!(
                "{what} has a print scale of 0"
            )));
        }
        options.push(("print-scale", s.to_string()));
    }
    if let Some(v) = c.climb_m {
        if v < 0.0 {
            return Err(PpenWriteError::Invalid(format!(
                "{what} has a negative climb"
            )));
        }
        options.push(("climb", num(v, &format!("{what} climb"))?));
    }
    if let Some(v) = c.length_m {
        if v <= 0.0 {
            return Err(PpenWriteError::Invalid(format!(
                "{what} has a length that is not positive"
            )));
        }
        options.push(("course-length", num(v, &format!("{what} length"))?));
    }
    if !options.is_empty() {
        kids.push(el("options", options, Body::Empty));
    }
    Ok(el(
        "course",
        vec![
            ("id", id.to_string()),
            ("kind", kind.into()),
            ("order", order.to_string()),
        ],
        children(kids),
    ))
}

/// `<course-control>`: id, control, map exchange and flip; next and number location. `exchange` says that the
/// stop is a map exchange at a normal control (`map-exchange="true"`); a map exchange control is one by its
/// kind.
fn course_control_element(
    id: u32,
    control: u32,
    stop: &Stop,
    exchange: bool,
    next: Option<u32>,
) -> Result<Element, PpenWriteError> {
    let mut attrs = vec![("id", id.to_string()), ("control", control.to_string())];
    if exchange {
        attrs.push(("map-exchange", "true".into()));
        if stop.map_flip {
            attrs.push(("map-flip", "true".into()));
        }
    }
    let mut kids = Vec::new();
    if let Some(n) = next {
        kids.push(el(
            "next",
            vec![("course-control", n.to_string())],
            Body::Empty,
        ));
    }
    if let Some(o) = stop.number_text_at {
        let what = format!("number location of control {}", stop.control);
        kids.push(el(
            "number-location",
            vec![
                ("x", num(o.dx_mm, &format!("{what} x"))?),
                ("y", num(o.dy_mm, &format!("{what} y"))?),
            ],
            Body::Empty,
        ));
    }
    Ok(el("course-control", attrs, children(kids)))
}

/// `<leg>`: id, start and end control; flagging, bends and gaps (start and length along the leg).
fn leg_element(
    id: u32,
    from: u32,
    to: u32,
    leg: &Leg<RawPoint>,
    flagging: bool,
) -> Result<Element, PpenWriteError> {
    let what = format!("leg from control id {from} to {to}");
    let mut kids = Vec::new();
    if flagging {
        kids.push(el("flagging", vec![("kind", "all".into())], Body::Empty));
    }
    if !leg.bends.is_empty() {
        let bends = leg
            .bends
            .iter()
            .map(|p| point_el("location", p, &format!("{what} bend")))
            .collect::<Result<Vec<_>, _>>()?;
        kids.push(el("bends", Vec::new(), children(bends)));
    }
    if !leg.gaps.is_empty() {
        let gaps = leg
            .gaps
            .iter()
            .map(|(a, b)| {
                Ok(el(
                    "gap",
                    vec![
                        ("start", num(*a, &format!("{what} gap start"))?),
                        ("length", num(b - a, &format!("{what} gap length"))?),
                    ],
                    Body::Empty,
                ))
            })
            .collect::<Result<Vec<_>, PpenWriteError>>()?;
        kids.push(el("gaps", Vec::new(), children(gaps)));
    }
    Ok(el(
        "leg",
        vec![
            ("id", id.to_string()),
            ("start-control", from.to_string()),
            ("end-control", to.to_string()),
        ],
        children(kids),
    ))
}

/// `<special-object>`: id and kind; the locations and the courses (`all`, or the ids in `courses`).
fn special_element(
    s: &Special<RawPoint>,
    id: u32,
    courses: &Option<Vec<u32>>,
) -> Result<Element, PpenWriteError> {
    let unsupported = |m: &str| Err(PpenWriteError::Unsupported(m.to_string()));
    fn one_ring<'r>(
        rings: &'r [Vec<RawPoint>],
        kind: &str,
    ) -> Result<&'r [RawPoint], PpenWriteError> {
        match rings {
            [ring] => Ok(ring.as_slice()),
            _ => Err(PpenWriteError::Unsupported(format!(
                "a {kind} special object with {} rings; Purple Pen keeps exactly one",
                rings.len()
            ))),
        }
    }
    let (kind, points): (&str, &[RawPoint]) = match s {
        Special::FirstAid { pt, .. } => ("first-aid", std::slice::from_ref(pt)),
        Special::Water { pt, .. } => ("water", std::slice::from_ref(pt)),
        Special::ForbiddenRoute { path, .. } => match path.as_slice() {
            [_] => ("forbidden-route", path.as_slice()),
            _ => {
                return unsupported(&format!(
                    "a forbidden route of {} points; Purple Pen marks one with a single cross",
                    path.len()
                ));
            }
        },
        Special::Boundary { path, .. } => ("boundary", path.as_slice()),
        Special::Line { path, .. } => ("line", path.as_slice()),
        Special::OutOfBounds { rings, .. } => ("out-of-bounds", one_ring(rings, "out-of-bounds")?),
        Special::Dangerous { rings, .. } => ("dangerous-area", one_ring(rings, "dangerous-area")?),
        Special::WhiteOut { rings, .. } => ("white-out", one_ring(rings, "white-out")?),
        Special::Text { .. } => {
            return unsupported("a text special object; the model keeps no text box width");
        }
        Special::MarkedRoute { .. } => {
            return unsupported("a marked route special object, which Purple Pen does not have");
        }
    };
    if points.is_empty() {
        return Err(PpenWriteError::Invalid(format!(
            "a {kind} special object without points"
        )));
    }
    let what = format!("special object {id}");
    let mut kids = points
        .iter()
        .map(|p| point_el("location", p, &what))
        .collect::<Result<Vec<_>, _>>()?;
    kids.push(courses_element(courses));
    Ok(el(
        "special-object",
        vec![("id", id.to_string()), ("kind", kind.into())],
        children(kids),
    ))
}

/// `<courses>` of a special object: `all`, or one `course` per id.
fn courses_element(courses: &Option<Vec<u32>>) -> Element {
    match courses {
        None => el("courses", vec![("all", "true".into())], Body::Empty),
        Some(ids) => el(
            "courses",
            Vec::new(),
            children(
                ids.iter()
                    .map(|i| el("course", vec![("course", i.to_string())], Body::Empty))
                    .collect(),
            ),
        ),
    }
}
