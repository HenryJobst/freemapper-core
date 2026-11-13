# Changes

[Deutsche Version](CHANGELOG.md)

This file lists all notable changes to freemapper-core, newest first, each with the library it affects. The
format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Added

- freemapper-contract: the sidecar can name the contour interval, as the table `[contours]` with `interval_m` (decision 0018). freemapper writes it only when it is not the kind's, for example 2.5 m for a forest map in flat land. `Sidecar::contour_interval_m` gives it in metres, else `None`, and then the kind's interval applies. A value that is no positive number counts as absent. `CONTRACT_KEYS` has five keys instead of four for it; `Sidecar` has the field `contour_interval_mm`.

## [0.24.0] - 2026-11-13

### Changed

- freemapper-render: `write_pdf` and `MultiPagePdf::add_page` write paths compactly (decision 0032). Path coordinates are rounded to 1/1000 point. A small fill or stroke, four points or more within a square of 7 mm such as the dot of a point symbol or a control circle, is written relative to its first point, so repeated symbols give the same numbers and deflate stores them once. freemapper-print's 13 example sheets of the Gleisdreieck and Rummelsburg sprint maps shrink by 76 to 82 %, from 25.8 MB to 5.37 MB together; training sheet A goes from 2.6 MB to 533 KB. Large pages are written in about a third of the time. The PDF still holds every point; Béziers fitted to the polylines were measured and left out, because they saved only 8 % more and poppler drew small dots as diamonds. Rasterised directly at 150 dpi with poppler, 66 of 41 million pixels of the 13 sheets differ by more than 32 of 255 levels, where a thin hatch line lands one pixel column over. Texts, images, `RenderedPage` and the SVG backend stay the same. Apps change no code; an app that compares committed PDFs writes them again.

## [0.23.0] - 2026-11-13

### Changed

- freemapper-contract: the map kind `forest` has drawing scales, 1:7500, 1:10 000 and 1:15 000 (ISOM 2017-2), so freemapper writes it (`is_written_by_freemapper`). A reader accepts forest maps at these scales now; before, `check` reported them as an unknown kind. freemapper-print and freemapper-courseplan need no code change.

## [0.22.0] - 2026-11-12

### Changed

- freemapper-render: `write_pdf` and `MultiPagePdf::add_page` no longer write map objects and course overprint that lie wholly outside the visible frame (decision 0031). Sheets whose frame shows part of the map get smaller. freemapper-print's example sheets of a 1.4 MB sprint map at 1:4000 shrink by 15 to 30 %, from 3.6 or 3.7 MB to between 2.6 and 3.2 MB; a sheet that shows one corner of the map goes from 252 KB to 38 KB. Rasterised, the PDFs match the old ones pixel for pixel (pdftoppm, 6 to 300 dpi); the bytes change. A shape is written when its ink box may reach the frame: its points with half the line width, miter tips, square caps and glyph outlines, plus 5 mm, because anti-aliasing at the frame mixes shapes just outside it into the edge pixels. `RenderedPage`, `BuildReport` and the hits stay the same. Missing glyphs and numbers a PDF cannot hold are still reported for shapes that are left out. The SVG backend leaves out by the same rule and so keeps a few more shapes at the edge of the view. Apps change no code; an app that compares committed PDFs writes them again.

## [0.21.1] - 2026-11-11

### Added

- freemapper-web: `suite.css` has the token `--on-accent` for text on a surface filled with `--accent` (light `#ffffff`, dark `#1c1c1e`) and the class `button.primary` for the main action of a view (decision 0030, addendum).

## [0.21.0] - 2026-11-11

### Added

- freemapper-web: the suite page frame (decision 0030). `serve` serves five shared files below `/{token}/suite/`, behind the same checks and headers as the app's routes: `suite.css` with the tokens, light and dark (accent `#0b6bcb`, dark `#6cb0ff`), and the base elements, and the modules `api.js`, `texts.js`, `status.js` and `tree.js` from freemapper-courseplan's editor. Any other name below `suite/` gets 404. `revision` computes the revision of a file (FNV-1a 64, 16 hex digits), `REVISION_HEADER` is `x-freemapper-revision`. `stale` answers 409 with `{ "error", "revision" }`, `refused` 422 with one sentence as plain text. `texts_language` gives the page texts the fixed language, else `de` or `en` from `Accept-Language` by q-value, else English. With the feature `page-rules`, `check_page_rules` checks a page's files for inline scripts, inline handlers, `javascript:` URLs, sources from the network and size; `suite_assets` hands the suite's files to that check.

## [0.20.0] - 2026-11-11

### Added

- freemapper-render: `PageSpec::page_mm` sets a page of any size from 10 mm to 5080 mm a side instead of A4 or A3 (decision 0029). A size outside that range is `BuildError::PageSize`; nothing is clamped. `BuildReport::source_objects` gives each object number a hit carries its position in the map, which differs from the number once an object is skipped.

## [0.19.0] - 2026-11-11

### Changed

- freemapper-printrules: `scale::MapKind` is the type of `freemapper-contract` now (decision 0028), one type for the suite. `MapKind::allowed_scales()` and `MapKind::proposed_contour_interval_m()` become the functions `scale::allowed_scales(kind)` and `scale::proposed_contour_interval_m(kind)`. A test checks that every scale freemapper draws is in the profile's table.

## [0.18.0] - 2026-11-11

### Added

- freemapper-contract: shared types for the three apps (decision 0027, stage 4 of the contract test). `MapKind` is the one definition of the map kind (`sprint`, `schoolyard`, `forest`) with `name`, `from_name`, `base_scale` and `drawing_scales`, the scales freemapper writes a kind at. `Sidecar::parse` and `Sidecar::next_to` read the contract keys of `freemapper.toml`; an unknown map kind comes back as `SidecarKind::Unknown` with its name and is no error. `ObjectClass::of` sorts the tags of an object into an object class. The module `tags` holds the reserved words: tag keys, sources, the role `scale-bar`, the names of the two parts, the sidecar's file name and its contract keys.

### Changed

- freemapper-contract: census, vocabulary and rules sit behind the feature `check`, which is on by default; `fixtures` turns it on. With `default-features = false` the crate holds the shared types only and does not need `freemapper-omap`. The rule `ScaleAllowed` reads the scales from `MapKind::drawing_scales`. A number in the sidecar that does not fit `u32`, such as a negative scale, counts as missing.

## [0.17.0] - 2026-11-11

### Added

- freemapper-update: new library for the apps' self-update from signed releases (decision 0026). It holds the manifest, the minisign check against a key list, versions, the `check_url` host rule, check record and mode, swapping the running executable with a way back and clean-up, and `install` with verification before the swap. An install lock holding its holder's token keeps two installs apart; one that loses it to a second install after 15 minutes stops before the swap. A check's outcome gives version, release date, notes link and download size. `ProcessRunner::new(app.name)` expects the app's name from `--version`, also when the executable was renamed. It has no network access: the apps bring it through `UpdateSource`. No release build accepts the test keys under `tests/keys`.

## [0.16.1] - 2026-11-11

### Changed

- freemapper-course: the shared placement cases (`tests/fixtures/placement/cases.toml`) now assume the number box the renderer draws: 0.8083747 × height per digit (the Arimo advance, checked by a test in freemapper-render) instead of 0.6 × height. The expected values of `numbers_avoid_each_other` and `mixed_course_4` change. `place_course_numbers` itself is unchanged; it takes the box size from the caller.

## [0.16.0] - 2026-11-11

### Added

- freemapper-printrules: the print rules from freemapper-print, unchanged in behaviour (decision 0025): the profile `iof-print`, the rule set, the map kinds and the symbol size table by print scale. `scale::course_symbols(kind, print_scale)` gives the course symbol sizes and the course factor for `freemapper_render::overprint::overprint`. With the feature `test-support`, `rules::load_for_tests` and `rules::embedded_text` are public.

## [0.15.0] - 2026-11-11

### Added

- freemapper-render: the course overprint from freemapper-print, unchanged in behaviour (decision 0024). `overprint::overprint` draws one course (start, controls, finish, crossing points, legs, special objects, control numbers) as the lower and upper purple of a `RenderedPage`, in the size the caller gives; `overprint::on_map` places the lower layer among the map colours. `CourseSymbolSizes` moves along. The crate now depends on `freemapper-course`.

## [0.14.1] - 2026-11-11

### Fixed

- freemapper-render: Bézier curves are flattened only up to one million points per path. One curve can need up to 65,536 points, so a crafted map could multiply the work of a page build before the page budget applies. Once the limit is reached, the remaining curves of the path become one chord each. Ordinary maps are unchanged.

## [0.14.0] - 2026-11-11

### Added

- freemapper-course: `description_symbols::ids(standard)` lists every symbol id of a description standard in file order, each once. `symbol_kind` gives the column for each.

## [0.13.0] - 2026-11-11

### Added

- freemapper-render: the page builder from freemapper-print, unchanged in behaviour (decision 0023). `build::build_page` turns a `MapDocument` into the `RenderedPage` the PDF gets, plus a `BuildReport` with the page fields of print's `PrintReport` under the same names. `PageSpec` takes the symbol factor and the geometry factor as numbers; the scale rules (map kind, allowed scales, factors) stay in print. `Paper`, `Orientation`, `PAGE_MARGIN_MM`, `MAX_UNITS_PER_PAGE`, `frame_box`, the tags `SHEET_TAG`, `SCALE_BAR_TAG`, `BASE_TAG`, the map part `BASE_PART` and `is_sheet_object`, `is_base_object`, `is_north_line` move along with it. `build_page_within` takes another page budget, for tests. The crate now depends on `serde`.

## [0.12.0] - 2026-11-11

### Added

- freemapper-render: SVG backend for the browser editors (decision 0022). `svg::write_svg` writes a `RenderedPage` as SVG in the same order as the PDF, with masks as clip paths and the course overprint with `mix-blend-mode:multiply`. The viewBox is in page mm; `SvgView` picks the part of the page and the pixels per mm, and shapes outside it are left out. Texts are Arimo glyph outlines, images PNG data URIs, and colours go from CMYK to sRGB by r = 255 · (1 − c) · (1 − k). A page beyond `svg::MAX_UNITS` gives `SvgError::TooLarge`.
- freemapper-render: `RenderedPage::mapping` returns the page mapping `PageMapping` between the map (mm at the print scale) and the page (mm). `RenderedPage::hits_at` lists the map objects under a page point, topmost first, and respects the clip rectangle, the masks and the fill rules (decision 0022).
- freemapper-contract: when the crate or freemapper-omap changes, CI now runs the contract tests (`contract_*`) of freemapper-print and freemapper-courseplan against the new state (decision 0022). A red run means the change breaks a reader; the reader then learns the new form first, and core is released after that.

## [0.11.0] - 2026-11-11

### Added

- freemapper-web: new crate with the shell for local web pages (decision 0020), generalised from the area selection of freemapper. `serve` binds 127.0.0.1 (port 0 picks a free one), puts a random 128-bit token in front of the app's routes and guards them: only the hosts `127.0.0.1:PORT` and `localhost:PORT` are served, an `Origin` that is sent must be the page itself, and every response carries `Cache-Control: no-store` (unless the handler sets its own), the app's Content-Security-Policy, `X-Frame-Options: DENY`, `Referrer-Policy: no-referrer` and `X-Content-Type-Options: nosniff`. With `ServeOptions::strict_post`, a request that changes state also needs an `Origin` and the content type `application/json` or `application/octet-stream`. `embedded` serves rust-embed files, `open_browser` starts the browser, `LocalServer::shutdown` and `finished` stop and await the server.

## [0.10.0] - 2026-11-10

### Changed

- freemapper-contract: `Expected::title` is now `Option<String>`, because maps from freemapper 0.9.0 have no title in the sidecar file. Reader tests compare with `expected.title.as_deref()`. `Expected::map_kind` can be `unknown`; a reader test that looks the kind up must handle that case. New: `Expected::breaks` with the rules a fixture breaks on purpose.

### Added

- freemapper-contract: two fixtures for the readers (decision 0018, addendum). `legacy-0.9.0-scale-bar` is a map from freemapper 0.9.0 with the old tag on the scale bar and without a title. `future-kind` has a kind, a key and a role no reader knows. `fixtures/README.md` names the source of each fixture.

## [0.9.0] - 2026-11-10

### Added

- freemapper-render: new crate with the renderer of freemapper-print, moved over with its behaviour unchanged (decision 0019). It draws OOM symbols into the render page model (`page`), writes PDF (`pdf`) and carries the Arimo fonts with their text metrics (`fonts`). Its only suite dependency is freemapper-omap. The feature `test-support` exposes `pdf::content_of`, `draw::work_units` and `draw::BUDGET_SLACK` for the apps' tests. The page builder (`print.rs`) stays in freemapper-print for now.

## [0.8.0] - 2026-11-10

### Added

- freemapper-contract: new crate with the contract between freemapper and its readers freemapper-print and freemapper-courseplan (decision 0018). `census` counts the objects of a map per object class (sheet objects, base area, north lines, content) and returns the boxes of content and of sheet objects. `Vocabulary::of` collects the words of a map and its sidecar file, and `check` tests twelve rules for freemapper's output. Map and sidecar file go in as text; no type of freemapper-omap is part of the API.
- freemapper-contract: the feature `fixtures` embeds two maps written by freemapper 0.9.2, `schoolyard-1000` and `sprint-4000`. Each comes with `expected.toml` and `vocabulary.toml`. An app lists the crate under `[dev-dependencies]` with this feature. `Fixture::write_into` puts map and sidecar file into a folder, and `Fixture::expected` says what a reader must derive from them.

## [0.7.0] - 2026-11-10

### Added

- freemapper-course: `MapGrid` (feature `fit`) converts between a map position (mm, x right, y down) and latitude and longitude: `MapGrid::of(&MapGeoref)`, `latlon_of`, `map_mm_of`. The fit uses the same conversion.

## [0.6.1] - 2026-11-10

### Fixed

- freemapper-course: `placement::candidate` wraps directions above 360° before rounding, not after. Newly placed directions carry no float noise any more (102.4 instead of 102.39999999999998).

## [0.6.0] - 2026-11-10

### Added

- freemapper-course: `placement::place_course_numbers` places all numbers of a course in running order. The
  search comes from `auto_place` in freemapper-courseplan (its decisions 0030 to 0032). Kept numbers
  (`NumberStop::kept`) stay where they are, and the others keep clear of them. Each placed number blocks the
  ones after it, and the circles and legs of the course count as overprint. The map obstacles go into one grid
  per course. Also added are `CourseNumbers`, `NumberStop`, `NO_LEGS_DIRECTION_DEG` (45 degrees at a control
  without legs), `Rect::UNBOUNDED` and `GapRange::strictest`, which combines several `number_gap` rules into
  the largest minimum and the smallest maximum. The existing items of `placement` stay as they are. Decision
  0016.
- freemapper-course: The module `description_symbols` holds the IOF control description symbols with their
  drawings and German and English texts from Purple Pen's `symbols.xml` (BSD licence). It comes from
  freemapper-print (`describe::symbols` there), and the file sits unchanged in `data/`. Compared with there it
  has `TextLang` instead of `SheetLang`, `symbol_kind` (the column a symbol belongs in) and `exists`. The
  module also maps between Purple Pen ids and IOF 2024 numbers. `iof_2024` is courseplan's forward mapping.
  `ppen_symbol` and `ppen_columns` go back and return only ids that the standard has and that Purple Pen
  offers in that column. They swap the cardinal and diagonal forms of 0.1 and 0.2, and write plain 9.1 and 9.3
  as `high` with a note. They refuse symbols a 2004 file lacks (2.10, 15.6, 5.23 and 5.24 of 2024) and put
  `single` into column F (`10.1single`). "Between" (12.14) keeps D and E and gets the plain `11.15`. Decision
  0017.

## [0.5.2] - 2026-11-10

### Added

- freemapper-georef: `TransverseMercator::inverse` converts easting and northing back to latitude and longitude for any UTM zone, both hemispheres and any ellipsoid (Krüger series to n^6). A round trip through `forward` is off by less than 1e-9 degrees (about 0.1 mm).

### Changed

- freemapper-georef: `Crs::inverse` uses the new inverse instead of the Newton loop over `forward`. Results change by less than 1e-9 degrees; points far outside the zone still give `None`.

## [0.5.1] - 2026-11-10

### Changed

- freemapper-course: the fit uses the UTM conversion from freemapper-georef; the private `tm` module is gone. Results stay the same.

## [0.5.0] - 2026-11-10

### Added

- freemapper-georef: new library (decision 0015). `Crs`, `LatLon`, `ProjectedPoint`, `BoundingBox` and
  `TileId` from freemapper, unchanged in names, behaviour and tests. Also `tm`, the suite's only UTM
  computation: Krüger series to n^6 on any ellipsoid with convergence and scale factor, and `utm_from_crs`
  for EPSG codes and PROJ texts. `Crs::utm_zone` and `Crs::transverse_mercator` tie the two together.
  freemapper-course is unchanged; freemapper-print switches `fit` to this library and then deletes its `tm`.

## [0.4.2] - 2026-11-10

### Fixed

- freemapper-course: Purple Pen 4.0 loaded a `.ppen` from the writer and crashed right after, because the
  finish had no description symbol. After loading, Purple Pen opens the All Controls view and looks up the
  symbol of every finish, crossing point and map issue point there. Without one, `SymbolDB` throws an
  `ArgumentNullException`. `write_ppen` now writes `14.3` for a finish without a symbol, `13.3` for a
  crossing point and `13.6` for a map issue point, the symbols Purple Pen gives a new control. A symbol the
  event holds always stays. `write_ppen_patch` keeps the base file's symbol when the event gives none, and
  adds the missing symbol also to controls the event does not mention, so Purple Pen opens a file written by
  0.4.1 again after the next patch. The writer also refuses what Purple Pen fails on after the load: a finish
  symbol other than `14.1` to `14.3`, a crossing point symbol other than `13.3` and `13.4`, any symbol but
  `13.6` at a map issue point, and a combination symbol such as `10.1` in column C or G. The test against
  Purple Pen's loader now also checks the symbols against Purple Pen's symbol table, per column and
  description standard, and that every reference between controls, course-controls, legs, courses and
  special objects resolves.

## [0.4.1] - 2026-11-10

### Fixed

- freemapper-course: Purple Pen 4.0 refused a `.ppen` from the writer as soon as a course had climb, length
  or a print scale. The course's `<options>` lacked `description-kind`, which Purple Pen reads without a
  default. `write_ppen` now writes it in every `<options>`, as `symbols`, Purple Pen's value for a new course.
  `write_ppen_patch` keeps the base file's value and adds one where it is missing, also when nothing else in
  the course changed, so Purple Pen loads a file written by 0.4.0 again after the next patch. A new test
  checks the writer's output against the attributes, child elements and values without which Purple Pen's
  `EventDB.Load` refuses a file.

## [0.4.0] - 2026-11-10

### Changed

- freemapper-course: every variant of `Special` has a new first field `id: Option<u32>`, and `CourseSource`
  has the field `item_scaling: Option<ItemScaling>`. Apps that build `Special` variants or `CourseSource`, or
  destructure them without `..`, add `id: None` and `item_scaling: None`; code that destructures `Special`
  with `..` needs no change. For freemapper-print this means literals gain `id: None` and
  `item_scaling: None`. The reader also refuses a `special-object/@id` with a bad entity now, where it used to
  ignore it. The library therefore moves to 0.4.0.

### Added

- freemapper-course: `Special::id` returns the id from `special-object/@id` of the `.ppen` (`None` for an id
  outside `u32`), and `Special::courses` returns the course filter. `ItemScaling` (so far private to the
  reader) is public, `CourseSource::item_scaling` holds the file's setting (`None` for IOF XML), and
  `course_obj_ratio` computes the course object ratio from scaling, map scale and print scale. The reader
  calls the same function. freemapper-courseplan's `.ppen` writer needs this.
- freemapper-course: `write_ppen` writes an event as a new `.ppen` file, laid out as Purple Pen writes it
  (module `ppen::write`). `ScratchOptions` sets the map (`PpenMap`), the map and description standards
  (`MapStandard`, `DescriptionStandard`), the language of the descriptions and the line end (`LineEnding`,
  default CRLF, which Purple Pen writes on Windows). The result `Written` holds the file text, the id each
  object got (`AssignedIds`) and notes (`WriteNote`). Objects keep their ids; a missing or repeated id becomes
  the largest id of its store plus one. The writer checks the event against the reader's limits first and
  refuses with `PpenWriteError` what Purple Pen cannot store, for example two courses that draw the same leg
  differently. Decision 0014.
- freemapper-course: `write_ppen_patch` writes an event into an existing `.ppen`, the base file. The patch
  matches controls and courses by code or name. It uses the event's ids only if `PatchOptions::ids_from_base`
  says they come from this base file, as with the file a course plan is linked to; then a renamed course or a
  control with a new code stays the same object. By default it ignores every id, so an event read from another
  file cannot take over objects through ids that happen to coincide. An object with no changed field keeps its
  bytes, so patching a file with the event the reader read from it gives back that file. Changed fields are
  written anew, and attributes and elements the writer does not know stay where they are. Only what
  `Deletions` in `PatchOptions` names by id is deleted; `Written::kept` lists the objects of the base file the
  event does not mention. The patch refuses to change a leg or circle gaps that a course outside the event
  also draws. Partial flagging whose point is no longer a bend of the leg is dropped and reported as
  `WriteNote::PartialFlaggingDropped`. Known limits: the patch does not check the branches of a kept forked
  course, crossing points always get orientation 0, and `options` is left out without a print scale, climb
  and length. Decision 0014.

## [0.3.0] - 2026-11-10

### Added

- freemapper-omap: map model and reader for OpenOrienteering Mapper maps (`.omap`, `.xmap`), with the
  embedded symbol sets ISSprOM 2019 and school map 2019 and the sheet symbols 991–994. Moved from
  freemapper-print (commit `918cec2`) with the same paths, the same behaviour and the same `OmapError` texts,
  including the nesting limit `MAX_XML_DEPTH`. `too_deep`, `symbol_set::attribute` and
  `SymbolSet::without_codes` are public now, because freemapper-print's renderer and tests need them.
  Decision 0012.
- freemapper-course: course model, readers for Purple Pen (`.ppen`) and IOF XML 3.0, fitting a course file to
  the map, and control number placement. Moved from freemapper-print (commit `918cec2`, modules `course/` and
  `placement/`) with the same paths, the same behaviour, the same reader limits and the same `CourseError` and
  `FitError` texts. New are `Control::id` and `Course::id`, holding the ids from the `.ppen` (`None` for IOF
  XML), and `placement_from_offset` and `offset_from_placement`, which turn a number offset into a number
  placement and back. The parts of `xml` that freemapper-print's description symbol reader needs are public
  now. Code that builds `Control` or `Course` itself adds `id: None`. The fit sits behind the feature `fit`
  (on by default); without it the crate builds without `freemapper-omap`. Decision 0013.

## [0.2.0] - 2026-11-09

### Added

- freemapper-rules: rule profiles for checks as TOML files, read and merged into a rule set. The app describes
  in a `Schema` which rule keys, value names, units, tables and filters exist. Moved from
  freemapper-courseplan 0.4.0 with the same file format, the same merge and the same `RulesError` texts. From
  freemapper-print come value names per key, range checks of numbers by unit, keys that take no filter, the
  app's own checks for tables and values, and `RuleSet::pick`, which stops with the new error `Ambiguous` when
  more than one rule matches. Filters are now named by their field (`map_kinds`, `disciplines`, `print_scales`) instead of
  being fixed fields of a rule. A rule that cites the program's own proposals needs a note. Decision 0011.

### Changed

- freemapper-text 0.2.0 no longer has the trait `Localize`. This breaks code written for 0.1.0: an app that
  imports it defines the trait itself, as freemapper-courseplan and freemapper-print already do.

### Removed

- freemapper-text: the trait `Localize`. No app could use it, because the orphan rule forbids implementing it
  for error types from a third crate. The apps keep their own trait; code that imports
  `freemapper_text::Localize` defines it itself (amendment to decision 0010).

## [0.1.0] - 2026-11-09

### Added

- Skeleton: workspace, CI on Linux, macOS and Windows, decisions 0001 to 0007.
- freemapper-text: program texts from Fluent catalogs. The app brings its own `.ftl` files; the library detects
  the language (`--lang`, `FREEMAPPER_LANG`, `LC_ALL`, `LC_MESSAGES`, `LANG`), looks texts up with a fallback
  to English and checks catalogs with `Catalog::problems`. Moved from freemapper-courseplan 0.4.0 with the
  same behaviour; decision 0010.
- freemapper-fs: writes, copies and removals inside the project folder that never follow a link. Each file is
  written as a new partial file and then renamed over the target, so a link at the target is replaced and its
  target stays untouched. A path whose folder leads out of the project folder through a link is refused. Copies
  read only files of the project itself. Moved from freemapper 0.6.2 with the same behaviour, except that the
  error texts name no app and `remove_stale_parts_inside` returns a refused folder as an error instead of
  printing a warning itself. Decisions 0008 and 0009 come along (formerly freemapper 0059 and 0060).
