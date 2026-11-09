# 0013. The course crate

Date: 2026-11-09
Status: Accepted

## Context
freemapper-print reads courses, fits them to the map and places control numbers (print ADR 0032, 0033, 0038;
ADR 0039 kept these modules apart). freemapper-courseplan places numbers by the same rules and writes `.ppen`.

## Decision
- `freemapper-course` takes print's `course/` and `placement/` from `918cec2` with their behaviour unchanged,
  with tests and fixtures, under the same paths (`freemapper_course::model`, `::fit`, `::placement`). Print
  owns it. Of the suite it uses `freemapper-omap` alone; a test fails if a source names another suite crate.
- `fit` is no cargo feature. The model needs `MapCoord` for `FittedEvent`, and `SourceFormat::y_axis` returns
  fit's `YAxis`; dropping omap would take a second point type. Courseplan's wish for a model without omap is
  deferred. Its cost is small: courseplan builds quick-xml and thiserror already.
- `Control::id` and `Course::id` (`Option<u32>`) hold Purple Pen's ids for courseplan's writer; IOF XML and
  ids outside `u32` give `None`.
- `placement_from_offset` and `offset_from_placement` are print's `numbers.rs` formula: reference point the
  circle centre, offset in map mm with y down, gap in mm at nominal size. The `From` conversions between
  `NumberPlacement` and `placement::Placement` come along, since print may not implement them any more.
- `xml::{Handler, drive, local, attr, number}`, used by print's description symbol reader, and `tm` are public.
  Error `Display` stays English, with no Fluent (ADR 0010).

## Consequences
- Code that builds `Control` or `Course` literals adds `id: None`. `xml::Handler` and `attr` take quick-xml
  types, so callers use this crate's quick-xml version.
- Cost if wrong: if courseplan cannot take omap after all, the model needs its own point type.

Source: print ADR 0032, 0033, 0038, 0039; courseplan's requests of 2026-11-09; plan "core extraction stage 2".
