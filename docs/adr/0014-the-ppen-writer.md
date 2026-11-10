# 0014. The .ppen writer

Date: 2026-11-10
Status: Accepted

## Context
freemapper-courseplan exports its courses to Purple Pen and exports again after edits in either program. The
file may hold more than the project knows: comments, part entries, objects made in Purple Pen. The reader is in
`freemapper-course`, so the writer has to follow it exactly.

## Decision
- The writer lives in the course crate, next to the reader (agreed with freemapper-print, 2026-11-10). It
  uses no new dependency.
- The output follows `EventDB.Save` at petergolde/PurplePen `201417f`: indented XML, no declaration, store
  order event, controls, courses, course-controls, legs, special objects. Numbers are written as `f32` in
  their shortest round-trip form without exponent. CRLF is the default; the patch takes line end, indent and
  byte order mark from the base.
- `.ppen` carries no version number, as `EventDB.Save` writes none. The gate for a base file is its shape:
  root `course-scribe-event` without attributes, top-level children only the six known elements, stores in
  load order, at most one `<event>` before all objects, UTF-8 (no NUL). Anything else is `UnknownVersion`.
- `write_ppen` writes from scratch. `write_ppen_patch` compares each field with what the reader makes of the
  base, as `f32`. An object without a changed field keeps its bytes. A changed object keeps attributes and
  elements the writer does not own. Objects the event does not mention stay and come back in
  `Written::kept`. Only `PatchOptions::delete` deletes, by id.
- Matching: normal controls by code, courses by name, other controls by reader code, then kind and position,
  special objects by kind and points. The event's ids are ignored unless `PatchOptions::ids_from_base` says
  they come from this base (the file courseplan has linked); then they also match renamed courses, recoded
  controls and moved specials, always by kind. (Claude's call: ids from another file match by accident.)
- New ids: the given id if valid (`1..=i32::MAX`) and free, else the largest plus one; past `i32::MAX` is
  `Limit`.
- Refused: what Purple Pen cannot store or would reload differently. That covers a leg or circle gaps drawn
  two ways by courses of the output or by a kept course (`SharedLeg`, `SharedCircleGaps`), a map issue point
  with more than one description box or text in column F of a control other than normal or start, lines and
  areas with fewer than 2 points, an empty language or map path, and the deletion of a control that a course of
  the output still visits. Dropped with a note: partial flagging whose point stops being a bend
  (`WriteNote::PartialFlaggingDropped`), because the model reads partial flagging as no marked route anyway.
- A rewritten object loses what the model does not hold: comments and part entries of the course, for
  example. A renamed course keeps its `name` tag with its attributes.
- Every attribute `EventDB.Load` reads without a default is written. `tests/purple_pen_strict.rs` holds the
  audit of `EventDB.Load` at `201417f` as a table (required attributes, required children, accepted values,
  store order, unique ids) and checks the fixtures and the writer's output against it, from scratch and as a
  patch. Added 2026-11-10, after Purple Pen 4.0 refused `<options>` without `description-kind`: the crate's
  reader has defaults where Purple Pen has none, so a round trip cannot catch such a gap.
- `<options>` always carries `description-kind`: `symbols` from scratch (Purple Pen's value for a new course,
  the model keeps none), the base's value in a patch. A patch adds it to a base `<options>` that lacks it even
  when the course is otherwise unchanged, so files of the 0.4.0 writer load again. (Claude's call: such a
  file never loaded in Purple Pen, so no user edit is lost.)

## Consequences
- Breaking: every `Special` variant gains `id: Option<u32>` and `CourseSource` gains `item_scaling`, so the
  writer can match specials and derive the circle gap key. Core goes to 0.4.0. The reader also refuses a
  `special-object/@id` with a bad entity now.
- Known limits: kept courses are checked along `next` only, so a patch can change a leg or gaps that only a
  branch of a kept forked course uses. Crossing points always get orientation 0. `<options>` is left out
  without print scale, climb or length; written from scratch, its `description-kind` is always `symbols`.
  The strict check does not cover `EventDB.Validate` (Purple Pen runs it only in debug builds) or the base64
  of `image-data`.
- Cost if wrong: a patch that misreads a base file rewrites the user's own Purple Pen objects. The tests patch
  fixtures with the event read from them and demand identical bytes, and the strict check stands in for Purple
  Pen's loader, but no test has run Purple Pen 4.0 itself.

Source: courseplan design §5.5 and §5.5a, the agreement with freemapper-print of 2026-11-10 (Task 1),
`src/PurplePenCore/EventDB.cs` at `201417f`.
