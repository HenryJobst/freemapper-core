# 0046. Paper frames in the contract

Date: 2026-11-27
Status: Accepted

## Context
freemapper now lets a project keep paper frames (freemapper issue 343). A frame is a sheet of A6, A5, A4, A3
or A2, landscape or portrait, centred on a point of the map. The export writes them as a template map, `templates/paper-frame.omap`, with
one closed line per frame. freemapper-print will offer a layout per frame, and freemapper-courseplan may show
them. The readers depend on the tags of these lines, so the words belong in the contract (ADR 0018, ADR 0027).
A template map is also an `.omap` with the map's georeferencing and symbol set, so a reader could take it for
the map itself.

## Decision
- `tags` reserves the words: the source `SOURCE_PAPER_FRAME` (`paper-frame`), the keys `PAPER_KEY`
  (`freemapper:paper`) and `ORIENTATION_KEY` (`freemapper:orientation`), their values `PAPERS` (`A6`, `A5`,
  `A4`, `A3`, `A2`) and `ORIENTATIONS` (`ORIENTATION_LANDSCAPE`, `ORIENTATION_PORTRAIT`), and the file
  `PAPER_FRAME_FILE` (`templates/paper-frame.omap`, relative to the folder of the map). The values are plain
  words and never translated.
- A frame is a closed line in map coordinates. It is the whole sheet at the scale of the map, axis-aligned,
  and its middle is the centre of the sheet. A reader takes the frames by their source tag. The part name `paper frames`
  and the label `freemapper:name` are freemapper's own and not reserved.
- `freemapper.toml` keeps the frames in `paper_frames` as freemapper's own record. That key does not join the
  contract keys; a reader reads the template map.
- `check` gets the rule `NoPaperFrameInMap`. A frame never lies in the map, so a template map given to `check`
  as a map breaks the rule once per frame and never passes for one.
- The new function `check_paper_frames(frames_xml, omap_xml)` has four rules: every object of the template is a
  frame (`PaperFrameFileOnlyFrames`), a frame carries both tags with a reserved value (`PaperFrameTags`), it
  is a closed line (`PaperFrameClosedLine`), and the template has the map's georeferencing
  (`PaperFrameGeorefOfMap`), so its coordinates are those of the map.
- `ObjectClass` gets no class for frames. In a map they are a broken rule, and in the template every object
  is one.
- The fifth fixture `paper-frames` is `sprint-4000` with an A4 portrait and an A5 landscape frame. `Fixture`
  gets the field `paper_frame_omap`, `write_into` puts the template where a project folder has it, and
  `Expected` gets `paper_frames` (paper, orientation and box of each frame, in file order), so a reader's
  `contract_*` tests compare what it finds with those values.

## Consequences
- The readers' contract tests run over the new fixture without a change: they read the map and ignore the
  template. A reader that uses the frames adds a test against `Expected::paper_frames`.
- freemapper can take the words from `tags` instead of its own constants and run `check_paper_frames` on the
  template it writes.
- This change builds the fixture by hand from `sprint-4000`, following the frame code of freemapper's branch
  for issue 343, because the producer of `sprint-4000` (0.9.2) had no frames. Once freemapper's
  `contract_write_fixtures` writes a project with frames, it refreshes this fixture with the others.
- `Rule`, `Fixture` and `Expected` grow. Code that matches every `Rule` or builds a `Fixture` or `Expected`
  by hand must add the new names. No app does either today.
- A paper beyond A2 or a third orientation is a new reserved word, so a reader that rejects unknown values
  must follow first.
- The mutation gate (ADR 0040) ran without features, so it could not test the changed `Fixture::write_into`
  behind `fixtures`. `.cargo/mutants.toml` now sets `all_features = true`, as the test run has
  `--all-features`.

Source: freemapper issue 343, `docs/superpowers/specs/2026-11-27-paper-frames-design.md` in freemapper
("freemapper-print uses the frames"); ADR 0018, ADR 0021, ADR 0027.
