//! Reader for IOF XML 3.0 `CourseData` files (Purple Pen, Condes, OCAD and others write them).
//!
//! Streaming with quick-xml. A DOCTYPE is refused and entities other than the five predefined ones are
//! errors (ADR 0019). Positions stay as the file gives them: the reader does not flip y, because
//! writers differ and the fitting step detects the axis direction.
//!
//! What it reads: `Event/Name`, `Map/Scale`, `Control` (`Id`, `Position`, `MapPosition`), `Course` (`Name`,
//! `CourseFamily`, `Length`, `Climb`, `CourseControl` with its `type`, `Control`, `Score` and
//! `MapTextPosition`). `MapText` is not read. IOF XML carries no descriptions, bends, gaps or special
//! objects.
//!
//! Courses of one `CourseFamily` are the variants of a fork; each stays its own course and the event gets a
//! `Forked` note. A `CourseControl` with several `Control` children means alternative controls (the runner
//! punches one of them); such a course is left out and noted `AlternativeControls`. `MapTextPosition` is the
//! text centre relative to the circle centre and is kept as an [`Offset`]. `CourseControl/@type` sets
//! [`Stop::kind`] for that course only; the control keeps its own `type`. `RaceCourseData` is read only
//! directly under the root `CourseData`.

use std::collections::HashSet;

use quick_xml::events::BytesStart;

use super::CourseError;
use super::model::*;
use super::xml::{Handler, Notes, attr, bounded, drive, local, number};

/// Parses the bytes of an IOF XML 3.0 `CourseData` file.
pub fn read_iof_xml(bytes: &[u8]) -> Result<RawEvent, CourseError> {
    let mut state = State::default();
    drive(bytes, &mut state)?;
    state.finish()
}

impl Handler for State {
    fn begin(&mut self, e: &BytesStart<'_>) -> Result<(), CourseError> {
        State::begin(self, e)
    }
    fn end(&mut self) -> Result<(), CourseError> {
        State::end(self)
    }
    fn text(&mut self, s: &str) {
        self.text.push_str(s);
    }
}

/// `MapPosition` / `MapTextPosition`: x and y in mm.
fn point(e: &BytesStart<'_>, what: &str) -> Result<RawPoint, CourseError> {
    match attr(e, "unit")?.as_deref() {
        None | Some("mm") => {}
        Some(u) => {
            return Err(CourseError::Unsupported(format!(
                "{what} uses unit {u:?}; only mm is supported"
            )));
        }
    }
    let get = |n: &str| -> Result<f64, CourseError> {
        let v = attr(e, n)?.ok_or_else(|| CourseError::Xml(format!("{what} lacks {n}")))?;
        number(&v, what)
    };
    Ok(RawPoint {
        x_mm: get("x")?,
        y_mm: get("y")?,
    })
}

struct ControlB {
    code: String,
    kind: ControlKind,
    position: Option<RawPoint>,
    geo: Option<(f64, f64)>,
}

struct CcB {
    codes: Vec<String>,
    kind: Option<ControlKind>,
    text_at: Option<Offset>,
    score: bool,
}

struct CourseB {
    course: Course<RawPoint>,
    alternatives: bool,
}

fn kind_of(s: &str) -> Option<ControlKind> {
    Some(match s {
        "Control" => ControlKind::Control,
        "Start" => ControlKind::Start,
        "Finish" => ControlKind::Finish,
        "CrossingPoint" => ControlKind::CrossingPoint,
        "EndOfMarkedRoute" => ControlKind::EndOfMarkedRoute,
        _ => return None,
    })
}

fn non_negative(text: &str, what: &str) -> Result<f64, CourseError> {
    let v = number(text, what)?;
    if v < 0.0 {
        return Err(CourseError::Xml(format!("{what} is negative: {text:?}")));
    }
    Ok(v)
}

#[derive(Default)]
struct State {
    stack: Vec<String>,
    text: String,
    controls: Vec<Control<RawPoint>>,
    codes: HashSet<String>,
    courses: Vec<Course<RawPoint>>,
    /// Courses seen so far, including those left out.
    course_count: usize,
    notes: Notes,
    event_name: Option<String>,
    map_scale: Option<u32>,
    control: Option<ControlB>,
    course: Option<CourseB>,
    cc: Option<CcB>,
}

impl State {
    fn note(&mut self, n: CourseNote) {
        self.notes.add(n);
    }

    fn parent(&self) -> &str {
        self.stack.last().map_or("", String::as_str)
    }

    /// The open element is a direct child of the `RaceCourseData` under the root.
    fn in_race(&self) -> bool {
        self.stack.len() == 2 && self.stack[0] == "CourseData" && self.stack[1] == "RaceCourseData"
    }

    fn begin(&mut self, e: &BytesStart<'_>) -> Result<(), CourseError> {
        let name = local(e);
        if self.stack.len() >= MAX_XML_DEPTH {
            return Err(CourseError::Limit(format!(
                "nesting too deep: more than {MAX_XML_DEPTH} levels of elements"
            )));
        }
        self.text.clear();
        let parent = self.parent().to_string();
        let in_race = self.in_race();
        if self.stack.is_empty() {
            if name != "CourseData" {
                return Err(CourseError::Xml(format!(
                    "root element is <{name}>, expected <CourseData>"
                )));
            }
        } else if self.stack.len() == 1 {
            if !matches!(name.as_str(), "Event" | "RaceCourseData") {
                self.note(CourseNote::UnknownElement(name.clone()));
            }
        } else if (in_race && !matches!(name.as_str(), "Map" | "Control" | "Course"))
            || name == "RaceCourseData"
        {
            self.note(CourseNote::UnknownElement(name.clone()));
        }
        // A `Control`, `Course` or `CourseControl` inside another one is malformed: it would otherwise be read as
        // part of the outer one.
        if parent == name && matches!(name.as_str(), "Control" | "Course" | "CourseControl") {
            return Err(CourseError::Xml(format!(
                "<{name}> opened inside another <{name}>"
            )));
        }
        match (parent.as_str(), name.as_str()) {
            ("RaceCourseData", "Control") if in_race => {
                if self.controls.len() >= MAX_CONTROLS {
                    return Err(CourseError::Limit(format!(
                        "more than {MAX_CONTROLS} controls"
                    )));
                }
                let kind = match attr(e, "type")?.as_deref() {
                    None => ControlKind::Control,
                    Some(t) => kind_of(t).unwrap_or_else(|| {
                        self.note(CourseNote::UnknownElement(format!("Control type {t}")));
                        ControlKind::Control
                    }),
                };
                self.control = Some(ControlB {
                    code: String::new(),
                    kind,
                    position: None,
                    geo: None,
                });
            }
            ("Control", "Position") => {
                if let Some(c) = self.control.as_mut() {
                    let lat = attr(e, "lat")?
                        .map(|v| number(&v, "Position"))
                        .transpose()?;
                    let lng = attr(e, "lng")?
                        .map(|v| number(&v, "Position"))
                        .transpose()?;
                    if let (Some(lat), Some(lng)) = (lat, lng) {
                        c.geo = Some((lat, lng));
                    }
                }
            }
            ("Control", "MapPosition") => {
                if let Some(c) = self.control.as_mut() {
                    c.position = Some(point(e, "MapPosition")?);
                }
            }
            ("RaceCourseData", "Course") if in_race => {
                if self.course_count >= MAX_COURSES {
                    return Err(CourseError::Limit(format!(
                        "more than {MAX_COURSES} courses"
                    )));
                }
                self.course_count += 1;
                self.course = Some(CourseB {
                    course: Course {
                        // IOF XML has no stable ids.
                        id: None,
                        name: String::new(),
                        family: None,
                        kind: CourseKind::Normal,
                        legs: Vec::new(),
                        stops: Vec::new(),
                        climb_m: None,
                        length_m: None,
                        print_scale_hint: None,
                        course_obj_ratio: None,
                        secondary_title: None,
                    },
                    alternatives: false,
                });
            }
            ("Course", "CourseControl") if self.course.is_some() => {
                if let Some(c) = self.course.as_ref()
                    && c.course.stops.len() >= MAX_STOPS_PER_COURSE
                {
                    return Err(CourseError::Limit(format!(
                        "more than {MAX_STOPS_PER_COURSE} stops in a course"
                    )));
                }
                let kind = match attr(e, "type")?.as_deref() {
                    None => None,
                    Some(t) => kind_of(t).or_else(|| {
                        self.note(CourseNote::UnknownElement(format!(
                            "CourseControl type {t}"
                        )));
                        None
                    }),
                };
                self.cc = Some(CcB {
                    codes: Vec::new(),
                    kind,
                    text_at: None,
                    score: false,
                });
            }
            ("CourseControl", "MapTextPosition") => {
                if let Some(c) = self.cc.as_mut() {
                    let p = point(e, "MapTextPosition")?;
                    c.text_at = Some(Offset {
                        dx_mm: p.x_mm,
                        dy_mm: p.y_mm,
                    });
                }
            }
            _ => {}
        }
        self.stack.push(name);
        Ok(())
    }

    fn end(&mut self) -> Result<(), CourseError> {
        let in_race_after = self.stack.len() == 3
            && self.stack[0] == "CourseData"
            && self.stack[1] == "RaceCourseData";
        let Some(name) = self.stack.pop() else {
            return Err(CourseError::Xml("unbalanced end tag".into()));
        };
        let parent = self.parent().to_string();
        let text = std::mem::take(&mut self.text);
        let text = text.trim();
        let missing = |what: &str| CourseError::Xml(format!("<{what}> closed but not open"));
        match (parent.as_str(), name.as_str()) {
            ("Event", "Name") => self.event_name = Some(text.to_string()),
            ("Map", "Scale") => {
                let v = number(text, "Scale")?;
                if v >= 1.0 && v <= f64::from(u32::MAX) {
                    self.map_scale = Some(v.round() as u32);
                }
            }
            ("Control", "Id") => {
                if let Some(c) = self.control.as_mut() {
                    c.code = bounded(text, MAX_CODE_LEN, "control code")?;
                }
            }
            ("RaceCourseData", "Control") if in_race_after => {
                let c = self.control.take().ok_or_else(|| missing("Control"))?;
                if c.code.is_empty() {
                    return Err(CourseError::Xml("a Control has no Id".into()));
                }
                if !self.codes.insert(c.code.clone()) {
                    return Err(CourseError::Xml(format!(
                        "control code {} is defined twice",
                        c.code
                    )));
                }
                self.controls.push(Control {
                    id: None,
                    code: c.code,
                    kind: c.kind,
                    position: c.position,
                    geo: c.geo,
                    description: None,
                });
            }
            ("Course", "Name") => {
                if let Some(c) = self.course.as_mut() {
                    c.course.name = bounded(text, MAX_NAME_LEN, "course name")?;
                }
            }
            ("Course", "CourseFamily") => {
                if let Some(c) = self.course.as_mut()
                    && !text.is_empty()
                {
                    c.course.family = Some(bounded(text, MAX_NAME_LEN, "course family")?);
                }
            }
            ("Course", "Length") => {
                if let Some(c) = self.course.as_mut() {
                    c.course.length_m = Some(non_negative(text, "Length")?);
                }
            }
            ("Course", "Climb") => {
                if let Some(c) = self.course.as_mut() {
                    c.course.climb_m = Some(non_negative(text, "Climb")?);
                }
            }
            ("CourseControl", "Control") => {
                if let Some(c) = self.cc.as_mut()
                    && c.codes.len() < 2
                {
                    c.codes.push(bounded(text, MAX_CODE_LEN, "control code")?);
                }
            }
            ("CourseControl", "Score") => {
                if let Some(c) = self.cc.as_mut() {
                    c.score = true;
                }
            }
            ("Course", "CourseControl") => {
                let cc = self.cc.take().ok_or_else(|| missing("CourseControl"))?;
                let c = self.course.as_mut().ok_or_else(|| missing("Course"))?;
                if cc.codes.len() > 1 {
                    c.alternatives = true;
                }
                if cc.score {
                    c.course.kind = CourseKind::Score;
                }
                if !c.alternatives {
                    let Some(code) = cc.codes.into_iter().next().filter(|s| !s.is_empty()) else {
                        return Err(CourseError::Xml("a CourseControl names no Control".into()));
                    };
                    if !c.course.stops.is_empty() {
                        c.course.legs.push(Leg {
                            bends: Vec::new(),
                            gaps: Vec::new(),
                            marked_route: false,
                        });
                    }
                    c.course.stops.push(Stop {
                        control: code,
                        kind: cc.kind,
                        number: None,
                        number_text_at: cc.text_at,
                        circle_gaps: Vec::new(),
                        map_flip: false,
                    });
                }
            }
            ("RaceCourseData", "Course") if in_race_after => {
                let c = self.course.take().ok_or_else(|| missing("Course"))?;
                if c.alternatives {
                    // A course left out must always be named, also after the list was cut short.
                    self.notes
                        .add_final(CourseNote::AlternativeControls(c.course.name));
                } else {
                    self.courses.push(c.course);
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn finish(mut self) -> Result<RawEvent, CourseError> {
        if !self.stack.is_empty() {
            return Err(CourseError::Xml("unexpected end of file".into()));
        }
        let index: std::collections::HashMap<String, usize> = self
            .controls
            .iter()
            .enumerate()
            .map(|(i, c)| (c.code.clone(), i))
            .collect();
        for course in &self.courses {
            for stop in &course.stops {
                let Some(&i) = index.get(stop.control.as_str()) else {
                    return Err(CourseError::Xml(format!(
                        "course {} uses control {}, which is not defined",
                        course.name, stop.control
                    )));
                };
                let c = &self.controls[i];
                if c.position.is_none() && c.geo.is_none() {
                    return Err(CourseError::Xml(format!(
                        "control {} has neither MapPosition nor Position",
                        c.code
                    )));
                }
            }
        }
        let mut families: Vec<(String, Vec<String>)> = Vec::new();
        for c in &self.courses {
            if let Some(f) = &c.family {
                match families.iter_mut().find(|(n, _)| n == f) {
                    Some((_, v)) => v.push(c.name.clone()),
                    None => families.push((f.clone(), vec![c.name.clone()])),
                }
            }
        }
        // Notes about the file as a whole come through even after the list was cut short.
        for (family, variants) in families {
            if variants.len() > 1 {
                self.notes
                    .add_final(CourseNote::Forked { family, variants });
            }
        }
        self.notes.add_final(CourseNote::NoDescriptions);
        Ok(CourseEvent {
            controls: self.controls,
            courses: self.courses,
            specials: Vec::new(),
            source: CourseSource {
                format: SourceFormat::IofXml,
                event_name: self.event_name,
                map_scale: self.map_scale,
                item_scaling: None,
            },
            notes: self.notes.into_vec(),
        })
    }
}
