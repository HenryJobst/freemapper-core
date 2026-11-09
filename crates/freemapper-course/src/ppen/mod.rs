//! Reader for Purple Pen's native event files (`.ppen`).
//!
//! The format follows Purple Pen's own reader and writer: `src/PurplePenCore/EventDB.cs` (the
//! `ReadAttributesAndContent` methods of `Event`, `ControlPoint`, `Course`, `CourseControl`, `Leg` and
//! `Special`, and `EventDB.Load`), `CircleGap.cs`, `QueryEvent.cs` (`GetLegFlagging`, `GetControlGaps`),
//! `CourseView.cs` (`CircleGapScale`) and `ExportXml.cs` (codes for controls without one) on GitHub
//! `petergolde/PurplePen`, commit `201417f97bb69302e3e0db123f428870e0914515` (2026-11-07).
//!
//! The root `course-scribe-event` holds, in this order, one `event`, the `control`s, `course`s,
//! `course-control`s, `leg`s and `special-object`s; objects refer to each other by their `id`. The reader
//! accepts them in any order. All positions are map mm with y up; the reader keeps them as they are and the
//! fit decides the axis (ADR 0032).
//!
//! - Controls: `kind` start, normal, finish, crossing-point and map-exchange map to [`ControlKind`];
//!   map-issue reads as a start and is noted. Only normal controls have a `code`; the others get the codes
//!   Purple Pen's IOF XML export gives them, counted per kind in file order: `STA1`, `XCHG1`, `FIN1`,
//!   `CROSS1`, and `ISSUE1` for map issue points (the export leaves those out). `description box="C".."H"`
//!   with `iof-2004-ref` fills [`Description::columns`] (`box="all"`, the one symbol of a finish or crossing
//!   point, goes to column C); text in box F is kept as [`Description::column_f_text`]. The standard is the
//!   event's `standards/@description`, 2004 when missing.
//! - Ids: `control/@id` and `course/@id` become [`Control::id`] and [`Course::id`], so a writer can refer to
//!   the same objects again; an id outside `u32` reads as `None`.
//! - Courses: `first` starts a chain of `course-control`s linked by `next`. A course-control with a
//!   `variation` (fork or loop) makes the course a variation course; it is left out and noted. Courses are
//!   sorted by `order`. `options/@print-scale` is the print scale hint, `@climb` and `@course-length` the
//!   climb and length, `secondary-title` [`Course::secondary_title`]. `map-exchange="true"` at a normal control
//!   sets [`Stop::kind`], and `map-flip="true"` with it [`Stop::map_flip`].
//!   `number-location` is the number text centre relative to the control, kept as an [`Offset`]. The
//!   control's `all-controls-code-angle` places the code in the all-controls view only and is not read.
//! - Circle gaps are stored per control and per scale (`circle-gaps scale` as `start:stop,...` in degrees,
//!   older files as a 32-bit string in `gaps`); a course uses the entry for its circle-gap scale, which is
//!   its print scale unless `course-appearance/@scale-sizes` says otherwise. The load-time fixes of
//!   `EventDB.FixControlPointGaps` are applied on lookup, without copying entries; old gaps (scale 0)
//!   are also the fallback for a scale without an entry of its own.
//! - Legs belong to an ordered pair of controls (`start-control`, `end-control`) and carry `bends`, `gaps`
//!   (start and length along the leg) and `flagging`. A leg is a marked route when its flagging is `all`, or
//!   when Purple Pen flags it on its own: from a map issue point, into a map exchange control, or into a
//!   finish whose symbol is 14.1. A partly flagged leg is drawn as a normal leg and noted.
//! - Special objects map to [`Special`]; see `special()`. The courses of a special are `courses/@all` or a
//!   list of course ids; id 0 is the all-controls view and has no course of its own.
//!
//! - Notes for settings the overprint cannot honour: `first/@control-number` other than 1
//!   (`course:control-number`), `labels/@label-kind` other than sequence (normal) or code (score)
//!   (`course:label-kind`), `course-control/@points` other than 0 (`course-control:points`), and partly
//!   flagged legs.
//! - Limits: those of the IOF reader (ADR 0019), `MAX_CODE_LEN` and `MAX_NAME_LEN` per code and name, at most
//!   `MAX_COURSES` distinct course ids per special. Legs, controls and a special's course list are shared,
//!   so every copy into the result is paid from two budgets for the whole read: `MAX_TOTAL_POINTS` (bends,
//!   leg and circle gap ranges, special points) and `MAX_TOTAL_TEXT_BYTES` (codes, names, standards). A variation
//!   course is skipped before anything is copied.
//!
//! Elements Purple Pen writes but the overprint does not need (punch patterns, print areas, relay settings
//! and the like) are skipped silently (`labels` is read for the note above); any other element is noted once
//! by name.

use std::collections::{HashMap, HashSet};

use quick_xml::events::BytesStart;

use super::CourseError;
use super::model::*;
use super::xml::{Handler, Notes, TextBudget, attr, bounded, drive, local, number};

/// Parses the bytes of a Purple Pen `.ppen` file.
pub fn read_ppen(bytes: &[u8]) -> Result<RawEvent, CourseError> {
    let mut state = State::default();
    drive(bytes, &mut state)?;
    state.finish()
}

const ROOT: &str = "course-scribe-event";
/// Course-controls and legs in one file; enough for every course at its stop limit.
const MAX_COURSE_CONTROLS: usize = MAX_COURSES * MAX_STOPS_PER_COURSE;
/// Scales with their own circle gaps at one control.
const MAX_GAP_SCALES: usize = 256;
/// Purple Pen's default print scale of a course without `options`.
const DEFAULT_PRINT_SCALE: f64 = 15000.0;

/// Takes `n` points from the output budget of one read (bends, gap ranges and special points over all
/// courses), so that shared legs and controls cannot multiply into an unbounded result.
fn spend(left: &mut usize, n: usize) -> Result<(), CourseError> {
    *left = left.checked_sub(n).ok_or_else(|| {
        CourseError::Limit(format!(
            "more than {MAX_TOTAL_POINTS} points, gaps and special points in all courses together"
        ))
    })?;
    Ok(())
}

/// Purple Pen's load-time circle-gap fixes (`EventDB.FixControlPointGaps`), applied on lookup.
struct GapRules {
    map_key: i64,
    course_keys: HashSet<i64>,
    /// The largest key fix 1 would create from old gaps.
    max_old_key: i64,
    /// Fix 2: with item scaling, a scale without gaps takes those of the largest scale.
    copy_to: Option<i64>,
}

impl GapRules {
    /// Fix 1: old gaps (scale 0) replace the entries of the map scale and of every course print scale.
    fn fixed<'a>(&self, c: &'a PControl, key: i64) -> Option<&'a Vec<(f64, f64)>> {
        let old = c.gaps.get(&0);
        if old.is_some() && (key == self.map_key || self.course_keys.contains(&key)) {
            old
        } else if key != 0 {
            c.gaps.get(&key)
        } else {
            None
        }
    }

    /// The gaps of `c` at the circle-gap scale `key`. Old gaps (scale 0) are the fallback for a scale
    /// without an entry of its own.
    fn gaps_at<'a>(&self, c: &'a PControl, key: i64) -> &'a [(f64, f64)] {
        if let Some(g) = self.fixed(c, key) {
            return g;
        }
        if self.copy_to == Some(key) {
            let explicit = c.gaps.keys().copied().filter(|k| *k != 0).max();
            let old = c.gaps.contains_key(&0).then_some(self.max_old_key);
            if let Some(from) = explicit.into_iter().chain(old).max()
                && let Some(g) = self.fixed(c, from)
            {
                return g;
            }
        }
        c.gaps.get(&0).map_or(&[], Vec::as_slice)
    }
}

/// The children Purple Pen knows below `parent` (whose parent is `grand`), or `None` if the reader does not
/// look inside `parent`.
fn known_children(parent: &str, grand: &str) -> Option<&'static [&'static str]> {
    Some(match (grand, parent) {
        (_, ROOT) => &[
            "event",
            "control",
            "course",
            "course-control",
            "leg",
            "special-object",
        ],
        (ROOT, "event") => &[
            "standards",
            "title",
            "notes",
            "map",
            "all-controls",
            "numbering",
            "punch-card",
            "course-appearance",
            "print-area",
            "descriptions",
            "ocad",
            "custom-symbol-text",
            "livelox",
        ],
        ("event", "custom-symbol-text") => &["text"],
        (ROOT, "control") => &[
            "code",
            "location",
            "description",
            "description-text",
            "gaps",
            "circle-gaps",
            "punch-pattern",
            "description-text-line",
        ],
        (ROOT, "course") => &[
            "name",
            "secondary-title",
            "first",
            "print-area",
            "options",
            "labels",
            "part-options",
            "relay",
            "relay-branch",
        ],
        (ROOT, "course-control") => &[
            "next",
            "variation",
            "number-location",
            "description-text-line",
        ],
        (ROOT, "leg") => &["flagging", "bends", "gaps"],
        ("leg", "bends") => &["location"],
        ("leg", "gaps") => &["gap"],
        (ROOT, "special-object") => &[
            "text",
            "font",
            "location",
            "appearance",
            "courses",
            "image-data",
        ],
        ("special-object", "courses") => &["course"],
        _ => return None,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PKind {
    Normal,
    Start,
    MapIssue,
    Finish,
    CrossingPoint,
    MapExchange,
}

struct PControl {
    id: i64,
    kind: PKind,
    code: Option<String>,
    position: Option<RawPoint>,
    columns: [Option<String>; 6],
    column_f_text: Option<String>,
    has_description: bool,
    /// Circle gaps by scale (0 = every scale, from older files).
    gaps: HashMap<i64, Vec<(f64, f64)>>,
}

struct PCourse {
    id: i64,
    name: String,
    kind: CourseKind,
    order: i64,
    first: Option<i64>,
    print_scale: Option<f64>,
    climb: Option<f64>,
    length: Option<f64>,
    secondary_title: Option<String>,
}

struct PCc {
    control: i64,
    next: Option<i64>,
    variation: bool,
    map_exchange: bool,
    map_flip: bool,
    number_at: Option<Offset>,
}

#[derive(Default)]
struct PLeg {
    start: i64,
    end: i64,
    flagging: Option<String>,
    bends: Vec<RawPoint>,
    gaps: Vec<(f64, f64)>,
}

struct PSpecial {
    id: Option<u32>,
    kind: String,
    locations: Vec<RawPoint>,
    all: bool,
    /// Distinct course ids, in file order.
    courses: Vec<i64>,
    course_set: HashSet<i64>,
    text: String,
    font_height: Option<f64>,
}

/// `CourseView.CourseObjRatio`: map mm per mm of a course symbol's nominal size. `ScaleRatio` (print scale
/// over map scale, 1 when either is unknown) without item scaling; 1 relative to the map; 15000 over the map
/// scale relative to 1:15000. The print scale defaults to 15000.
pub fn course_obj_ratio(
    scaling: ItemScaling,
    map_scale: Option<f64>,
    print_scale: Option<f64>,
) -> f64 {
    let print = print_scale.unwrap_or(DEFAULT_PRINT_SCALE);
    let map = map_scale.filter(|m| *m > 0.0 && m.is_finite());
    let ratio = match (scaling, map) {
        (_, None) => 1.0,
        (ItemScaling::None, Some(m)) if print > 0.0 && print.is_finite() => print / m,
        (ItemScaling::None, Some(_)) => 1.0,
        (ItemScaling::RelativeToMap, Some(_)) => 1.0,
        (ItemScaling::RelativeTo15000, Some(m)) => 15000.0 / m,
    };
    // Purple Pen rounds a ratio this close to 1 to 1.
    if (ratio - 1.0).abs() < 1e-5 {
        1.0
    } else {
        ratio
    }
}

#[derive(Default)]
struct State {
    stack: Vec<String>,
    /// Per open element: the reader knows it and reads it.
    known: Vec<bool>,
    seen_root: bool,
    text: String,
    notes: Notes,
    title: Option<String>,
    map_scale: Option<f64>,
    standard: Option<String>,
    item_scaling: ItemScaling,
    gaps_scaled: bool,
    controls: Vec<PControl>,
    control_ids: HashSet<i64>,
    courses: Vec<PCourse>,
    course_ids: HashSet<i64>,
    ccs: HashMap<i64, PCc>,
    cc_count: usize,
    legs: HashMap<(i64, i64), PLeg>,
    leg_count: usize,
    specials: Vec<PSpecial>,
    control: Option<PControl>,
    /// `box` and `iof-2004-ref` of the open `description`.
    desc: Option<(String, Option<String>)>,
    /// `scale` of the open control `gaps` or `circle-gaps`.
    gap_scale: i64,
    course: Option<PCourse>,
    cc: Option<(i64, PCc)>,
    leg: Option<PLeg>,
    special: Option<PSpecial>,
}

fn int(e: &BytesStart<'_>, name: &str, what: &str) -> Result<Option<i64>, CourseError> {
    attr(e, name)?
        .map(|v| {
            v.trim()
                .parse::<i64>()
                .map_err(|_| CourseError::Xml(format!("bad number in {what}/@{name}: {v:?}")))
        })
        .transpose()
}

fn need_int(e: &BytesStart<'_>, name: &str, what: &str) -> Result<i64, CourseError> {
    int(e, name, what)?.ok_or_else(|| CourseError::Xml(format!("<{what}> lacks {name}")))
}

fn float(e: &BytesStart<'_>, name: &str, what: &str) -> Result<Option<f64>, CourseError> {
    attr(e, name)?
        .map(|v| number(&v, &format!("{what}/@{name}")))
        .transpose()
}

fn boolean(e: &BytesStart<'_>, name: &str, what: &str) -> Result<bool, CourseError> {
    match attr(e, name)?.as_deref().map(str::trim) {
        None | Some("false") | Some("0") => Ok(false),
        Some("true") | Some("1") => Ok(true),
        Some(v) => Err(CourseError::Xml(format!(
            "bad boolean in {what}/@{name}: {v:?}"
        ))),
    }
}

fn point(e: &BytesStart<'_>, what: &str) -> Result<RawPoint, CourseError> {
    let get = |n: &str| -> Result<f64, CourseError> {
        float(e, n, what)?.ok_or_else(|| CourseError::Xml(format!("<{what}> lacks {n}")))
    };
    Ok(RawPoint {
        x_mm: get("x")?,
        y_mm: get("y")?,
    })
}

fn too_many_points() -> CourseError {
    CourseError::Limit(format!(
        "more than {MAX_POINTS_PER_PATH} points in one path"
    ))
}

/// A scale denominator as a whole number, if it is one.
fn scale(v: f64) -> Option<u32> {
    (v >= 1.0 && v <= f64::from(u32::MAX)).then(|| v.round() as u32)
}

/// `CircleGap.DecodeGaps`: `start:stop` pairs separated by commas; malformed pairs are skipped.
fn decode_gaps(text: &str) -> Result<Vec<(f64, f64)>, CourseError> {
    let mut out = Vec::new();
    for pair in text.split(',') {
        let mut it = pair.split(':');
        if let (Some(a), Some(b), None) = (it.next(), it.next(), it.next())
            && let (Ok(a), Ok(b)) = (a.trim().parse::<f64>(), b.trim().parse::<f64>())
            && a.is_finite()
            && b.is_finite()
        {
            if out.len() >= MAX_POINTS_PER_PATH {
                return Err(too_many_points());
            }
            out.push((a, b));
        }
    }
    Ok(out)
}

/// `CircleGap.ComputeCircleGaps`: an old-style 32-bit string, most significant bit first. Bit `i` covers
/// `i·11.25°` to `(i+1)·11.25°` counter-clockwise from east; a clear bit is a gap.
fn bit_gaps(text: &str) -> Result<Vec<(f64, f64)>, CourseError> {
    let bits = u32::from_str_radix(text.trim(), 2)
        .map_err(|_| CourseError::Xml(format!("bad circle gaps {text:?}")))?;
    if bits == u32::MAX {
        return Ok(Vec::new());
    }
    if bits == 0 {
        return Ok(vec![(-360.0, 0.0)]);
    }
    let bit = |i: i32| bits & (1u32 << (i & 31)) != 0;
    let first = (0..32).find(|&i| !bit(i) && bit(i - 1)).unwrap_or(0);
    let mut gaps = Vec::new();
    let mut gap_start = first;
    for i in first..first + 32 {
        if bit(i) && !bit(i - 1) {
            gaps.push((
                f64::from(gap_start) * 360.0 / 32.0,
                f64::from(i) * 360.0 / 32.0,
            ));
        } else if !bit(i) && bit(i - 1) {
            gap_start = i;
        }
    }
    Ok(simplify_gaps(gaps))
}

/// `CircleGap.SimplifyGaps`: `stop` into 0..360 and `start` below it, sorted, overlaps merged.
fn simplify_gaps(gaps: Vec<(f64, f64)>) -> Vec<(f64, f64)> {
    let mut out: Vec<(f64, f64)> = gaps
        .into_iter()
        .filter(|(a, b)| b > a)
        .map(|(a, b)| {
            let stop = b.rem_euclid(360.0);
            let mut start = a.rem_euclid(360.0);
            if start > stop {
                start -= 360.0;
            }
            (start, stop)
        })
        .collect();
    out.sort_by(|x, y| x.0.total_cmp(&y.0));
    let mut i = 0;
    while i + 1 < out.len() {
        if out[i].1 >= out[i + 1].0 {
            out[i].1 = out[i].1.max(out[i + 1].1);
            out.remove(i + 1);
        } else {
            i += 1;
        }
    }
    while out.len() >= 2 && out[0].0 < out[out.len() - 1].1 - 360.0 {
        let last = out.remove(out.len() - 1);
        out[0].0 = last.0 - 360.0;
    }
    out
}

impl Handler for State {
    fn begin(&mut self, e: &BytesStart<'_>) -> Result<(), CourseError> {
        if self.stack.len() >= MAX_XML_DEPTH {
            return Err(CourseError::Limit(format!(
                "nesting too deep: more than {MAX_XML_DEPTH} levels of elements"
            )));
        }
        let name = local(e);
        self.text.clear();
        let known = match (self.stack.last(), self.known.last()) {
            (None, _) => {
                if name != ROOT {
                    return Err(CourseError::Xml(format!(
                        "root element is <{name}>, expected <{ROOT}>"
                    )));
                }
                if self.seen_root {
                    return Err(CourseError::Xml(format!("a second <{ROOT}>")));
                }
                self.seen_root = true;
                true
            }
            (Some(parent), Some(true)) => {
                let grand = self
                    .stack
                    .len()
                    .checked_sub(2)
                    .map_or("", |i| &self.stack[i]);
                match known_children(parent, grand) {
                    Some(set) if set.contains(&name.as_str()) => true,
                    Some(_) => {
                        self.notes.add(CourseNote::UnknownElement(name.clone()));
                        false
                    }
                    None => false,
                }
            }
            _ => false,
        };
        if known {
            self.open(e, &name)?;
        }
        self.stack.push(name);
        self.known.push(known);
        Ok(())
    }

    fn end(&mut self) -> Result<(), CourseError> {
        let (Some(name), Some(known)) = (self.stack.pop(), self.known.pop()) else {
            return Err(CourseError::Xml("unbalanced end tag".into()));
        };
        let text = std::mem::take(&mut self.text);
        if known {
            self.close(&name, text.trim())?;
        }
        Ok(())
    }

    fn text(&mut self, s: &str) {
        self.text.push_str(s);
    }
}

impl State {
    fn parent(&self) -> &str {
        self.stack.last().map_or("", String::as_str)
    }

    fn open(&mut self, e: &BytesStart<'_>, name: &str) -> Result<(), CourseError> {
        let parent = self.parent().to_string();
        match (parent.as_str(), name) {
            ("event", "map") => {
                self.map_scale = float(e, "scale", "map")?;
            }
            ("event", "standards") => {
                self.standard = attr(e, "description")?
                    .map(|s| bounded(s.trim(), MAX_STANDARD_LEN, "description standard"))
                    .transpose()?;
            }
            ("event", "course-appearance") => {
                self.item_scaling = match attr(e, "scale-sizes")?.as_deref().map(str::trim) {
                    Some("RelativeToMap") => ItemScaling::RelativeToMap,
                    Some("RelativeTo15000") => ItemScaling::RelativeTo15000,
                    _ => ItemScaling::None,
                };
                self.gaps_scaled = boolean(e, "scale-sizes-circle-gaps", "course-appearance")?;
            }
            (ROOT, "control") => {
                if self.controls.len() >= MAX_CONTROLS {
                    return Err(CourseError::Limit(format!(
                        "more than {MAX_CONTROLS} controls"
                    )));
                }
                let id = need_int(e, "id", "control")?;
                let kind_text = attr(e, "kind")?.unwrap_or_default();
                let kind = match kind_text.trim() {
                    "normal" => PKind::Normal,
                    "start" => PKind::Start,
                    "map-issue" => PKind::MapIssue,
                    "finish" => PKind::Finish,
                    "crossing-point" => PKind::CrossingPoint,
                    "map-exchange" => PKind::MapExchange,
                    other => {
                        self.notes
                            .add(CourseNote::UnknownElement(format!("control kind {other}")));
                        PKind::Normal
                    }
                };
                let mut gaps = HashMap::new();
                if let Some(g) = attr(e, "gaps")?
                    && !g.trim().is_empty()
                {
                    gaps.insert(0, bit_gaps(&g)?);
                }
                self.control = Some(PControl {
                    id,
                    kind,
                    code: None,
                    position: None,
                    columns: Default::default(),
                    column_f_text: None,
                    has_description: false,
                    gaps,
                });
            }
            ("control", "location") => {
                let p = point(e, "location")?;
                if let Some(c) = self.control.as_mut() {
                    c.position = Some(p);
                }
            }
            ("control", "description") => {
                let b = attr(e, "box")?.unwrap_or_default();
                let r = attr(e, "iof-2004-ref")?;
                self.desc = Some((b.trim().to_string(), r.map(|s| s.trim().to_string())));
            }
            ("control", "gaps" | "circle-gaps") => {
                self.gap_scale = int(e, "scale", name)?.unwrap_or(0);
            }
            (ROOT, "course") => {
                if self.courses.len() >= MAX_COURSES {
                    return Err(CourseError::Limit(format!(
                        "more than {MAX_COURSES} courses"
                    )));
                }
                let id = need_int(e, "id", "course")?;
                let kind = match attr(e, "kind")?.as_deref().map(str::trim) {
                    Some("normal") => CourseKind::Normal,
                    Some("score") => CourseKind::Score,
                    other => {
                        self.notes.add(CourseNote::UnknownElement(format!(
                            "course kind {}",
                            other.unwrap_or("")
                        )));
                        CourseKind::Normal
                    }
                };
                self.course = Some(PCourse {
                    id,
                    name: String::new(),
                    kind,
                    order: int(e, "order", "course")?.unwrap_or(0),
                    first: None,
                    print_scale: None,
                    climb: None,
                    length: None,
                    secondary_title: None,
                });
            }
            ("course", "first") => {
                let first = need_int(e, "course-control", "first")?;
                // The model numbers every course from 1.
                if int(e, "control-number", "first")?.is_some_and(|n| n != 1) {
                    self.notes
                        .add(CourseNote::UnknownElement("course:control-number".into()));
                }
                if let Some(c) = self.course.as_mut() {
                    c.first = Some(first);
                }
            }
            ("course", "labels") => {
                // Purple Pen labels a normal course by sequence number and a score course by code; the
                // overprint always prints sequence numbers.
                let default = match self.course.as_ref().map(|c| c.kind) {
                    Some(CourseKind::Score) => "code",
                    _ => "sequence",
                };
                if attr(e, "label-kind")?
                    .as_deref()
                    .is_some_and(|k| k.trim() != default)
                {
                    self.notes
                        .add(CourseNote::UnknownElement("course:label-kind".into()));
                }
            }
            ("course", "options") => {
                let print_scale = float(e, "print-scale", "options")?;
                let climb = float(e, "climb", "options")?.filter(|v| *v >= 0.0);
                let length = float(e, "course-length", "options")?.filter(|v| *v > 0.0);
                if let Some(c) = self.course.as_mut() {
                    c.print_scale = print_scale;
                    c.climb = climb;
                    c.length = length;
                }
            }
            (ROOT, "course-control") => {
                if self.cc_count >= MAX_COURSE_CONTROLS {
                    return Err(CourseError::Limit(format!(
                        "more than {MAX_COURSE_CONTROLS} course controls"
                    )));
                }
                self.cc_count += 1;
                let id = need_int(e, "id", "course-control")?;
                let variation = attr(e, "variation")?.is_some_and(|v| !v.trim().is_empty());
                // Score points are not printed.
                if int(e, "points", "course-control")?.is_some_and(|p| p != 0) {
                    self.notes
                        .add(CourseNote::UnknownElement("course-control:points".into()));
                }
                self.cc = Some((
                    id,
                    PCc {
                        control: need_int(e, "control", "course-control")?,
                        next: None,
                        variation,
                        map_exchange: boolean(e, "map-exchange", "course-control")?,
                        map_flip: boolean(e, "map-flip", "course-control")?,
                        number_at: None,
                    },
                ));
            }
            ("course-control", "next") => {
                let next = need_int(e, "course-control", "next")?;
                if let Some((_, c)) = self.cc.as_mut() {
                    c.next = Some(next);
                }
            }
            ("course-control", "variation") => {
                if let Some((_, c)) = self.cc.as_mut() {
                    c.variation = true;
                }
            }
            ("course-control", "number-location") => {
                let p = point(e, "number-location")?;
                if let Some((_, c)) = self.cc.as_mut() {
                    c.number_at = Some(Offset {
                        dx_mm: p.x_mm,
                        dy_mm: p.y_mm,
                    });
                }
            }
            (ROOT, "leg") => {
                if self.leg_count >= MAX_COURSE_CONTROLS {
                    return Err(CourseError::Limit(format!(
                        "more than {MAX_COURSE_CONTROLS} legs"
                    )));
                }
                self.leg_count += 1;
                self.leg = Some(PLeg {
                    start: need_int(e, "start-control", "leg")?,
                    end: need_int(e, "end-control", "leg")?,
                    ..PLeg::default()
                });
            }
            ("leg", "flagging") => {
                let kind = attr(e, "kind")?.unwrap_or_default();
                if let Some(l) = self.leg.as_mut() {
                    l.flagging = Some(kind.trim().to_string());
                }
            }
            ("bends", "location") => {
                let p = point(e, "location")?;
                if let Some(l) = self.leg.as_mut() {
                    if l.bends.len() >= MAX_POINTS_PER_PATH {
                        return Err(too_many_points());
                    }
                    l.bends.push(p);
                }
            }
            ("gaps", "gap") => {
                let start = float(e, "start", "gap")?.unwrap_or(0.0);
                let length = float(e, "length", "gap")?.unwrap_or(0.0);
                if let Some(l) = self.leg.as_mut() {
                    if l.gaps.len() >= MAX_POINTS_PER_PATH {
                        return Err(too_many_points());
                    }
                    l.gaps.push((start, start + length));
                }
            }
            (ROOT, "special-object") => {
                if self.specials.len() >= MAX_SPECIALS {
                    return Err(CourseError::Limit(format!(
                        "more than {MAX_SPECIALS} special objects"
                    )));
                }
                self.special = Some(PSpecial {
                    id: attr(e, "id")?
                        .and_then(|v| v.trim().parse::<i64>().ok())
                        .and_then(|v| u32::try_from(v).ok()),
                    kind: attr(e, "kind")?.unwrap_or_default().trim().to_string(),
                    locations: Vec::new(),
                    all: true,
                    courses: Vec::new(),
                    course_set: HashSet::new(),
                    text: String::new(),
                    font_height: None,
                });
            }
            ("special-object", "location") => {
                let p = point(e, "location")?;
                if let Some(s) = self.special.as_mut() {
                    if s.locations.len() >= MAX_POINTS_PER_PATH {
                        return Err(too_many_points());
                    }
                    s.locations.push(p);
                }
            }
            ("special-object", "font") => {
                let h = float(e, "height", "font")?.filter(|v| *v > 0.0);
                if let Some(s) = self.special.as_mut() {
                    s.font_height = h;
                }
            }
            ("special-object", "courses") => {
                let all = boolean(e, "all", "courses")?;
                if let Some(s) = self.special.as_mut() {
                    s.all = all;
                }
            }
            ("courses", "course") => {
                let id = need_int(e, "course", "course")?;
                if let Some(s) = self.special.as_mut()
                    && s.course_set.insert(id)
                {
                    if s.courses.len() >= MAX_COURSES {
                        return Err(CourseError::Limit(format!(
                            "more than {MAX_COURSES} courses for one special object"
                        )));
                    }
                    s.courses.push(id);
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn close(&mut self, name: &str, text: &str) -> Result<(), CourseError> {
        let parent = self.parent().to_string();
        let missing = |what: &str| CourseError::Xml(format!("<{what}> closed but not open"));
        match (parent.as_str(), name) {
            ("event", "title") => self.title = Some(text.to_string()),
            ("control", "code") => {
                if let Some(c) = self.control.as_mut() {
                    c.code = Some(bounded(text, MAX_CODE_LEN, "control code")?);
                }
            }
            ("control", "description") => {
                let (b, r) = self.desc.take().ok_or_else(|| missing("description"))?;
                let column = match b.as_str() {
                    "all" | "C" => Some(0),
                    "D" => Some(1),
                    "E" => Some(2),
                    "F" => Some(3),
                    "G" => Some(4),
                    "H" => Some(5),
                    other => {
                        self.notes.add(CourseNote::UnknownElement(format!(
                            "description box {other}"
                        )));
                        None
                    }
                };
                if let (Some(i), Some(c)) = (column, self.control.as_mut()) {
                    c.has_description = true;
                    if r.is_some() {
                        c.columns[i] = r;
                    }
                    if i == 3 && b == "F" && !text.is_empty() {
                        c.column_f_text = Some(text.to_string());
                    }
                }
            }
            ("control", "gaps" | "circle-gaps") => {
                let scale = self.gap_scale;
                if let Some(c) = self.control.as_mut()
                    && !text.is_empty()
                {
                    let new_style = text.contains(':');
                    let gaps = if new_style {
                        Some(decode_gaps(text)?)
                    } else if name == "gaps" && !c.gaps.contains_key(&scale) {
                        Some(bit_gaps(text)?)
                    } else {
                        None
                    };
                    if let Some(g) = gaps {
                        if c.gaps.len() >= MAX_GAP_SCALES && !c.gaps.contains_key(&scale) {
                            return Err(CourseError::Limit(format!(
                                "circle gaps for more than {MAX_GAP_SCALES} scales at one control"
                            )));
                        }
                        c.gaps.insert(scale, g);
                    }
                }
            }
            (ROOT, "control") => {
                let c = self.control.take().ok_or_else(|| missing("control"))?;
                if !self.control_ids.insert(c.id) {
                    return Err(CourseError::Xml(format!(
                        "control id {} is defined twice",
                        c.id
                    )));
                }
                self.controls.push(c);
            }
            ("course", "name") => {
                if let Some(c) = self.course.as_mut() {
                    c.name = bounded(text, MAX_NAME_LEN, "course name")?;
                }
            }
            ("course", "secondary-title") => {
                if let Some(c) = self.course.as_mut() {
                    c.secondary_title = Some(bounded(text, MAX_NAME_LEN, "secondary title")?);
                }
            }
            (ROOT, "course") => {
                let c = self.course.take().ok_or_else(|| missing("course"))?;
                if !self.course_ids.insert(c.id) {
                    return Err(CourseError::Xml(format!(
                        "course id {} is defined twice",
                        c.id
                    )));
                }
                self.courses.push(c);
            }
            (ROOT, "course-control") => {
                let (id, c) = self.cc.take().ok_or_else(|| missing("course-control"))?;
                if self.ccs.insert(id, c).is_some() {
                    return Err(CourseError::Xml(format!(
                        "course-control id {id} is defined twice"
                    )));
                }
            }
            (ROOT, "leg") => {
                let l = self.leg.take().ok_or_else(|| missing("leg"))?;
                // Purple Pen uses the first leg of a pair.
                self.legs.entry((l.start, l.end)).or_insert(l);
            }
            ("special-object", "text") => {
                if let Some(s) = self.special.as_mut() {
                    s.text = text.to_string();
                }
            }
            (ROOT, "special-object") => {
                let s = self
                    .special
                    .take()
                    .ok_or_else(|| missing("special-object"))?;
                if s.locations.is_empty() {
                    return Err(CourseError::Xml(format!(
                        "special object {:?} has no location",
                        s.kind
                    )));
                }
                self.specials.push(s);
            }
            _ => {}
        }
        Ok(())
    }

    /// What `EventDB.FixControlPointGaps` needs to know, so that a lookup gives the gaps Purple Pen would
    /// have after its load-time fixes without copying any entry.
    fn gap_rules(&self) -> GapRules {
        let map_key = self.map_scale.map_or(0, |s| s.round() as i64);
        let course_keys: HashSet<i64> = self
            .courses
            .iter()
            .map(|c| c.print_scale.unwrap_or(DEFAULT_PRINT_SCALE).round() as i64)
            .collect();
        let copy_to = match self.item_scaling {
            _ if self.gaps_scaled => None,
            ItemScaling::None => None,
            ItemScaling::RelativeToMap => Some(map_key),
            ItemScaling::RelativeTo15000 => Some(15000),
        };
        GapRules {
            map_key,
            max_old_key: course_keys
                .iter()
                .copied()
                .chain([map_key])
                .max()
                .unwrap_or(map_key),
            course_keys,
            copy_to,
        }
    }

    fn course_obj_ratio(&self, course: &PCourse) -> f64 {
        course_obj_ratio(self.item_scaling, self.map_scale, course.print_scale)
    }

    /// `CourseView.CircleGapScale`, rounded as Purple Pen keys the gaps.
    fn gap_key(&self, course: &PCourse) -> i64 {
        let print = course.print_scale.unwrap_or(DEFAULT_PRINT_SCALE);
        let s = match self.item_scaling {
            ItemScaling::None => print,
            ItemScaling::RelativeToMap => self.map_scale.unwrap_or(0.0),
            ItemScaling::RelativeTo15000 => 15000.0,
        };
        s.round() as i64
    }

    fn finish(mut self) -> Result<RawEvent, CourseError> {
        if !self.stack.is_empty() || !self.seen_root {
            return Err(CourseError::Xml("unexpected end of file".into()));
        }
        let rules = self.gap_rules();
        if let Some(c) = self.controls.iter().find(|c| c.gaps.len() > MAX_GAP_SCALES) {
            return Err(CourseError::Limit(format!(
                "circle gaps for more than {MAX_GAP_SCALES} scales at control id {}",
                c.id
            )));
        }
        let mut budget = MAX_TOTAL_POINTS;
        let standard = self.standard.clone().unwrap_or_else(|| "2004".into());

        // Codes: a normal control's own code, else the code Purple Pen's IOF XML export gives it.
        let mut used: HashSet<String> = HashSet::new();
        for c in &self.controls {
            if c.kind == PKind::Normal
                && let Some(code) = c.code.as_ref().filter(|s| !s.is_empty())
                && !used.insert(code.clone())
            {
                return Err(CourseError::Xml(format!(
                    "control code {code} is defined twice"
                )));
            }
        }
        let mut counters: HashMap<&'static str, u32> = HashMap::new();
        let mut codes: Vec<String> = Vec::with_capacity(self.controls.len());
        for c in &self.controls {
            let own = c
                .code
                .as_ref()
                .filter(|s| c.kind == PKind::Normal && !s.is_empty());
            let code = match own {
                Some(code) => code.clone(),
                None => {
                    let prefix = match c.kind {
                        PKind::Start => "STA",
                        PKind::MapExchange => "XCHG",
                        PKind::Normal => "CTL",
                        PKind::Finish => "FIN",
                        PKind::CrossingPoint => "CROSS",
                        PKind::MapIssue => "ISSUE",
                    };
                    let n = counters.entry(prefix).or_insert(0);
                    loop {
                        *n += 1;
                        let code = format!("{prefix}{n}");
                        if used.insert(code.clone()) {
                            break code;
                        }
                    }
                }
            };
            codes.push(code);
        }
        let index: HashMap<i64, usize> = self
            .controls
            .iter()
            .enumerate()
            .map(|(i, c)| (c.id, i))
            .collect();

        // Courses, in Purple Pen's order; an order of 0 (older files) sorts last.
        let mut order: Vec<usize> = (0..self.courses.len()).collect();
        order.sort_by_key(|&i| {
            let o = self.courses[i].order;
            if o <= 0 { i64::MAX } else { o }
        });
        let names: HashMap<i64, String> = self
            .courses
            .iter()
            .map(|c| (c.id, c.name.clone()))
            .collect();
        let mut text = TextBudget::default();
        let mut courses = Vec::new();
        for &ci in &order {
            let pc = &self.courses[ci];
            // First the chain alone, so that a variation course is skipped before anything is copied.
            let mut chain: Vec<(&PCc, usize)> = Vec::new();
            let mut seen = HashSet::new();
            let mut variations = false;
            let mut next = pc.first;
            while let Some(id) = next {
                if !seen.insert(id) {
                    return Err(CourseError::Xml(format!(
                        "the controls of course {} run in a loop",
                        pc.name
                    )));
                }
                if chain.len() >= MAX_STOPS_PER_COURSE {
                    return Err(CourseError::Limit(format!(
                        "more than {MAX_STOPS_PER_COURSE} stops in a course"
                    )));
                }
                let cc = self.ccs.get(&id).ok_or_else(|| {
                    CourseError::Xml(format!(
                        "course {} uses course-control {id}, which is not defined",
                        pc.name
                    ))
                })?;
                if cc.variation {
                    // A fork or loop: the `next` links of its branches lead back into the chain, so the
                    // walk stops here (QueryEvent.EnumCourseControlsAndSplitStartsToJoin follows them).
                    variations = true;
                    break;
                }
                let &i = index.get(&cc.control).ok_or_else(|| {
                    CourseError::Xml(format!(
                        "course {} uses control id {}, which is not defined",
                        pc.name, cc.control
                    ))
                })?;
                if self.controls[i].position.is_none() {
                    return Err(CourseError::Xml(format!(
                        "control {} has no location",
                        codes[i]
                    )));
                }
                chain.push((cc, i));
                next = cc.next;
            }
            if variations {
                // A course left out must always be named, also after the list was cut short.
                self.notes
                    .add_final(CourseNote::Variations(text.copy(&pc.name)?));
                continue;
            }
            let mut stops = Vec::with_capacity(chain.len());
            let mut ids = Vec::with_capacity(chain.len());
            for (cc, i) in chain {
                let c = &self.controls[i];
                let gaps = rules.gaps_at(c, self.gap_key(pc));
                spend(&mut budget, gaps.len())?;
                stops.push(Stop {
                    control: text.copy(&codes[i])?,
                    kind: (cc.map_exchange && c.kind == PKind::Normal)
                        .then_some(ControlKind::MapExchange),
                    // Purple Pen honours the flip only at a map exchange.
                    number: None,
                    number_text_at: cc.number_at,
                    circle_gaps: gaps.to_vec(),
                    map_flip: cc.map_flip && cc.map_exchange,
                });
                ids.push(i);
            }
            let mut legs = Vec::new();
            for w in ids.windows(2) {
                let (a, b) = (&self.controls[w[0]], &self.controls[w[1]]);
                let leg = self.legs.get(&(a.id, b.id));
                let flagging = leg.and_then(|l| l.flagging.as_deref()).unwrap_or("none");
                let implied = a.kind == PKind::MapIssue
                    || b.kind == PKind::MapExchange
                    || (b.kind == PKind::Finish && b.columns[0].as_deref() == Some("14.1"));
                let marked = implied || flagging == "all";
                if !marked && flagging != "none" {
                    self.notes.add(CourseNote::UnknownElement(format!(
                        "leg flagging {flagging}"
                    )));
                }
                legs.push(Leg {
                    bends: match leg {
                        Some(l) => {
                            spend(&mut budget, l.bends.len() + l.gaps.len())?;
                            l.bends.clone()
                        }
                        None => Vec::new(),
                    },
                    gaps: leg.map(|l| l.gaps.clone()).unwrap_or_default(),
                    marked_route: marked,
                });
            }
            courses.push(Course {
                id: u32::try_from(pc.id).ok(),
                name: text.copy(&pc.name)?,
                family: None,
                kind: pc.kind,
                legs,
                stops,
                climb_m: pc.climb,
                length_m: pc.length,
                print_scale_hint: pc.print_scale.and_then(scale),
                course_obj_ratio: Some(self.course_obj_ratio(pc)),
                secondary_title: pc
                    .secondary_title
                    .as_deref()
                    .map(|t| text.copy(t))
                    .transpose()?,
            });
        }

        let mut controls = Vec::with_capacity(self.controls.len());
        let mut any_description = false;
        for (c, code) in std::mem::take(&mut self.controls).into_iter().zip(codes) {
            text.copy(&code)?;
            let kind = match c.kind {
                PKind::Normal => ControlKind::Control,
                PKind::Start => ControlKind::Start,
                PKind::MapIssue => {
                    self.notes.add(CourseNote::MapIssueAsStart(code.clone()));
                    ControlKind::Start
                }
                PKind::Finish => ControlKind::Finish,
                PKind::CrossingPoint => ControlKind::CrossingPoint,
                PKind::MapExchange => ControlKind::MapExchange,
            };
            any_description |= c.has_description;
            controls.push(Control {
                id: u32::try_from(c.id).ok(),
                code,
                kind,
                position: c.position,
                geo: None,
                description: if c.has_description {
                    Some(Description {
                        columns: c.columns,
                        standard: text.copy(&standard)?,
                        column_f_text: c.column_f_text,
                    })
                } else {
                    None
                },
            });
        }

        let mut specials = Vec::new();
        for s in std::mem::take(&mut self.specials) {
            spend(&mut budget, s.locations.len())?;
            if let Some(sp) = self.special(s, &names, &mut text)? {
                specials.push(sp);
            }
        }
        if !any_description {
            self.notes.add_final(CourseNote::NoDescriptions);
        }
        Ok(CourseEvent {
            controls,
            courses,
            specials,
            source: CourseSource {
                format: SourceFormat::PurplePen,
                event_name: self.title,
                map_scale: self.map_scale.and_then(scale),
                item_scaling: Some(self.item_scaling),
            },
            notes: self.notes.into_vec(),
        })
    }

    /// One special object, or `None` with a note for kinds the model has no variant for.
    fn special(
        &mut self,
        s: PSpecial,
        names: &HashMap<i64, String>,
        text: &mut TextBudget,
    ) -> Result<Option<Special<RawPoint>>, CourseError> {
        const KNOWN: &[&str] = &[
            "first-aid",
            "water",
            "forbidden-route",
            "boundary",
            "line",
            "out-of-bounds",
            "dangerous-area",
            "white-out",
            "text",
        ];
        let Some(first) = s.locations.first().copied() else {
            return Ok(None);
        };
        let unknown = match s.kind.as_str() {
            "descriptions" => Some("special:descriptions ignored (layout decides)".to_string()),
            "text" if s.locations.len() < 2 => Some("special:text without a box".to_string()),
            k if !KNOWN.contains(&k) => Some(format!("special:{k}")),
            _ => None,
        };
        if let Some(what) = unknown {
            self.notes.add(CourseNote::UnknownElement(what));
            return Ok(None);
        }
        let courses = if s.all {
            CourseFilter::All
        } else {
            let mut ids = HashSet::new();
            let mut only = Vec::new();
            for id in &s.courses {
                if ids.insert(*id)
                    && let Some(n) = names.get(id)
                {
                    only.push(text.copy(n)?);
                }
            }
            CourseFilter::Only(only)
        };
        Ok(Some(match s.kind.as_str() {
            "first-aid" => Special::FirstAid {
                id: s.id,
                pt: first,
                courses,
            },
            "water" => Special::Water {
                id: s.id,
                pt: first,
                courses,
            },
            "forbidden-route" => Special::ForbiddenRoute {
                id: s.id,
                path: vec![first],
                courses,
            },
            "boundary" => Special::Boundary {
                id: s.id,
                path: s.locations,
                courses,
            },
            "line" => Special::Line {
                id: s.id,
                path: s.locations,
                courses,
            },
            "out-of-bounds" => Special::OutOfBounds {
                id: s.id,
                rings: vec![s.locations],
                courses,
            },
            "dangerous-area" => Special::Dangerous {
                id: s.id,
                rings: vec![s.locations],
                courses,
            },
            "white-out" => {
                self.notes.add(CourseNote::WhiteOutLaterInM4);
                Special::WhiteOut {
                    id: s.id,
                    rings: vec![s.locations],
                    courses,
                }
            }
            "text" => {
                let second = s.locations.get(1).copied().unwrap_or(first);
                // Without a font height Purple Pen sizes the text, possibly several lines, to fit the box;
                // the box height is only an approximation of the letter height.
                Special::Text {
                    id: s.id,
                    pt: RawPoint {
                        x_mm: first.x_mm.min(second.x_mm),
                        y_mm: first.y_mm.max(second.y_mm),
                    },
                    text: s.text,
                    size_mm: s
                        .font_height
                        .unwrap_or_else(|| (first.y_mm - second.y_mm).abs()),
                    courses,
                }
            }
            // Every other kind returned above with a note.
            _ => return Ok(None),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bit_gaps_match_purple_pen() {
        assert_eq!(bit_gaps(&"1".repeat(32)).unwrap(), vec![]);
        assert_eq!(bit_gaps(&"0".repeat(32)).unwrap(), vec![(-360.0, 0.0)]);
        // Lincoln Park control 31: bits 20-28 clear, next to the angle form 224.4:330.1 of the same file.
        assert_eq!(
            bit_gaps("11100000000011111111111111111111").unwrap(),
            vec![(225.0, 326.25)]
        );
        // A gap across east starts negative.
        assert_eq!(
            bit_gaps("01111111111111111111111111111110").unwrap(),
            vec![(-11.25, 11.25)]
        );
        // Two gaps, sorted.
        assert_eq!(
            bit_gaps("11111111111111110111111111111101").unwrap(),
            vec![(11.25, 22.5), (168.75, 180.0)]
        );
        assert!(bit_gaps("12").is_err());
        assert!(bit_gaps(&"1".repeat(33)).is_err());
    }

    #[test]
    fn decode_gaps_skips_malformed_pairs() {
        assert_eq!(
            decode_gaps("10:20,bad,30:x,40:50:60,-5:5").unwrap(),
            vec![(10.0, 20.0), (-5.0, 5.0)]
        );
    }

    #[test]
    fn simplify_merges_and_wraps() {
        assert_eq!(
            simplify_gaps(vec![
                (350.0, 370.0),
                (30.0, 40.0),
                (35.0, 50.0),
                (20.0, 10.0)
            ]),
            vec![(-10.0, 10.0), (30.0, 50.0)]
        );
    }

    fn event(appearance: &str, control_gaps: &str) -> RawEvent {
        let xml = format!(
            "<course-scribe-event><event id=\"1\"><map kind=\"none\" scale=\"10000\"/>{appearance}</event>\
             <control id=\"1\" kind=\"normal\" {control_gaps}<code>31</code><location x=\"0\" y=\"0\"/></control>\
             <course id=\"1\" kind=\"normal\" order=\"1\"><name>A</name><first course-control=\"1\"/>\
             <options print-scale=\"7500\"/></course>\
             <course id=\"2\" kind=\"normal\" order=\"2\"><name>B</name><first course-control=\"2\"/></course>\
             <course-control id=\"1\" control=\"1\"/><course-control id=\"2\" control=\"1\"/>\
             </course-scribe-event>"
        );
        read_ppen(xml.as_bytes()).unwrap()
    }

    #[test]
    fn old_gaps_attribute_applies_to_every_course_scale() {
        let ev = event("", "gaps=\"01111111111111111111111111111110\">");
        for c in &ev.courses {
            assert_eq!(c.stops[0].circle_gaps, vec![(-11.25, 11.25)], "{}", c.name);
        }
    }

    #[test]
    fn old_gaps_are_a_fallback_not_copied_per_scale() {
        // Old gaps for every scale plus explicit entries up to the limit, and 300 courses at their own print
        // scales: the entries stay as read, and every course finds the old gaps.
        let explicit: String = (1..MAX_GAP_SCALES as i64)
            .map(|k| format!("<circle-gaps scale=\"{}\">1:2</circle-gaps>", 100_000 + k))
            .collect();
        let mut xml = format!(
            "<course-scribe-event><event id=\"1\"><map kind=\"none\" scale=\"10000\"/></event>\
             <control id=\"1\" kind=\"normal\" gaps=\"01111111111111111111111111111110\">\
             <code>31</code><location x=\"0\" y=\"0\"/>{explicit}</control>"
        );
        for i in 0..300 {
            xml.push_str(&format!(
                "<course id=\"{i}\" kind=\"normal\" order=\"{}\"><name>{i}</name>\
                 <first course-control=\"{i}\"/><options print-scale=\"{}\"/></course>\
                 <course-control id=\"{i}\" control=\"1\"/>",
                i + 1,
                1000 + i
            ));
        }
        xml.push_str("</course-scribe-event>");
        let mut state = State::default();
        drive(xml.as_bytes(), &mut state).unwrap();
        assert_eq!(state.controls[0].gaps.len(), MAX_GAP_SCALES);
        let rules = state.gap_rules();
        assert_eq!(rules.gaps_at(&state.controls[0], 1000), &[(-11.25, 11.25)]);
        assert_eq!(rules.gaps_at(&state.controls[0], 100_001), &[(1.0, 2.0)]);
        assert_eq!(state.controls[0].gaps.len(), MAX_GAP_SCALES);
        let ev = state.finish().unwrap();
        assert_eq!(ev.courses.len(), 300);
        let with_old = ev
            .courses
            .iter()
            .filter(|c| c.stops[0].circle_gaps == [(-11.25, 11.25)])
            .count();
        assert_eq!(with_old, 300);
        // One scale more is refused.
        let over = xml.replace(
            "<location x=\"0\" y=\"0\"/>",
            "<location x=\"0\" y=\"0\"/><circle-gaps scale=\"99\">1:2</circle-gaps>",
        );
        assert!(matches!(
            read_ppen(over.as_bytes()),
            Err(CourseError::Limit(_))
        ));
    }

    #[test]
    fn item_scaling_picks_the_scaled_gaps() {
        // Relative to 1:15000, gaps written for 1:7500 before Purple Pen scaled them are copied over.
        let ev = event(
            "<course-appearance scale-sizes=\"RelativeTo15000\"/>",
            "><circle-gaps scale=\"7500\">10:20</circle-gaps>",
        );
        assert_eq!(ev.courses[0].stops[0].circle_gaps, vec![(10.0, 20.0)]);
        assert_eq!(ev.courses[1].stops[0].circle_gaps, vec![(10.0, 20.0)]);
        // Newer files say the gaps are already scaled: no 1:15000 entry, no gaps.
        let ev = event(
            "<course-appearance scale-sizes=\"RelativeTo15000\" scale-sizes-circle-gaps=\"true\"/>",
            "><circle-gaps scale=\"7500\">10:20</circle-gaps>",
        );
        assert!(ev.courses[0].stops[0].circle_gaps.is_empty());
        // Relative to the map: the map scale's entry.
        let ev = event(
            "<course-appearance scale-sizes=\"RelativeToMap\" scale-sizes-circle-gaps=\"true\"/>",
            "><circle-gaps scale=\"10000\">1:2</circle-gaps><circle-gaps scale=\"7500\">3:4</circle-gaps>",
        );
        assert_eq!(ev.courses[0].stops[0].circle_gaps, vec![(1.0, 2.0)]);
    }

    #[test]
    fn course_obj_ratio_follows_item_scaling() {
        // Map 1:10000; course A at 1:7500, course B without a print scale (Purple Pen's default 1:15000).
        let ratios = |appearance: &str| -> Vec<Option<f64>> {
            event(appearance, ">")
                .courses
                .iter()
                .map(|c| c.course_obj_ratio)
                .collect()
        };
        // No item scaling: the symbols keep their size on paper, so they take print over map scale in map mm.
        assert_eq!(ratios(""), [Some(0.75), Some(1.5)]);
        assert_eq!(
            ratios("<course-appearance scale-sizes=\"None\"/>"),
            [Some(0.75), Some(1.5)]
        );
        assert_eq!(
            ratios("<course-appearance scale-sizes=\"RelativeToMap\"/>"),
            [Some(1.0), Some(1.0)]
        );
        assert_eq!(
            ratios("<course-appearance scale-sizes=\"RelativeTo15000\"/>"),
            [Some(1.5), Some(1.5)]
        );
    }
}
