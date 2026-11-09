# 0013. The course crate

Date: 2026-11-09
Status: Accepted

## Context
freemapper-print reads courses, fits them to the map and places control numbers (print ADR 0032, 0033, 0038;
ADR 0039 kept these modules apart). freemapper-courseplan places numbers by the same rules and writes `.ppen`.

## Decision
- `freemapper-course` takes print's `course/` and `placement/` from `918cec2` with their behaviour unchanged,
  same paths, all their tests (`fit_courses.rs` with the Lübeck map and `luebeck-georef.xml` too). Print owns
  it. Of the suite it uses `freemapper-omap` alone; a test fails if a source names another suite crate.
- The cargo feature `fit`, on by default, holds `fit`, `tm`, `FittedEvent` and `SourceFormat::y_axis`, the
  only users of omap. Without it the model, the readers and the placement build without omap, for courseplan.
- `Control::id` and `Course::id` (`Option<u32>`) hold Purple Pen's ids for courseplan's writer; IOF XML and
  ids outside `u32` give `None`. Error `Display` stays English, with no Fluent (ADR 0010).
- `placement_from_offset` and `offset_from_placement` are print's `numbers.rs` formula: reference point the
  circle centre, offset in map mm with y down, gap in mm at nominal size. A `.ppen` stores `number-location`
  with y up, so a writer negates `dy_mm` of the result (and a reader of a raw file before the call).
- The `From` conversions between `NumberPlacement` and `placement::Placement` come along, since print may not
  implement them any more. `xml::{Handler, drive, local, attr, number}` are public for print's description
  symbol reader; `tm` stays private, as UTM belongs to the planned `freemapper-georef`.

## Consequences
- Code that builds `Control` or `Course` literals adds `id: None`. `xml::Handler` and `attr` take quick-xml
  types, so callers use this crate's quick-xml version. The repository carries a 1 MB map for the fit tests.
- Cost if wrong: a writer that forgets the sign of `dy_mm` mirrors every number about its circle's x axis.

Source: print ADR 0032, 0033, 0038, 0039; courseplan's requests of 2026-11-09; plan "core extraction stage 2".
